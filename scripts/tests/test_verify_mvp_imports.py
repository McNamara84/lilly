import copy
from datetime import date
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

spec = importlib.util.spec_from_file_location("verify_mvp_imports", Path(__file__).parents[1] / "verify-mvp-imports.py")
verifier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verifier)


def fixture():
    issues = [{"id": number, "issue_number": number, "title": title, "authors": authors,
               "published_at": published, "source_key": "maddraxikon", "source_record_id": f"Quelle:MX{number}",
               "source_wiki_url": f"https://de.maddraxikon.com/index.php?title=Quelle:MX{number}",
               "cover_local_path": f"covers/{number}.jpg"}
              for number, (title, authors, published) in verifier.REFERENCES["maddrax"].items()]
    for number in range(2, 556):
        if number not in verifier.REFERENCES["maddrax"]:
            issues.append({**issues[0], "id": number, "issue_number": number,
                           "source_record_id": f"Quelle:MX{number}",
                           "published_at": None if number == 2 else "2000-02-08"})
    items = [{"issue_number": issue["issue_number"], "outcome": "created", "published_at": issue["published_at"]} for issue in issues]
    counts = {"total": len(items), "created": len(items), "updated": 0, "unchanged": 0, "skipped": 0, "failed": 0, "not_processed": 0}
    job = {"adapter_name": "maddrax", "id": 1, "status": "completed", "total_issues": len(items),
           **{f"{key}_issues": count for key, count in counts.items() if key not in ("total", "not_processed")}}
    summary = {"job_id": 1, "blocking_count": 0, "warning_count": 0, "series_active": True,
               "outcomes": counts, "reference_checks": [{"status": "passed"}]}
    return job, summary, items, issues


class AuditTests(unittest.TestCase):
    def test_complete_import_with_optional_date_and_reference_checks(self):
        report = verifier.audit(*fixture(), date(2026, 10, 31))
        self.assertEqual(report["missing_publication_dates"], 1)
        self.assertEqual(report["references_passed"], [1, 409, 555])
        self.assertEqual(report["persisted"], 555)
        self.assertEqual(len(report["identity_sha256"]), 64)
        self.assertNotIn("title", report)

    def test_invalid_or_incomplete_imports_are_rejected(self):
        mutations = [
            lambda j, s, r, i: j.update(status="running"),
            lambda j, s, r, i: j.update(adapter_name="unknown"),
            lambda j, s, r, i: s.update(blocking_count=1),
            lambda j, s, r, i: s.update(series_active=False),
            lambda j, s, r, i: s.update(job_id=2),
            lambda j, s, r, i: j.update(total_issues=5),
            lambda j, s, r, i: r[1].update(issue_number=1),
            lambda j, s, r, i: r[1].update(outcome="failed"),
            lambda j, s, r, i: r[1].update(outcome="not_processed"),
            lambda j, s, r, i: r[1].update(outcome="unknown"),
            lambda j, s, r, i: j.update(created_issues=0),
            lambda j, s, r, i: s["outcomes"].update(created=0),
            lambda j, s, r, i: i[1].update(id=1),
            lambda j, s, r, i: i[1].update(issue_number=1),
            lambda j, s, r, i: i[1].update(issue_number=999),
            lambda j, s, r, i: i[1].update(title=" "),
            lambda j, s, r, i: i[1].update(authors=[]),
            lambda j, s, r, i: i[1].update(source_record_id=""),
            lambda j, s, r, i: i[1].update(source_key="other"),
            lambda j, s, r, i: i[1].update(source_record_id=i[0]["source_record_id"]),
            lambda j, s, r, i: i[1].update(source_wiki_url="http://evil.example"),
            lambda j, s, r, i: i[-1].update(published_at="invalid"),
            lambda j, s, r, i: i[-1].update(published_at="2099-01-01"),
            lambda j, s, r, i: i[0].update(title="Different reference"),
            lambda j, s, r, i: s["reference_checks"][0].update(status="failed"),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                values = fixture()
                mutate(*values)
                with self.assertRaises(ValueError):
                    verifier.audit(*values, date(2026, 10, 31))

    def test_only_demonstrably_future_issues_may_be_skipped(self):
        for published, valid in [(None, False), ("2020-01-01", False), ("2099-01-01", True)]:
            with self.subTest(published=published):
                job, summary, items, issues = fixture()
                items.append({"issue_number": 556, "outcome": "skipped", "published_at": published})
                job.update(total_issues=len(items), skipped_issues=1)
                summary["outcomes"].update(total=len(items), skipped=1)
                if valid:
                    self.assertEqual(verifier.audit(job, summary, items, issues, date(2026, 10, 31))["outcomes"]["skipped"], 1)
                else:
                    with self.assertRaises(ValueError):
                        verifier.audit(job, summary, items, issues, date(2026, 10, 31))

    def test_silently_omitted_overview_rows_cannot_pass_even_with_consistent_counts(self):
        job, summary, items, issues = fixture()
        items = [row for row in items if row["issue_number"] != 204]
        issues = [row for row in issues if row["issue_number"] != 204]
        job.update(total_issues=len(items), created_issues=len(items))
        summary["outcomes"].update(total=len(items), created=len(items))
        with self.assertRaisesRegex(ValueError, "source-number gaps.*204"):
            verifier.audit(job, summary, items, issues, date(2026, 10, 31))

    def test_repeat_preserves_ids_and_collection(self):
        first = {"runs": [verifier.audit(*fixture(), date(2026, 10, 31))], "collection_sha256": "unchanged"}
        second = copy.deepcopy(first)
        second["runs"][0].update(job_id=2)
        second["runs"][0]["outcomes"].update(created=0, unchanged=555)
        verifier.compare(first, second)
        for mutation in [
            lambda report: report.update(collection_sha256="changed"),
            lambda report: report["runs"][0].update(job_id=1),
            lambda report: report["runs"][0].update(identity_sha256="changed"),
            lambda report: report["runs"][0].update(metadata_sha256="changed"),
            lambda report: report["runs"][0]["outcomes"].update(created=1),
            lambda report: report["runs"][0]["outcomes"].update(updated=1),
            lambda report: report.update(runs=[]),
        ]:
            candidate = copy.deepcopy(second)
            mutation(candidate)
            with self.assertRaises(ValueError):
                verifier.compare(first, candidate)

    def test_metadata_digest_ignores_relation_order_but_detects_source_and_cover_changes(self):
        values = fixture()
        values[3][-1].update(keywords=["Phobos", "PHOBOS"], notes=["one", "two"])
        baseline = verifier.audit(*values, date(2026, 10, 31))["metadata_sha256"]
        values[3].reverse()
        values[3][0]["keywords"].reverse()
        values[3][0]["notes"].reverse()
        self.assertEqual(verifier.audit(*values, date(2026, 10, 31))["metadata_sha256"], baseline)
        for field, value in [("keywords", ["Phobos"]), ("notes", ["changed"]),
                             ("title", "Changed title"), ("cover_url", "https://example.test/new.jpg"),
                             ("cover_local_path", "new-cover.jpg")]:
            candidate = copy.deepcopy(values)
            candidate[3][0][field] = value
            self.assertNotEqual(verifier.audit(*candidate, date(2026, 10, 31))["metadata_sha256"], baseline)


class PaginationTests(unittest.TestCase):
    def test_all_pages_and_empty_results(self):
        requests = []
        def get(path):
            requests.append(path)
            return {"page": len(requests), "total": 2, "items": [len(requests)]}
        self.assertEqual(verifier.paginate(get, "/items", "items"), [1, 2])
        self.assertEqual(requests, ["/items?page=1&per_page=100", "/items?page=2&per_page=100"])
        self.assertEqual(verifier.paginate(lambda _: {"page": 1, "total": 0, "data": []}, "/empty"), [])

    def test_broken_pagination_fails_instead_of_hanging_or_accepting_partial_data(self):
        for batch in [
            {"page": 2, "total": 1, "data": [1]},
            {"page": 1, "total": -1, "data": []},
            {"page": 1, "total": 1, "data": []},
            {"page": 1, "total": 1, "data": [1, 2]},
            {"page": 1, "total": 1, "data": {}},
        ]:
            with self.subTest(batch=batch), self.assertRaises(ValueError):
                verifier.paginate(lambda _: batch, "/broken")
        batches = iter([{"page": 1, "total": 2, "data": [1]}, {"page": 2, "total": 3, "data": [2]}])
        with self.assertRaisesRegex(ValueError, "changed"):
            verifier.paginate(lambda _: next(batches), "/changing")

    def test_client_restricts_credentials_to_https_or_loopback_origins(self):
        for origin in ["http://localhost:18080", "http://127.0.0.1:80", "https://lilly.example"]:
            self.assertEqual(verifier.Client(origin).base_url, origin)
        for origin in ["http://lilly.example", "https://user:secret@lilly.example", "https://lilly.example/path", "file:///tmp", "https://lilly.example?token=secret"]:
            with self.subTest(origin=origin), self.assertRaises(ValueError):
                verifier.Client(origin)


class CommandTests(unittest.TestCase):
    def test_client_uses_cookie_session_json_and_same_origin(self):
        client = verifier.Client("http://localhost:18080/")
        with patch.object(client.opener, "open", return_value=io.BytesIO(b'{"ok":true}')) as request:
            self.assertEqual(client.call("/example", {"sample": 1}), {"ok": True})
        sent = request.call_args.args[0]
        self.assertEqual(sent.get_method(), "POST")
        self.assertEqual(sent.get_header("Origin"), "http://localhost:18080")
        self.assertEqual(json.loads(sent.data), {"sample": 1})

    def test_command_exports_sanitized_baseline_and_checks_repeat(self):
        job, summary, items, issues = fixture()
        scope = {"job": job, "summary": summary}
        def call(path, data=None):
            if path.endswith("/auth/login"):
                self.assertEqual(data, {"email": "test@example.test", "password": "private-password"})
                return {}
            self.assertIsNone(data)
            if path.endswith("/auth/me"):
                return {"role": "admin"}
            if path.endswith("/review/summary"):
                return scope["summary"]
            if "/review/items?" in path:
                return {"items": items, "page": 1, "total": len(items)}
            if "/me/collection?" in path:
                return {"data": [{"id": 1, "series_slug": "maddrax", "notes": "private-note"},
                                 {"id": 2, "series_slug": "another-series"}], "page": 1, "total": 2}
            if "?" in path:
                return {"data": issues, "page": 1, "total": len(issues)}
            return scope["job"]
        with tempfile.TemporaryDirectory(prefix="lilly-verifier-") as directory:
            baseline = Path(directory) / "first.json"
            repeat = Path(directory) / "second.json"
            for job_id, output, extra in [(1, baseline, []), (2, repeat, ["--baseline", str(baseline)])]:
                if job_id == 2:
                    scope["job"].update(id=2, created_issues=0, unchanged_issues=len(items))
                    scope["summary"]["job_id"] = 2
                    scope["summary"]["outcomes"].update(created=0, unchanged=len(items))
                    for item in items:
                        item["outcome"] = "unchanged"
                with patch.object(verifier, "Client") as factory, \
                     patch.dict(os.environ, {"LILLY_ACCEPTANCE_EMAIL": "test@example.test", "LILLY_ACCEPTANCE_PASSWORD": "private-password"}), \
                     patch("sys.argv", ["verify", "--jobs", str(job_id), "--output", str(output), *extra]), \
                     patch("sys.stdout", new_callable=io.StringIO):
                    factory.return_value.call.side_effect = call
                    self.assertEqual(verifier.main(), 0)
                document = output.read_text(encoding="utf-8")
                for secret in ["test@example.test", "private-password", "private-note"]:
                    self.assertNotIn(secret, document)
                self.assertEqual(json.loads(document)["collection_entries"], 1)
            self.assertTrue(json.loads(repeat.read_text(encoding="utf-8"))["repeat_verified"])

    def test_command_prompts_and_reports_http_errors_without_response_bodies(self):
        with patch.object(verifier, "Client") as factory, patch.dict(os.environ, {}, clear=True), \
             patch("builtins.input", return_value="test@example.test"), \
             patch.object(verifier.getpass, "getpass", return_value="private-password"), \
             patch("sys.argv", ["verify", "--jobs", "1", "--output", "unused.json"]), \
             patch("sys.stderr", new_callable=io.StringIO) as error:
            factory.return_value.call.side_effect = HTTPError("https://local", 401, "private response", {}, None)
            self.assertEqual(verifier.main(), 1)
            self.assertIn("HTTP 401", error.getvalue())
            self.assertNotIn("private", error.getvalue())

    def test_invalid_origin_is_reported_before_asking_for_credentials(self):
        with patch("sys.argv", ["verify", "--base-url", "http://external.example", "--jobs", "1", "--output", "unused.json"]), \
             patch("sys.stderr", new_callable=io.StringIO), patch("builtins.input") as prompt:
            self.assertEqual(verifier.main(), 1)
            prompt.assert_not_called()


if __name__ == "__main__":
    unittest.main()
