"""Timing, concurrency, fail-closed reports and disposable-stack lifecycle tests."""

import copy
import io
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import collection_performance as perf

REFERENCE = dict(page=1, per_page=50, total=0, data=[])


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        self.do_GET()

    def do_GET(self):
        with self.server.counter_lock:
            self.server.active += 1
            self.server.maximum = max(self.server.maximum, self.server.active)
            if self.server.active == self.server.expected_concurrency:
                self.server.all_started.set()
        try:
            body = b"invalid" if self.path == "/bad-json" else json.dumps(REFERENCE).encode()
            self.send_response(429 if self.path == "/limited" else 200)
            self.send_header("Content-Type", "text/plain" if self.path == "/bad-type" else "application/json")
            self.send_header("Content-Length", str(len(body)))
            if self.path == "/cookie":
                self.send_header("Set-Cookie", "access_token=test; HttpOnly")
            self.end_headers()
            self.wfile.flush()
            self.server.headers_sent.set()
            if self.server.gated:
                self.server.release.wait(timeout=10)
            if self.server.expected_concurrency > 1:
                if not self.server.all_started.wait(timeout=10):
                    raise RuntimeError("Expected parallel requests did not arrive")
            self.wfile.write(body)
            self.wfile.flush()
        finally:
            with self.server.counter_lock:
                self.server.active -= 1


class ClientTests(unittest.TestCase):
    def setUp(self):
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.server.counter_lock = threading.Lock()
        self.server.headers_sent = threading.Event()
        self.server.all_started = threading.Event()
        self.server.release = threading.Event()
        self.server.active = self.server.maximum = 0
        self.server.expected_concurrency = 1
        self.server.gated = False
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.release.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)

    def test_client_reads_body_and_collects_login_cookie(self):
        client = perf.Client(self.server.server_port)
        try:
            result, duration = client.request("/cookie", "POST", {"test": True})
            self.assertEqual(result, REFERENCE)
            self.assertGreater(duration, 0)
            self.assertEqual(client.cookie, "access_token=test")
        finally:
            client.close()

    def test_headers_alone_cannot_complete_a_measurement(self):
        self.server.gated = True
        client = perf.Client(self.server.server_port)
        try:
            with ThreadPoolExecutor(max_workers=1) as executor:
                pending = executor.submit(client.request, "/")
                self.assertTrue(self.server.headers_sent.wait(timeout=5))
                self.assertFalse(pending.done())
                self.server.release.set()
                self.assertEqual(pending.result(timeout=5)[0], REFERENCE)
        finally:
            client.close()

    def test_http_errors_invalid_json_and_wrong_content_type_fail(self):
        for path in ("/limited", "/bad-json", "/bad-type"):
            with self.subTest(path=path):
                client = perf.Client(self.server.server_port)
                try:
                    with self.assertRaises(ValueError):
                        client.request(path)
                finally:
                    client.close()

    def test_serial_and_five_worker_runs_have_exact_request_counts(self):
        for concurrency in (1, 5):
            with self.subTest(concurrency=concurrency):
                self.server.maximum = 0
                self.server.expected_concurrency = concurrency
                self.server.all_started.clear()
                result = perf.measure(self.server.server_port, "", {}, REFERENCE, concurrency, samples=10, warmup=0)
                self.assertEqual(result["samples"], 10)
                self.assertEqual(self.server.maximum, concurrency)
                self.assertEqual(len(result["durations_ms"]), 10)

    def test_warmup_is_validated_and_not_counted(self):
        result = perf.measure(self.server.server_port, "", {}, REFERENCE, 1, samples=10, warmup=2)
        self.assertEqual(result["samples"], 10)
        with self.assertRaises(ValueError):
            perf.measure(self.server.server_port, "", {}, {**REFERENCE, "total": 1}, 1, samples=10, warmup=2)
        for concurrency, samples, warmup in [(2, 10, 0), (5, 11, 0), (5, 5, 2)]:
            with self.assertRaises(ValueError):
                perf.measure(1, "", {}, REFERENCE, concurrency, samples, warmup)


def valid_measurement(**changes):
    return dict(samples=200, errors=0, durations_ms=[1.0] * 200, p95_ms=1.0,
                p50_ms=1.0, p99_ms=1.0, concurrency=1, client_cpu_ratio=.1, **changes)


class EvaluationTests(unittest.TestCase):
    def test_nearest_rank_and_invalid_samples(self):
        self.assertEqual(perf.percentile(list(range(1, 201)), .95), 190)
        for values, proportion in [([], .95), ([1], 0), ([1], 2), ([float("nan")], .95),
                                    ([float("inf")], .95), ([-1], .95)]:
            with self.assertRaises(ValueError):
                perf.percentile(values, proportion)

    def test_strict_threshold_full_samples_and_generator_capacity(self):
        perf.check_measurement(valid_measurement())
        for changes in [dict(samples=199), dict(errors=1), dict(durations_ms=[1] * 199),
                        dict(p95_ms=2), dict(durations_ms=[500] * 200, p95_ms=500),
                        dict(concurrency=5, client_cpu_ratio=.95)]:
            result = valid_measurement()
            result.update(changes)
            with self.subTest(changes=changes.keys()), self.assertRaises(ValueError):
                perf.check_measurement(result)

    def test_streamed_plans_and_executed_collection_visits(self):
        plan = {"query_block": {"nested_loop": [{"table": {"table_name": "ce", "r_rows": 2, "r_loops": 5000,
                                                          "used_key_parts": ["user_id", "issue_id"]}}]}}
        self.assertEqual(perf.collection_reads(plan), 10000)
        self.assertEqual(perf.collection_reads(None), 0)
        self.assertEqual(perf.decode_json_stream(' {"a": 1}\n {"b": 2}\n'), [{"a": 1}, {"b": 2}])
        for table in [dict(table_name="ce"), dict(table_name="ce", r_rows=-1, r_loops=2),
                      dict(table_name="ce", r_rows=1e308, r_loops=1e308)]:
            with self.assertRaises(ValueError):
                perf.collection_reads(table)
        plans = [dict(scenario=name, kind=kind, analyze=plan)
                 for name, _ in perf.fixture.scenarios() for kind in ("count", "results")]
        perf.check_plans(plans)
        broken = copy.deepcopy(plans)
        broken[0]["analyze"]["query_block"]["nested_loop"][0]["table"]["r_rows"] = 22
        with self.assertRaises(ValueError):
            perf.check_plans(broken)
        scoped = copy.deepcopy(plans)
        scoped[0]["analyze"]["author_source"] = {"table_name": "ce_authors", "r_rows": 110000, "r_loops": 1,
                                                 "used_key_parts": ["user_id"]}
        with self.assertRaises(ValueError):
            perf.check_plans(scoped)
        with self.assertRaises(ValueError):
            perf.check_plans([])
        self.assertEqual(perf.collection_reads({"table_name": "ce", "r_loops": 0}), 0)
        broken = copy.deepcopy(plans)
        broken[0]["analyze"] = {}
        with self.assertRaises(ValueError):
            perf.check_plans(broken)
        unsafe = copy.deepcopy(plans)
        unsafe[0]["analyze"] = {"query_block": {"table": {"table_name": "ce", "r_rows": 1, "r_loops": 1,
                                                          "used_key_parts": ["status"]}}}
        with self.assertRaises(ValueError):
            perf.check_plans(unsafe)
        page = copy.deepcopy(plans)
        page[0]["analyze"] = {"query_block": {"table": {"table_name": "ce", "r_rows": 1, "r_loops": 50,
                                                        "access_type": "eq_ref", "key": "PRIMARY"}}}
        perf.check_plans(page)
        skipped = copy.deepcopy(plans)
        skipped[0]["analyze"]["unused"] = [{"table_name": "i"}, {"table_name": "ce_authors", "r_loops": 0}]
        perf.check_plans(skipped)

    def test_missing_duplicate_scenarios_and_missing_plans_fail(self):
        matrix = [dict(phase=phase, scenario=name, **valid_measurement())
                  for phase in ("own-only", "with-foreign") for name, _ in perf.fixture.scenarios()]
        # Both concurrency modes are required; a large number of fast requests is insufficient.
        with self.assertRaises(ValueError):
            perf.report_is_complete(dict(measurements=matrix, plans={}))
        full = []
        for row in matrix:
            full += [row, {**row, "concurrency": 5}]
        with patch.object(perf, "check_plans"):
            report = dict(measurements=full, plans={"own-only": [], "with-foreign": []})
            perf.report_is_complete(report)
            report["plans"] = {}
            with self.assertRaises(ValueError):
                perf.report_is_complete(report)

    def test_partial_failure_report_is_written(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            report = dict(run_id="test", status="failed", measurements=[], failure="Missing samples")
            perf.write_report(output, report)
            self.assertEqual(json.loads((output / "results.json").read_text())["status"], "failed")
            self.assertIn("Missing samples", (output / "summary.md").read_text())

    def test_command_output_propagates_nonzero_exit_codes(self):
        self.assertEqual(perf.command_output([sys.executable, "-c", "print('value')"]), "value")
        with self.assertRaises(subprocess.CalledProcessError):
            perf.command_output([sys.executable, "-c", "raise SystemExit(7)"])

    def test_cli_resolves_output_and_forwards_the_prebuilt_image_policy(self):
        with patch.object(sys, "argv", ["collection_performance.py", "--output", "performance-results/test", "--skip-build"]), \
                patch.object(perf, "run", return_value=1) as run:
            self.assertEqual(perf.main(), 1)
            run.assert_called_once_with(Path("performance-results/test").resolve(), skip_build=True,
                                        browser_check=False, profile="full")

    def test_help_entrypoint_never_requires_a_running_docker_engine(self):
        with patch.object(sys, "argv", ["collection_performance.py", "--help"]), \
                patch.object(sys, "stdout", new_callable=io.StringIO) as stdout:
            with self.assertRaises(SystemExit) as exit_code:
                runpy.run_path(str(Path(perf.__file__)), run_name="__main__")
            self.assertEqual(exit_code.exception.code, 0)
            self.assertIn("--browser-check", stdout.getvalue())


def profile_report(profile):
    config = perf.benchmark_profile(profile)
    plan = {"query_block": {"table": {"table_name": "ce", "r_rows": 10000, "r_loops": 1,
                                       "used_key_parts": ["user_id", "issue_id"]}}}
    return dict(profile=profile,
                measurements=[dict(phase=phase, scenario=name,
                                   **{**valid_measurement(), "concurrency": mode})
                              for phase in config["phases"] for name, _ in config["scenarios"]
                              for mode in config["concurrency"]],
                plans={phase: [dict(scenario=name, kind=kind, analyze=plan)
                               for name, _ in config["scenarios"] for kind in ("count", "results")]
                       for phase in config["phases"]})


class ProfileTests(unittest.TestCase):
    def test_full_report_requires_complete_structured_plans_in_every_phase(self):
        valid = profile_report("full")
        for phase in ("own-only", "with-foreign"):
            for replacement in ([], None, {}, [None] * 126, valid["plans"][phase][:-1],
                                [*valid["plans"][phase][1:], valid["plans"][phase][1]]):
                report = copy.deepcopy(valid)
                report["plans"][phase] = replacement
                with self.subTest(phase=phase, replacement=type(replacement).__name__), self.assertRaises(ValueError):
                    perf.report_is_complete(report)
            for broken in ({}, {"scenario": [], "kind": "count"},
                           {"scenario": "all", "kind": []}, {"analyze": None},
                           {"analyze": {"query_block": None}}, {"analyze": {"query_block": []}},
                           {"analyze": {"query_block": {}}},
                           {"analyze": {"query_block": {"table": {"table_name": "ce", "r_loops": 1}}}}):
                report = copy.deepcopy(valid)
                report["plans"][phase][0].update(broken)
                if not broken:
                    report["plans"][phase][0] = {}
                with self.subTest(phase=phase, broken=broken), self.assertRaises(ValueError):
                    perf.report_is_complete(report)

    def test_only_foreign_plans_enforce_user_indexes_and_scan_limits(self):
        report = profile_report("full")
        table = {"table_name": "ce", "access_type": "ALL", "r_loops": 2, "r_rows": 10000}
        report["plans"]["own-only"][0]["analyze"] = {"query_block": {"table": table}}
        perf.report_is_complete(report)
        report["plans"]["with-foreign"][0]["analyze"] = {"query_block": {"table": table}}
        with self.assertRaises(ValueError):
            perf.report_is_complete(report)
        table["used_key_parts"] = ["user_id"]
        with self.assertRaises(ValueError):
            perf.report_is_complete(report)

    def test_invalid_execution_metrics_are_rejected_even_without_foreign_limits(self):
        for field in ("r_rows", "r_loops"):
            for value in (None, True, "1", -1, float("nan"), float("inf")):
                report = profile_report("full")
                plan = report["plans"]["own-only"][0]
                # Each synthetic report shares one immutable plan until this test changes it.
                plan["analyze"] = copy.deepcopy(plan["analyze"])
                plan["analyze"]["query_block"]["table"][field] = value
                with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                    perf.report_is_complete(report)

    def test_smoke_has_twelve_original_cases_and_keeps_the_failed_author_case(self):
        full = dict(perf.benchmark_profile("full")["scenarios"])
        config = perf.benchmark_profile("smoke")
        self.assertEqual(len(config["scenarios"]), 12)
        self.assertEqual(len(dict(config["scenarios"])), 12)
        self.assertEqual(config["phases"], ("with-foreign",))
        self.assertEqual(config["concurrency"], (5,))
        self.assertIn("sort-author-asc", dict(config["scenarios"]))
        self.assertIn("author-last-page", dict(config["scenarios"]))
        self.assertIn("missing-sort-author-asc", dict(config["scenarios"]))
        for name, params in config["scenarios"]:
            self.assertEqual(params, full[name])
        self.assertEqual((perf.SAMPLES, perf.WARMUP, perf.LIMIT_MS), (200, 20, 500))

    def test_unknown_profiles_and_missing_smoke_cases_fail(self):
        with self.assertRaises(ValueError):
            perf.benchmark_profile("quick-but-incomplete")
        with patch.object(perf.fixture, "scenarios", return_value=[]):
            with self.assertRaises(ValueError):
                perf.benchmark_profile("smoke")

    def test_smoke_is_never_accepted_as_a_complete_full_run(self):
        full = profile_report("full")
        smoke = profile_report("smoke")
        self.assertEqual(len(full["measurements"]), 252)
        self.assertEqual(len(smoke["measurements"]), 12)
        perf.report_is_complete(full)
        perf.report_is_complete(smoke)
        with self.assertRaises(ValueError):
            perf.report_is_complete({**smoke, "profile": "full"})
        for extra in ([], [smoke["measurements"][0]],
                      [{**smoke["measurements"][0], "concurrency": 1}],
                      [{**smoke["measurements"][0], "phase": "own-only"}]):
            broken = copy.deepcopy(smoke)
            if extra:
                broken["measurements"] += extra
            else:
                broken["measurements"].pop()
            with self.assertRaises(ValueError):
                perf.report_is_complete(broken)
        broken = copy.deepcopy(smoke)
        broken["plans"]["with-foreign"].pop()
        with self.assertRaises(ValueError):
            perf.report_is_complete(broken)

    def test_cli_forwards_smoke_and_rejects_unknown_profiles(self):
        with patch.object(sys, "argv", ["collection_performance.py", "--profile", "smoke"]), \
                patch.object(perf, "run", return_value=0) as run:
            self.assertEqual(perf.main(), 0)
            run.assert_called_once_with(perf.ROOT / "performance-results" / "collection",
                                        skip_build=False, browser_check=False, profile="smoke")
        with patch.object(sys, "argv", ["collection_performance.py", "--profile", "invalid"]), \
                patch.object(perf, "run") as run, patch.object(sys, "stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as error:
                perf.main()
            self.assertEqual(error.exception.code, 2)
            run.assert_not_called()

    def test_smoke_seeds_foreign_data_and_measures_only_five_workers_without_browsers(self):
        self.run_smoke()

    def test_smoke_keeps_the_strict_threshold_and_retains_the_failed_measurement(self):
        self.run_smoke(failing=True)

    def run_smoke(self, failing=False):
        reference = profile_report("smoke")
        def measurement(*args):
            result = {**valid_measurement(), "concurrency": args[4]}
            if failing and args[2] == {"sort": "author", "sort_dir": "asc"}:
                result.update(durations_ms=[500.0] * 200, p95_ms=500.0)
            return result
        with tempfile.TemporaryDirectory() as directory, patch.object(perf, "Stack") as create, \
                patch.object(perf, "command_output", side_effect=["sha", ""]), \
                patch.object(perf, "measure", side_effect=measurement) as measure:
            stack = create.return_value
            stack.plans.return_value = reference["plans"]["with-foreign"]
            stack.metadata.return_value = {}
            output = Path(directory)
            self.assertEqual(perf.run(output, profile="smoke"), int(failing))
            report = json.loads((output / "results.json").read_text())
            self.assertEqual(report["profile"], "smoke")
            self.assertEqual(report["contract"]["concurrency"], [5])
            self.assertEqual(report["contract"]["phases"], ["with-foreign"])
            self.assertTrue(all(call.args[4] == 5 for call in measure.call_args_list))
            stack.validate_counts.assert_called_once_with(100000)
            stack.login.assert_called_once()
            stack.close.assert_called_once()
            stack.browser_check.assert_not_called()
            self.assertIn("does not establish full acceptance", (output / "summary.md").read_text())
            if failing:
                self.assertEqual(report["status"], "failed")
                self.assertEqual(report["measurements"][-1]["scenario"], "sort-author-asc")
                self.assertIn("strictly below 500", report["failure"])
            else:
                self.assertEqual(report["status"], "passed")
                self.assertEqual(len(report["measurements"]), 12)
                self.assertEqual(sum(r["samples"] for r in report["measurements"]), 2400)


class StackTests(unittest.TestCase):
    def test_browser_check_is_sequential_and_checks_all_browsers_without_retries(self):
        stack = perf.Stack()
        try:
            with patch.object(stack, "compose", side_effect=["", "", "127.0.0.1:1234"]), \
                    patch.object(perf.subprocess, "run") as run:
                result = stack.browser_check("private-password")
                self.assertEqual(result["browsers"], ["chromium", "firefox", "webkit"])
                self.assertEqual(run.call_args.kwargs["env"]["PLAYWRIGHT_COLLECTION_TOTAL"], "10000")
                self.assertIn("--retries=0", run.call_args.args[0])
            with patch.object(stack, "compose", side_effect=["", "", "0.0.0.0:1234"]):
                with self.assertRaises(ValueError):
                    stack.browser_check("private-password")
        finally:
            stack.directory.cleanup()
    def test_start_checks_the_bound_address_and_empty_database(self):
        for address, count, valid in [("127.0.0.1:1234", "0", True),
                                      ("0.0.0.0:1234", "0", False), ("127.0.0.1:1234", "1", False)]:
            with self.subTest(address=address, count=count):
                stack = perf.Stack()
                try:
                    with patch.object(stack, "compose", side_effect=["", "", address]) as compose, \
                            patch.object(stack, "sql", return_value=count):
                        if valid:
                            stack.start()
                            self.assertEqual(stack.port, 1234)
                            self.assertIn("build", compose.call_args_list[0].args)
                        else:
                            with self.assertRaises(ValueError):
                                stack.start()
                finally:
                    stack.directory.cleanup()

    def test_revision_matched_prebuilt_image_skips_build_and_dirty_checkout_fails(self):
        for dirty, valid in [("", True), (" M source", False)]:
            stack = perf.Stack()
            try:
                with patch.object(perf, "command_output", side_effect=["sha", "sha", dirty]), \
                        patch.object(stack, "compose", side_effect=["", "127.0.0.1:1234"]) as compose, \
                        patch.object(stack, "sql", return_value="0"):
                    if valid:
                        stack.start(skip_build=True)
                        self.assertNotIn("build", compose.call_args_list[0].args)
                    else:
                        with self.assertRaises(ValueError):
                            stack.start(skip_build=True)
            finally:
                stack.directory.cleanup()

    def test_login_requires_a_cookie_and_closes_the_client(self):
        stack = perf.Stack()
        try:
            with patch.object(perf, "Client") as create:
                client = create.return_value
                client.cookie = "access_token=test"
                self.assertEqual(stack.login("password"), "access_token=test")
                client.request.assert_called_once()
                client.close.assert_called_once()
                client.cookie = ""
                with self.assertRaises(ValueError):
                    stack.login("password")
                self.assertEqual(client.close.call_count, 2)
        finally:
            stack.directory.cleanup()

    def test_seed_verifies_the_created_account_before_inserting_data(self):
        stack = perf.Stack()
        try:
            with patch.object(perf, "Client") as client, patch.object(stack, "sql") as sql, \
                    patch.object(stack, "validate_counts") as counts:
                stack.seed([], [], "password")
                self.assertIn("privacy_policy_version", client.return_value.request.call_args.args[2])
                self.assertIn("verification_token_expires_at=NULL", sql.call_args_list[0].args[0])
                counts.assert_called_once_with(0)
                client.return_value.close.assert_called_once()
        finally:
            stack.directory.cleanup()

    def test_query_capture_uses_the_executed_production_selects(self):
        query = "SELECT COUNT(*) FROM collection_entries ce WHERE ce.user_id=1"
        executed = json.dumps({"query_block": {"table": {"table_name": "ce", "r_rows": 1, "r_loops": 1}}})
        for queries, plans, valid in [([query, query], "\n".join([executed] * 4), True),
                                     ([], "", False), ([query + "; DROP TABLE t", query], "", False),
                                     (["UPDATE t SET x=1", query], "", False), ([query, query], "{}", False)]:
            with self.subTest(queries=queries, valid=valid):
                stack = perf.Stack()
                try:
                    with patch.object(perf.fixture, "scenarios", return_value=[("all", {})]), \
                            patch.object(perf, "Client") as client, \
                            patch.object(stack, "compose", side_effect=["", "", "127.0.0.1:1234"]) as compose, \
                            patch.object(stack, "sql", side_effect=["", "", json.dumps(queries), plans]):
                        client.return_value.request.return_value = (REFERENCE, 1)
                        if valid:
                            result = stack.plans("cookie", {"all": REFERENCE})
                            self.assertEqual([p["kind"] for p in result], ["count", "results"])
                            self.assertEqual(result[0]["sql"], query)
                            self.assertEqual(stack.port, 1234)
                            self.assertEqual(compose.call_args_list[0].args, ("restart", "backend"))
                            self.assertIn("--wait", compose.call_args_list[1].args)
                            client.assert_called_once_with(1234, "cookie")
                        else:
                            with self.assertRaises(ValueError):
                                stack.plans("cookie", {"all": REFERENCE})
                        client.return_value.close.assert_called_once()
                finally:
                    stack.directory.cleanup()

    def test_query_capture_stops_logging_when_backend_restart_fails(self):
        stack = perf.Stack()
        try:
            with patch.object(stack, "sql") as sql, patch.object(perf, "Client") as client, \
                    patch.object(stack, "compose", side_effect=subprocess.CalledProcessError(1, "restart")):
                with self.assertRaises(subprocess.CalledProcessError):
                    stack.plans("cookie", {})
                client.assert_not_called()
                self.assertIn("SET GLOBAL general_log=ON", sql.call_args_list[0].args[0])
                sql.assert_called_with("SET GLOBAL general_log=OFF;")
        finally:
            stack.directory.cleanup()

    def test_metadata_validates_resource_limits_and_records_actual_versions(self):
        for memory, valid in [(512 * 1024 * 1024, True), (1, False)]:
            stack = perf.Stack()
            try:
                values = [json.dumps(dict(image_id="db", cpu_limit=10**9, memory_limit_bytes=1024**3)),
                          json.dumps(dict(image_id="backend", cpu_limit=10**9, memory_limit_bytes=memory)),
                          '{"cpus":4}', '{"Server":{"Version":"test"}}']
                with patch.object(perf, "command_output", side_effect=values), \
                        patch.object(stack, "sql", return_value=f"MariaDB\t{256 * 1024**2}"), \
                        patch.object(stack, "compose", side_effect=["db-id", "backend-id", "model name : CPU fixture"]):
                    if valid:
                        result = stack.metadata()
                        self.assertEqual(result["cpu_model"], "CPU fixture")
                        self.assertEqual(result["docker_host"], {"cpus": 4})
                    else:
                        with self.assertRaises(ValueError):
                            stack.metadata()
            finally:
                stack.directory.cleanup()

    def test_stack_scopes_commands_and_cleanup_to_its_own_project(self):
        with patch.dict(perf.os.environ, {"PERF_DB_PASSWORD": "ambient-password"}):
            stack = perf.Stack()
        self.assertNotEqual(stack.environment["PERF_DB_PASSWORD"], "ambient-password")
        name = stack.directory.name
        try:
            self.assertTrue(stack.project.startswith("lilly-collection-perf-"))
            self.assertNotIn("--profile", stack.command)
            self.assertIn(str(perf.ROOT / "docker-compose.performance.yml"), stack.command)
            with patch.object(perf.subprocess, "run", return_value=MagicMock(stdout="ok")) as run:
                self.assertEqual(stack.sql("SELECT 1;"), "ok")
                self.assertEqual(run.call_args.kwargs["input"], "SELECT 1;")
                self.assertEqual(run.call_args.kwargs["env"], stack.environment)
                self.assertIn(stack.project, run.call_args.args[0])
                stack.close()
                self.assertIn("--volumes", run.call_args.args[0])
                self.assertEqual(run.call_args.args[0][len(stack.command):],
                                 ["--profile", "browser", "down", "--volumes", "--remove-orphans"])
        finally:
            stack.directory.cleanup()
        self.assertFalse(Path(name).exists())

    def test_counts_and_prebuilt_image_guards(self):
        stack = perf.Stack()
        try:
            with patch.object(stack, "sql", side_effect=["10000\t10000\t0\n", ""]) as sql:
                stack.validate_counts(0)
                self.assertIn("ANALYZE TABLE", sql.call_args.args[0])
            with patch.object(stack, "sql", return_value="110000\t10000\t0"):
                with self.assertRaises(ValueError):
                    stack.validate_counts(100000)
            with patch.object(perf, "command_output", side_effect=["old", "new"]):
                with self.assertRaises(ValueError):
                    stack.start(skip_build=True)
        finally:
            stack.directory.cleanup()

    def test_orchestration_failure_cleans_up_and_writes_safe_failure(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(perf, "Stack") as create, \
                patch.object(perf, "command_output", side_effect=["commit", "dirty"]):
            stack = create.return_value
            stack.start.side_effect = subprocess.TimeoutExpired("secret-command", 30)
            self.assertEqual(perf.run(Path(directory)), 1)
            stack.close.assert_called_once()
            report = json.loads((Path(directory) / "results.json").read_text())
            self.assertEqual(report["failure"], "TimeoutExpired")
            self.assertNotIn("secret-command", json.dumps(report))

    def test_complete_orchestration_checks_both_datasets_and_both_modes(self):
        plan = {"query_block": {"table": {"table_name": "ce", "r_rows": 10000, "r_loops": 1,
                                           "used_key_parts": ["user_id", "issue_id"]}}}
        plans = [dict(scenario="all", kind=kind, analyze=plan) for kind in ("count", "results")]
        for browser_check in (False, True):
            with self.subTest(browser_check=browser_check), tempfile.TemporaryDirectory() as directory, \
                    patch.object(perf, "Stack") as create, \
                    patch.object(perf, "command_output", side_effect=["sha", ""]), \
                    patch.object(perf.fixture, "scenarios", return_value=[("all", {})]), \
                    patch.object(perf.time, "monotonic", side_effect=[0, 2001, 2002, 2003, 2004]), \
                    patch.object(perf, "measure", side_effect=lambda *args: {**valid_measurement(), "concurrency": args[4]}):
                stack = create.return_value
                stack.plans.return_value = plans
                stack.metadata.return_value = {}
                stack.browser_check.return_value = dict(status="passed")
                self.assertEqual(perf.run(Path(directory), browser_check=browser_check), 0)
                if browser_check:
                    stack.browser_check.assert_called_once()
                else:
                    stack.browser_check.assert_not_called()
                stack.validate_counts.assert_called_once_with(100000)
                self.assertEqual(stack.login.call_count, 3)
                stack.close.assert_called_once()
                report = json.loads((Path(directory) / "results.json").read_text())
                self.assertEqual(report["status"], "passed")
                self.assertTrue(report["started_at_utc"].endswith("+00:00"))
                self.assertGreaterEqual(report["finished_at_utc"], report["started_at_utc"])
                self.assertEqual(len(report["measurements"]), 4)
                self.assertNotIn("access_token", json.dumps(report))

    def test_cleanup_error_cannot_produce_success(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(perf, "Stack") as create, \
                patch.object(perf, "command_output", side_effect=["sha", ""]):
            stack = create.return_value
            stack.start.side_effect = ValueError("Startup failed")
            stack.close.side_effect = RuntimeError("cleanup failed")
            self.assertEqual(perf.run(Path(directory)), 1)
            report = json.loads((Path(directory) / "results.json").read_text())
            self.assertIn("Cleanup failed", report["failure"])

    def test_failure_before_stack_creation_still_has_a_report(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(perf, "command_output", side_effect=ValueError("Revision unavailable")):
            self.assertEqual(perf.run(Path(directory)), 1)
            report = json.loads((Path(directory) / "results.json").read_text())
            self.assertEqual(report["failure"], "Revision unavailable")

    def test_subprocess_diagnostics_redact_generated_credentials(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(perf, "Stack") as create, \
                patch.object(perf, "command_output", side_effect=["sha", ""]):
            stack = create.return_value
            stack.secret_values = {"private-password"}
            stack.start.side_effect = subprocess.CalledProcessError(1, "compose", stderr="error: private-password")
            self.assertEqual(perf.run(Path(directory)), 1)
            report = json.loads((Path(directory) / "results.json").read_text())
            self.assertEqual(report["failure_details"], "error: [redacted]")


if __name__ == "__main__":
    unittest.main()
