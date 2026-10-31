#!/usr/bin/env python3
"""Read completed import jobs through the API and write a sanitized acceptance snapshot.

Only authentication creates a session. This tool never imports, activates, edits,
or deletes data. Use a dedicated local administrator and keep credentials out of arguments.
"""

import argparse
from datetime import date, datetime, timezone
import getpass
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import HTTPCookieProcessor, Request, build_opener

REFERENCES = {
    "maddrax": {
        1: ("Der Gott aus dem Eis", ["Jo Zybell"], "2000-02-08"),
        409: ("Falsche Götter", ["Jana Paradigi"], "2015-09-22"),
        555: ("Das Echo des Wandlers", ["Lucy Guth"], "2021-04-27"),
    },
    "john-sinclair": {
        1: ("Im Nachtclub der Vampire", ["Jason Dark"], "1978-01-17"),
        1000: ("Das Schwert des Salomo", ["Jason Dark"], "1997-09-01"),
        2303: ("Die Hure Babylon", ["Ian Rolf Hill"], "2022-08-30"),
    },
}
SOURCES = {
    "maddrax": ("maddraxikon", "de.maddraxikon.com"),
    "john-sinclair": ("gruselroman-wiki", "www.gruselroman-wiki.de"),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode()).hexdigest()


def paginate(get, path, key="data"):
    rows, expected, page = [], None, 1
    while True:
        batch = get(f"{path}?page={page}&per_page=100")
        require(batch["page"] == page, "Pagination returned the wrong page")
        total = batch["total"]
        require(isinstance(total, int) and total >= 0, "Invalid pagination total")
        require(expected is None or total == expected, "Data changed during pagination")
        expected = total
        entries = batch[key]
        require(isinstance(entries, list), "Invalid pagination entries")
        require(bool(entries) or len(rows) == total, "Pagination ended before the declared total")
        rows.extend(entries)
        require(len(rows) <= total, "Pagination exceeded the declared total")
        if len(rows) == total:
            return rows
        page += 1


def audit(job, summary, items, issues, today):
    slug = job["adapter_name"]
    require(slug in REFERENCES, "Unexpected adapter")
    require(job["status"] in ("completed", "completed_with_errors"), "Import is not complete")
    require(summary["job_id"] == job["id"], "Review belongs to another job")
    require(summary["blocking_count"] == 0, "Import has blocking findings")
    require(summary["series_active"], "Series has not been published")
    require(len(items) == job["total_issues"] == summary["outcomes"]["total"], "Discovery and review totals disagree")
    numbers = [item["issue_number"] for item in items]
    require(len(set(numbers)) == len(numbers) and all(n > 0 for n in numbers), "Duplicate or invalid source issue numbers")
    number_gaps = sorted(set(range(1, max(numbers) + 1)) - set(numbers)) if numbers else []
    require(not number_gaps, f"Unresolved source-number gaps prevent acceptance: {number_gaps}")
    counts = {key: sum(item["outcome"] == key for item in items) for key in
              ("created", "updated", "unchanged", "skipped", "failed", "not_processed")}
    require(sum(counts.values()) == len(items), "Unknown review outcome")
    require(counts["failed"] == counts["not_processed"] == 0, "Incomplete issue processing")
    for key, count in counts.items():
        require(summary["outcomes"][key] == count, "Review counts disagree")
        if key != "not_processed":
            require(job[f"{key}_issues"] == count, "Job counts disagree")
    for item in items:
        if item["outcome"] == "skipped":
            require(item["published_at"] is not None and date.fromisoformat(item["published_at"]) > today,
                    "Skipped issue is not demonstrably a future publication")
    imported = {item["issue_number"] for item in items if item["outcome"] != "skipped"}
    persisted = {issue["issue_number"]: issue for issue in issues}
    require(len(persisted) == len(issues), "Duplicate persisted issue numbers")
    require(imported == set(persisted), "Persisted issues do not match the complete source result")
    ids = [issue["id"] for issue in issues]
    require(len(set(ids)) == len(ids), "Duplicate persisted issue IDs")
    source_ids = [(issue["source_key"], issue["source_record_id"]) for issue in issues]
    require(len(set(source_ids)) == len(source_ids), "Duplicate persisted source identities")
    source_key, host = SOURCES[slug]
    for issue in issues:
        require(bool(issue["title"].strip()) and bool(issue["authors"])
                and all(author.strip() for author in issue["authors"]), "Missing mandatory issue metadata")
        require(issue["source_key"] == source_key and bool(issue["source_record_id"]), "Missing or wrong source identity")
        url = urlparse(issue["source_wiki_url"] or "")
        require(url.scheme == "https" and url.hostname == host, "Invalid source URL")
        if issue["published_at"] is not None:
            require(date.fromisoformat(issue["published_at"]) <= today, "Future issue was published")
    for number, (title, authors, published) in REFERENCES[slug].items():
        issue = persisted.get(number)
        require(issue is not None and (issue["title"], issue["authors"], issue["published_at"])
                == (title, authors, published), f"Pinned reference mismatch: {slug} {number}")
    require(all(check["status"] == "passed" for check in summary["reference_checks"]), "Server reference check failed")
    identities = sorted((row["issue_number"], row["id"], row["source_key"], row["source_record_id"]) for row in issues)
    # Compare source metadata independently of SQL relation ordering and sync timestamps.
    metadata_fields = ("issue_number", "title", "published_at", "part_number", "part_total", "cycle",
                       "source_key", "source_record_id", "source_wiki_url", "cover_url", "cover_local_path")
    relations = ("authors", "cover_artists", "keywords", "notes")
    metadata = sorted(({**{key: row.get(key) for key in metadata_fields},
                        **{key: sorted(row.get(key, [])) for key in relations}} for row in issues),
                      key=lambda row: row["issue_number"])
    return {
        "adapter": slug, "job_id": job["id"], "status": job["status"],
        "started_at": job.get("started_at"), "completed_at": job.get("completed_at"),
        "discovered": len(items), "persisted": len(issues), "outcomes": counts,
        "warnings": summary["warning_count"], "blocking": 0,
        "missing_publication_dates": sum(row["published_at"] is None for row in issues),
        "covers_available": sum(bool(row.get("cover_local_path")) for row in issues),
        "references_passed": sorted(REFERENCES[slug]),
        "identity_sha256": digest(identities),
        "metadata_sha256": digest(metadata),
        "number_gaps": number_gaps,
    }


def compare(previous, current):
    old = {run["adapter"]: run for run in previous["runs"]}
    new = {run["adapter"]: run for run in current["runs"]}
    require(set(old) == set(new), "Baseline adapters differ")
    for adapter, run in new.items():
        require(run["job_id"] != old[adapter]["job_id"], "Repeat acceptance needs a different import job")
        require(run["identity_sha256"] == old[adapter]["identity_sha256"], "Issue identities changed; inspect source changes before acceptance")
        require(run["outcomes"]["created"] == 0, "Repeat import created additional issues")
        require(run["metadata_sha256"] == old[adapter]["metadata_sha256"],
                "Issue metadata changed; inspect source changes and capture a new baseline before acceptance")
        require(run["outcomes"]["updated"] == 0, "Unchanged source metadata was reported as updated")
    require(current["collection_sha256"] == previous["collection_sha256"], "User collection changed")


class Client:
    def __init__(self, base_url):
        parsed = urlparse(base_url)
        require(parsed.scheme == "https" or (parsed.scheme == "http" and parsed.hostname in ("localhost", "127.0.0.1", "::1")),
                "Use HTTPS or a local loopback URL")
        require(not parsed.username and not parsed.password and parsed.path in ("", "/")
                and not parsed.query and not parsed.fragment, "Expected an origin without credentials or a path")
        self.base_url = base_url.rstrip("/")
        self.opener = build_opener(HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, path, data=None):
        request = Request(self.base_url + path, json.dumps(data).encode() if data is not None else None,
                          {"Content-Type": "application/json", "Origin": self.base_url})
        with self.opener.open(request, timeout=60) as response:
            return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://localhost:18080")
    parser.add_argument("--jobs", type=int, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    args = parser.parse_args()
    try:
        client = Client(args.base_url)
        email = os.environ.get("LILLY_ACCEPTANCE_EMAIL") or input("Administrator email: ")
        password = os.environ.get("LILLY_ACCEPTANCE_PASSWORD") or getpass.getpass("Administrator password: ")
        client.call("/api/v1/auth/login", {"email": email, "password": password})
        require(client.call("/api/v1/auth/me")["role"] == "admin", "Administrator role required")
        report = {"captured_at": datetime.now(timezone.utc).isoformat(), "runs": []}
        for job_id in args.jobs:
            root = f"/api/v1/admin/import/{job_id}"
            job = client.call(root)
            summary = client.call(root + "/review/summary")
            items = paginate(client.call, root + "/review/items", "items")
            issues = paginate(client.call, root + "/series-issues")
            run = audit(job, summary, items, issues, datetime.now(timezone.utc).date())
            public = paginate(client.call, f"/api/v1/series/{job['adapter_name']}/issues")
            require({row["id"] for row in public} == {row["id"] for row in issues}, "Public catalog differs from reviewed catalog")
            report["runs"].append(run)
        require(len({run["adapter"] for run in report["runs"]}) == len(report["runs"]), "Use one job per adapter")
        collection = paginate(client.call, "/api/v1/me/collection")
        selected_series = {run["adapter"] for run in report["runs"]}
        collection = [row for row in collection if row["series_slug"] in selected_series]
        require(len({row["id"] for row in collection}) == len(collection), "Duplicate collection entries")
        report["collection_scope"] = sorted(selected_series)
        # Compare user-owned fields, not joined source metadata that a sync may update.
        fields = ("id", "issue_id", "edition_label", "copy_number", "status", "condition_grade", "notes", "revision", "created_at", "updated_at")
        entries = sorted(({key: row.get(key) for key in fields} for row in collection), key=lambda row: row["id"])
        report["collection_sha256"] = digest(entries)
        report["collection_entries"] = len(entries)
        if args.baseline:
            compare(json.loads(args.baseline.read_text(encoding="utf-8")), report)
            report["repeat_verified"] = True
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"Verified {len(report['runs'])} import(s); sanitized report: {args.output}")
        return 0
    except (ValueError, KeyError, TypeError, HTTPError, URLError) as error:
        # Never print HTTP bodies, credentials, tokens, or user-owned data.
        message = f"HTTP {error.code}" if isinstance(error, HTTPError) else str(error)
        print(f"Acceptance failed: {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
