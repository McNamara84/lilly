"""Deterministic collection-search data and an independent in-memory result oracle."""

from datetime import date, timedelta
from functools import lru_cache
import hashlib
import json
import re
import unicodedata

VERSION = 2
OWN_ENTRIES = 10_000
FOREIGN_ENTRIES = 100_000
SERIES = {1: ("Maddrax fixture", "maddrax"), 2: ("John Sinclair fixture", "john-sinclair")}
AUTHORS = ("Anna Müller", "Béla Kühn", "Clara Beispiel", "David Zybell")
SORTS = ("issue_number", "series", "condition", "title", "author", "added")


@lru_cache(maxsize=16384)
def text_key(value):
    """Model the fixture's Unicode-CI comparisons without calling the database."""
    return "".join(c for c in unicodedata.normalize("NFKD", value.casefold())
                   if not unicodedata.combining(c))


def contains(value, pattern):
    pattern = text_key(pattern.strip())
    expression = "".join(".*" if c == "%" else "." if c == "_" else re.escape(c)
                         for c in pattern)
    return re.search(expression, text_key(value), re.DOTALL) is not None


def issues():
    result = []
    for identifier in range(1, 7001):
        series_id = 1 if identifier <= 3500 else 2
        number = (identifier - 1) % 3500 + 1
        name, slug = SERIES[series_id]
        prefix = "Über den Cafébergen" if identifier % 11 == 0 else "Abenteuer"
        title = f"{prefix} {number:04d}"
        if identifier == 42:
            title = "Seltene Äonen 0042"
        authors = [AUTHORS[identifier % len(AUTHORS)]]
        if identifier % 7 == 0:
            authors.append(AUTHORS[(identifier + 1) % len(AUTHORS)])
        result.append(dict(issue_id=identifier, issue_number=number, title=title,
                           series_id=series_id, series_name=name, series_slug=slug,
                           authors=authors))
    return result


def entries(catalogue):
    result = []
    # Interleave series, leaving 1,000 unrecorded issues in each for missing searches.
    for index in range(5000):
        issue = catalogue[index // 2 + (3500 if index % 2 else 0)]
        copies = 1 if index < 1000 else 2 if index < 4000 else 3
        for copy in range(1, copies + 1):
            identifier = len(result) + 1
            status = ("owned", "duplicate", "wanted")[index % 3]
            condition = None if status == "wanted" and index % 2 else f"Z{index % 5}"
            created = date(2025, 1, 1) + timedelta(days=index % 365)
            result.append({**issue, "id": identifier, "copy_number": copy,
                           "edition_label": f"Edition {copy}", "condition_grade": condition,
                           "status": status, "notes": "Fixture note " + "x" * (index % 160),
                           "created_at": f"{created.isoformat()}T12:00:00"})
    assert len(result) == OWN_ENTRIES
    return result


def scenarios():
    cases = [("all", {}), ("full-page", {"per_page": 100})]
    cases += [(f"series-{slug}", {"series_slug": slug}) for _, slug in SERIES.values()]
    cases += [(f"status-{s}", {"status": s}) for s in ("owned", "duplicate", "wanted")]
    cases += [(f"condition-Z{i}", {"condition": f"Z{i}"}) for i in range(5)]
    cases += [("issue-number", {"issue_number": 42}), ("issue-id", {"issue_id": 42}),
              ("condition-range", {"condition_min": "Z1", "condition_max": "Z3"})]
    for field, values in (("title", ("Abenteuer", "Seltene", "does-not-exist", "cafe")),
                          ("author", ("muller", "Zybell", "does-not-exist", "Kühn")),
                          ("q", ("Abenteuer", "Zybell", "does-not-exist"))):
        cases += [(f"{field}-{i}", {field: value}) for i, value in enumerate(values)]
    cases += [(f"sort-{field}-{direction}", {"sort": field, "sort_dir": direction})
              for field in SORTS for direction in ("asc", "desc")]
    cases += [("combined-status", {"series_slug": "maddrax", "status": "owned", "condition": "Z1"}),
              ("combined-title", {"series_slug": "john-sinclair", "title": "Abenteuer"}),
              ("combined-author", {"series_slug": "maddrax", "author": "muller"}),
              ("combined-grade", {"author": "Kühn", "condition": "Z2"}),
              ("search-sort-author", {"q": "Abenteuer", "sort": "author"})]
    for page in (2, 100, 200, 201):
        cases.append((f"page-{page}", {"page": page}))
    cases += [("author-last-page", {"sort": "author", "page": 200}),
              ("page-size-cap", {"per_page": 500}),
              ("page-normalization", {"page": 0, "per_page": 0}),
              ("offset-cap", {"page": 4_294_967_295, "per_page": 100})]
    for slug in ("maddrax", "john-sinclair"):
        cases.append((f"missing-{slug}", {"series_slug": slug, "status": "missing"}))
    cases += [("missing-title", {"series_slug": "maddrax", "status": "missing", "title": "cafe"}),
              ("missing-author", {"series_slug": "maddrax", "status": "missing", "author": "muller"})]
    cases += [(f"missing-sort-{field}-{direction}",
               {"series_slug": "maddrax", "status": "missing", "sort": field, "sort_dir": direction})
              for field in ("issue_number", "series", "title", "author")
              for direction in ("asc", "desc")]
    return [(name, {str(k): str(v) for k, v in params.items()}) for name, params in cases]


def expected(catalogue, collection, params):
    missing = params.get("status") == "missing"
    if missing:
        owned = {row["issue_id"] for row in collection}
        rows = [{**issue, "id": 0, "copy_number": None, "edition_label": None,
                 "condition_grade": None, "status": "missing", "notes": None, "created_at": None}
                for issue in catalogue if issue["issue_id"] not in owned]
    else:
        rows = collection[:]
    for field in ("series_slug", "status", "condition_grade", "issue_number", "issue_id"):
        param = "condition" if field == "condition_grade" else field
        if param in params:
            rows = [row for row in rows if str(row[field]) == params[param]]
    if "condition_min" in params and "condition_max" in params:
        rows = [r for r in rows if r["condition_grade"] is not None
                and params["condition_min"] <= r["condition_grade"] <= params["condition_max"]]
    for field in ("title", "author", "q"):
        if field in params and params[field].strip():
            def matches(row):
                values = ([row["title"]] if field == "title" else row["authors"] if field == "author"
                          else [row["title"], *row["authors"]])
                return any(contains(value, params[field]) for value in values)
            rows = [row for row in rows if matches(row)]
    total = len(rows)
    rows.sort(key=lambda r: (text_key(r["series_name"]), r["issue_number"],
                             r["issue_id"] if missing else r["id"]))
    field = params.get("sort", "issue_number")
    def sort_value(row):
        if field == "series":
            return text_key(row["series_name"])
        if field == "condition":
            return 0 if row["condition_grade"] is None else int(row["condition_grade"][1]) + 1
        if field == "author":
            return min(map(text_key, row["authors"]), default="")
        if field == "added":
            return row["created_at"]
        return text_key(row[field]) if field == "title" else row[field]
    rows.sort(key=sort_value, reverse=params.get("sort_dir") == "desc")
    page = max(1, int(params.get("page", 1)))
    per_page = min(100, max(1, int(params.get("per_page", 50))))
    offset = min(1_000_000, (page - 1) * per_page)
    fields = ("id", "issue_id", "issue_number", "title", "series_id", "series_name", "series_slug",
              "copy_number", "edition_label", "condition_grade", "status", "notes", "created_at")
    return dict(page=page, per_page=per_page, total=total,
                data=[{key: row[key] for key in fields} for row in rows[offset:offset + per_page]])


def validate_response(payload, reference):
    if not isinstance(payload, dict):
        raise ValueError("Response must be an object")
    for field in ("page", "per_page", "total"):
        if type(payload.get(field)) is not int or payload[field] != reference[field]:
            raise ValueError(f"Incorrect {field}")
    actual = payload.get("data")
    if not isinstance(actual, list) or len(actual) != len(reference["data"]):
        raise ValueError("Incorrect result length")
    for row, expected_row in zip(actual, reference["data"]):
        if not isinstance(row, dict) or any(row.get(k) != v or k not in row or type(row[k]) is not type(v)
                                           for k, v in expected_row.items()):
            raise ValueError("Incorrect entry, order, ownership or metadata")


def manifest(catalogue, collection):
    value = dict(version=VERSION, own_entries=len(collection), foreign_entries=FOREIGN_ENTRIES,
                 catalogue=catalogue, entries=collection, scenarios=scenarios())
    digest = hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode()).hexdigest()
    return dict(version=VERSION, seed=0, catalogue_issues=len(catalogue),
                own_entries=len(collection), foreign_entries=FOREIGN_ENTRIES,
                scenarios=len(scenarios()), sha256=digest)


def literal(value):
    if value is None:
        return "NULL"
    if isinstance(value, int):
        return str(value)
    return "CONVERT(0x" + str(value).encode("utf-8").hex() + " USING utf8mb4)"


def insert_batches(table, columns, rows, batch_size=500):
    batch = []
    for row in rows:
        batch.append("(" + ",".join(map(literal, row)) + ")")
        if len(batch) == batch_size:
            yield f"INSERT INTO {table} ({','.join(columns)}) VALUES " + ",".join(batch) + ";"
            batch = []
    if batch:
        yield f"INSERT INTO {table} ({','.join(columns)}) VALUES " + ",".join(batch) + ";"


def fixture_sql(catalogue, collection):
    yield "START TRANSACTION;"
    yield from insert_batches("series", ("id", "name", "slug", "active", "total_issues"),
                              [(i, name, slug, 1, 3500) for i, (name, slug) in SERIES.items()]
                              + [(3, "Hidden fixture", "hidden-fixture", 0, 1)])
    yield from insert_batches("issues", ("id", "series_id", "issue_number", "title", "published_at"),
                              [(i["issue_id"], i["series_id"], i["issue_number"], i["title"], "2025-01-01")
                               for i in catalogue] + [(7001, 3, 1, "Hidden issue", "2025-01-01")])
    yield from insert_batches("persons", ("id", "name"),
                              list(enumerate(AUTHORS, 1)) + [(5, "Cover Artist Only")])
    people = {name: i for i, name in enumerate(AUTHORS, 1)}
    yield from insert_batches("issue_persons", ("issue_id", "person_id", "role"),
                              ((i["issue_id"], people[name], "author") for i in catalogue for name in i["authors"]))
    yield from insert_batches("issue_persons", ("issue_id", "person_id", "role"),
                              ((i["issue_id"], 5, "cover_artist") for i in catalogue))
    columns = ("id", "user_id", "issue_id", "copy_number", "edition_label", "condition_grade", "status", "notes", "created_at")
    yield from insert_batches("collection_entries", columns,
                              ((r["id"], 1, r["issue_id"], r["copy_number"], r["edition_label"],
                                r["condition_grade"], r["status"], r["notes"], r["created_at"].replace("T", " "))
                               for r in collection))
    yield "COMMIT;"


def foreign_sql():
    yield "START TRANSACTION;"
    yield from insert_batches("users", ("id", "email", "display_name", "email_verified"),
                              ((i + 2, f"foreign-{i}@example.test", f"Foreign {i}", 1) for i in range(100)))
    yield from insert_batches("collection_entries", ("user_id", "issue_id", "copy_number", "condition_grade", "status"),
                              ((user + 2, (number + user * 61) % 7000 + 1, 1,
                                None if number % 3 == 2 else f"Z{number % 5}",
                                ("owned", "duplicate", "wanted")[number % 3])
                               for user in range(100) for number in range(1000)))
    yield "COMMIT;"
