#!/usr/bin/env python3
"""Verify collection-search correctness and p95 latency in a disposable Docker stack."""

import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from http.client import HTTPConnection
from http.cookies import SimpleCookie
import json
import math
import os
from pathlib import Path
import platform
import secrets
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import urlencode
import uuid

import collection_fixture as fixture

ROOT = Path(__file__).resolve().parent.parent
SAMPLES = 200
WARMUP = 20
LIMIT_MS = 500
MODES = (1, 5)
SMOKE_SCENARIOS = (
    "all", "full-page", "status-owned", "condition-range", "q-0",
    "sort-title-asc", "sort-title-desc", "sort-author-asc", "sort-author-desc",
    "search-sort-author", "author-last-page", "missing-sort-author-asc",
)
SQL_CLIENT = ('MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb --default-character-set=utf8mb4 '
              '--batch --raw --skip-column-names "$MARIADB_DATABASE"')


def benchmark_profile(name):
    if name not in ("full", "smoke"):
        raise ValueError("Unknown benchmark profile")
    scenarios = fixture.scenarios()
    if name == "smoke":
        by_name = dict(scenarios)
        if not set(SMOKE_SCENARIOS).issubset(by_name):
            raise ValueError("Missing smoke scenario in the full fixture")
        scenarios = [(name, by_name[name]) for name in SMOKE_SCENARIOS]
    return dict(scenarios=scenarios,
                phases=("own-only", "with-foreign") if name == "full" else ("with-foreign",),
                concurrency=MODES if name == "full" else (5,))


def percentile(values, proportion):
    if not values or not 0 < proportion <= 1 or any(not math.isfinite(v) or v < 0 for v in values):
        raise ValueError("Invalid latency sample or percentile")
    return sorted(values)[math.ceil(proportion * len(values)) - 1]


class Client:
    def __init__(self, port, cookie="", timeout=10):
        self.connection = HTTPConnection("127.0.0.1", port, timeout=timeout)
        self.cookie = cookie

    def close(self):
        self.connection.close()

    def request(self, path, method="GET", body=None):
        headers = {"Cookie": self.cookie, "Accept": "application/json"}
        encoded = None if body is None else json.dumps(body).encode()
        if encoded is not None:
            headers["Content-Type"] = "application/json"
        start = time.perf_counter_ns()
        self.connection.request(method, path, body=encoded, headers=headers)
        response = self.connection.getresponse()
        data = response.read()
        elapsed_ms = (time.perf_counter_ns() - start) / 1_000_000
        if response.status not in (200, 201):
            raise ValueError(f"HTTP {response.status} for {path.split('?')[0]}")
        cookies = SimpleCookie()
        for key, value in response.getheaders():
            if key.lower() == "set-cookie":
                cookies.load(value)
        if "access_token" in cookies:
            self.cookie = f"access_token={cookies['access_token'].value}"
        if response.getheader("Content-Type", "").split(";")[0].strip() != "application/json":
            raise ValueError("API response is not JSON")
        return json.loads(data), elapsed_ms


def measure(port, cookie, params, reference, concurrency, samples=SAMPLES, warmup=WARMUP):
    if concurrency not in MODES or samples < concurrency or samples % concurrency or warmup < 0 or warmup % concurrency:
        raise ValueError("Sample counts must be divisible by the supported concurrency")
    path = "/api/v1/me/collection?" + urlencode(params)
    clocks = []
    barrier = threading.Barrier(concurrency, timeout=30,
                                action=lambda: clocks.extend((time.perf_counter(), time.process_time())))

    def worker():
        client = Client(port, cookie)
        try:
            for _ in range(warmup // concurrency):
                payload, _ = client.request(path)
                fixture.validate_response(payload, reference)
            barrier.wait()
            durations = []
            for _ in range(samples // concurrency):
                payload, duration = client.request(path)
                fixture.validate_response(payload, reference)
                durations.append(duration)
            return durations
        except BaseException:
            barrier.abort()
            raise
        finally:
            client.close()

    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [executor.submit(worker) for _ in range(concurrency)]
        durations = [duration for future in futures for duration in future.result()]
    wall = time.perf_counter() - clocks[0]
    cpu_ratio = (time.process_time() - clocks[1]) / max(wall, 0.000001)
    return dict(concurrency=concurrency, samples=len(durations), errors=0,
                p50_ms=percentile(durations, .5), p95_ms=percentile(durations, .95),
                p99_ms=percentile(durations, .99), max_ms=max(durations),
                client_cpu_ratio=cpu_ratio, elapsed_seconds=wall, requests_per_second=samples / wall,
                durations_ms=durations)


def check_measurement(result):
    if result.get("samples") != SAMPLES or result.get("errors") != 0:
        raise ValueError("Incomplete or failed measurement")
    durations = result.get("durations_ms", [])
    if len(durations) != SAMPLES or result.get("p95_ms") != percentile(durations, .95):
        raise ValueError("Missing or inconsistent raw samples")
    if result["p95_ms"] >= LIMIT_MS:
        raise ValueError("p95 must be strictly below 500 ms")
    # Python saturating a core can throttle a five-worker generator through the GIL.
    if result["concurrency"] == 5 and result["client_cpu_ratio"] >= .9:
        raise ValueError("Load generator is CPU saturated; measurement is invalid")


def decode_json_stream(value):
    decoder = json.JSONDecoder()
    result = []
    rest = value.strip()
    while rest:
        parsed, end = decoder.raw_decode(rest)
        result.append(parsed)
        rest = rest[end:].lstrip()
    return result


def collection_reads(plan, table_name="ce"):
    """Count executed collection row visits, including nested-loop repetitions."""
    if isinstance(plan, dict):
        own = 0
        if plan.get("table_name") == table_name:
            loops = plan.get("r_loops")
            if type(loops) not in (int, float) or not math.isfinite(loops) or loops < 0:
                raise ValueError("Executed collection plan lacks valid loop metrics")
            if loops == 0:
                return 0
            rows = plan.get("r_rows")
            if type(rows) not in (int, float) or not math.isfinite(rows) or rows < 0:
                raise ValueError("Executed collection plan lacks valid row metrics")
            own = float(rows) * float(loops)
            if not math.isfinite(own) or own < 0:
                raise ValueError("Invalid executed row metrics")
        return own + sum(collection_reads(value, table_name) for value in plan.values()
                         if isinstance(value, (dict, list)))
    if isinstance(plan, list):
        return sum(collection_reads(value, table_name) for value in plan)
    return 0


def plan_tables(value):
    if isinstance(value, dict):
        if "table_name" in value:
            yield value
        for child in value.values():
            yield from plan_tables(child)
    elif isinstance(value, list):
        for child in value:
            yield from plan_tables(child)


def check_plans(plans, scenarios=None, enforce_user_scope=True):
    scenarios = fixture.scenarios() if scenarios is None else scenarios
    required = {(name, kind) for name, _ in scenarios for kind in ("count", "results")}
    if (not isinstance(plans, list) or len(plans) != len(required)
            or any(not isinstance(p, dict) or not isinstance(p.get("scenario"), str)
                   or not isinstance(p.get("kind"), str) for p in plans)
            or {(p.get("scenario"), p.get("kind")) for p in plans} != required):
        raise ValueError("Incomplete SQL plan matrix")
    for plan in plans:
        if (not isinstance(plan.get("analyze"), dict)
                or not isinstance(plan["analyze"].get("query_block"), dict)
                or not plan["analyze"]["query_block"]):
            raise ValueError("Missing executed query plan")
        reads = {alias: collection_reads(plan["analyze"], alias) for alias in ("ce", "ce_authors")}
        if not enforce_user_scope:
            continue
        for table in plan_tables(plan["analyze"]):
            if table["table_name"] not in ("ce", "ce_authors") or table.get("r_loops") == 0:
                continue
            user_key = "user_id" in table.get("used_key_parts", [])
            page_lookup = (table["table_name"] == "ce" and table.get("access_type") == "eq_ref"
                           and table.get("key") == "PRIMARY")
            if not user_key and not page_lookup:
                raise ValueError(f"Collection access does not use user scope: {plan['scenario']} / {plan['kind']}")
        # Rounding in MariaDB's per-loop averages warrants a small, documented margin.
        for alias in ("ce", "ce_authors"):
            if reads[alias] > fixture.OWN_ENTRIES * 1.05 + 100:
                raise ValueError(f"Query scans unrelated collections: {plan['scenario']} / {plan['kind']} / {alias}")


class Stack:
    def __init__(self):
        self.project = "lilly-collection-perf-" + uuid.uuid4().hex[:12]
        self.directory = tempfile.TemporaryDirectory(prefix="lilly-collection-perf-")
        self.env_file = Path(self.directory.name) / ".env"
        values = {"PERF_DB_ROOT_PASSWORD": secrets.token_hex(24),
                  "PERF_DB_PASSWORD": secrets.token_hex(24), "PERF_JWT_SECRET": secrets.token_hex(48)}
        self.secret_values = set(values.values())
        self.environment = {**os.environ, **values}
        self.env_file.write_text("".join(f"{k}={v}\n" for k, v in values.items()), encoding="utf-8")
        self.env_file.chmod(0o600)
        self.command = ["docker", "compose", "--project-name", self.project,
                        "--env-file", str(self.env_file), "--file", str(ROOT / "docker-compose.performance.yml")]
        self.port = None

    def compose(self, *args, input_text=None, timeout=180):
        return subprocess.run([*self.command, *args], cwd=ROOT, input=input_text,
                              env=self.environment,
                              text=True, encoding="utf-8", errors="strict",
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=timeout, check=True).stdout

    def sql(self, statements):
        value = statements if isinstance(statements, str) else "\n".join(statements)
        return self.compose("exec", "-T", "db", "sh", "-c", SQL_CLIENT,
                            input_text=value, timeout=180)

    def start(self, skip_build=False):
        print("Building release backend and starting isolated reference stack...", flush=True)
        if skip_build:
            revision = command_output(["docker", "image", "inspect", "--format",
                                       '{{index .Config.Labels "org.opencontainers.image.revision"}}',
                                       "lilly-backend:collection-performance"])
            if revision != command_output(["git", "rev-parse", "HEAD"]) or command_output(
                    ["git", "status", "--porcelain"]):
                raise ValueError("Prebuilt image requires a clean checkout and matching commit label")
        else:
            self.compose("build", "backend", timeout=1800)
        self.compose("up", "--detach", "--no-build", "--wait", "--wait-timeout", "180", timeout=240)
        self.refresh_backend_port()
        if self.sql("SELECT COUNT(*) FROM users;").strip() != "0":
            raise ValueError("Refusing to seed a nonempty database")

    def refresh_backend_port(self):
        address = self.compose("port", "backend", "8080").strip()
        if not address.startswith("127.0.0.1:"):
            raise ValueError("Backend must be bound to loopback")
        self.port = int(address.rsplit(":", 1)[1])

    def login(self, password):
        client = Client(self.port)
        try:
            client.request("/api/v1/auth/login", "POST",
                           {"email": "performance@example.test", "password": password})
            if not client.cookie:
                raise ValueError("Login did not issue an access cookie")
            return client.cookie
        finally:
            client.close()

    def seed(self, catalogue, collection, password):
        self.secret_values.add(password)
        client = Client(self.port)
        try:
            client.request("/api/v1/auth/register", "POST",
                           dict(email="performance@example.test", display_name="Performance fixture",
                                password=password, password_confirmation=password, privacy_consent=True,
                                privacy_policy_version="performance-v1"))
        finally:
            client.close()
        self.sql("UPDATE users SET email_verified=TRUE, verification_token=NULL, "
                 "verification_token_expires_at=NULL WHERE id=1;")
        self.sql(fixture.fixture_sql(catalogue, collection))
        self.validate_counts(0)

    def validate_counts(self, foreign):
        actual = self.sql("SELECT COUNT(*), SUM(user_id=1), SUM(user_id<>1) FROM collection_entries;").strip()
        if actual != f"{fixture.OWN_ENTRIES + foreign}\t{fixture.OWN_ENTRIES}\t{foreign}":
            raise ValueError("Fixture row counts do not match the reference profile")
        self.sql("ANALYZE TABLE users, series, issues, persons, issue_persons, collection_entries;")

    def plans(self, cookie, references, scenarios=None):
        scenarios = fixture.scenarios() if scenarios is None else scenarios
        self.sql("SET GLOBAL general_log=OFF; SET GLOBAL log_output='TABLE'; "
                 "TRUNCATE TABLE mysql.general_log; SET GLOBAL general_log=ON;")
        client = None
        try:
            # MariaDB can leave parameters unexpanded for statements prepared while
            # logging was disabled. Recreate only the backend connection pool with
            # logging already enabled, outside all latency measurements.
            self.compose("restart", "backend")
            self.compose("up", "--detach", "--no-build", "--wait", "--wait-timeout", "180", "backend", timeout=240)
            # Docker may assign a new ephemeral host port when restarting.
            self.refresh_backend_port()
            client = Client(self.port, cookie)
            for name, params in scenarios:
                payload, _ = client.request("/api/v1/me/collection?" + urlencode(params))
                fixture.validate_response(payload, references[name])
        finally:
            if client is not None:
                client.close()
            self.sql("SET GLOBAL general_log=OFF;")
        queries = json.loads(self.sql(
            "SET SESSION group_concat_max_len=4194304; "
            "SELECT JSON_ARRAYAGG(argument ORDER BY event_time) FROM mysql.general_log "
            "WHERE command_type IN ('Execute','Query') AND argument LIKE 'SELECT%' "
            "AND (argument LIKE '%FROM collection_entries ce%' "
            "OR argument LIKE '%LEFT JOIN collection_entries ce%');"))
        if not isinstance(queries, list) or len(queries) != 2 * len(scenarios):
            raise ValueError("Could not capture the complete production SQL matrix")
        statements = []
        for query in queries:
            if not query.lstrip().upper().startswith("SELECT ") or ";" in query:
                raise ValueError("Unexpected SQL in read-only query capture")
            statements += [f"EXPLAIN FORMAT=JSON {query};", f"ANALYZE FORMAT=JSON {query};"]
        values = decode_json_stream(self.sql(statements))
        if len(values) != len(queries) * 2:
            raise ValueError("Missing EXPLAIN/ANALYZE output")
        return [dict(scenario=scenarios[i // 2][0], kind="count" if i % 2 == 0 else "results",
                     sql=query, explain=values[2 * i], analyze=values[2 * i + 1])
                for i, query in enumerate(queries)]

    def metadata(self):
        result = dict(host_os=platform.platform(), host_cpu=platform.processor(),
                      host_logical_cpus=os.cpu_count(), python=platform.python_version())
        for service in ("db", "backend"):
            container = self.compose("ps", "--quiet", service).strip()
            template = ('{"image_id":{{json .Image}},"cpu_limit":{{json .HostConfig.NanoCpus}},'
                        '"memory_limit_bytes":{{json .HostConfig.Memory}}}')
            result[service] = json.loads(command_output(["docker", "inspect", "--format", template, container]))
        result["database"] = self.sql("SELECT VERSION(), @@innodb_buffer_pool_size;").strip()
        if (result["backend"]["cpu_limit"] != 1_000_000_000
                or result["backend"]["memory_limit_bytes"] != 512 * 1024 * 1024
                or result["db"]["cpu_limit"] != 1_000_000_000
                or result["db"]["memory_limit_bytes"] != 1024 * 1024 * 1024
                or result["database"].split("\t")[-1] != str(256 * 1024 * 1024)):
            raise ValueError("Reference resource limits were not applied")
        result["docker_host"] = json.loads(command_output([
            "docker", "info", "--format", '{"cpus":{{json .NCPU}},"memory_bytes":{{json .MemTotal}},'
            '"os":{{json .OperatingSystem}},"kernel":{{json .KernelVersion}}}']))
        cpuinfo = self.compose("exec", "-T", "db", "cat", "/proc/cpuinfo")
        result["cpu_model"] = next((line.split(":", 1)[1].strip() for line in cpuinfo.splitlines()
                                    if line.startswith("model name")), platform.machine())
        result["docker"] = json.loads(command_output(["docker", "version", "--format", "{{json .}}" ]))
        result["backend_rust"] = (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")
        return result

    def close(self):
        try:
            # Explicit file/project limits cleanup to resources created by this instance.
            self.compose("--profile", "browser", "down", "--volumes", "--remove-orphans", timeout=120)
        finally:
            self.directory.cleanup()

    def browser_check(self, password):
        # Build and run browsers after the HTTP benchmark to keep its timing isolated.
        self.compose("--profile", "browser", "build", "frontend", timeout=1800)
        self.compose("--profile", "browser", "up", "--detach", "--no-build", "--wait",
                     "--wait-timeout", "180", "caddy", timeout=240)
        address = self.compose("--profile", "browser", "port", "caddy", "80").strip()
        if not address.startswith("127.0.0.1:"):
            raise ValueError("Browser reference stack must be bound to loopback")
        env = {**os.environ, "PLAYWRIGHT_BASE_URL": "http://" + address,
               "PLAYWRIGHT_LOGIN_EMAIL": "performance@example.test",
               "PLAYWRIGHT_LOGIN_PASSWORD": password,
               "PLAYWRIGHT_COLLECTION_TOTAL": str(fixture.OWN_ENTRIES), "PLAYWRIGHT_WORKERS": "1"}
        command = ["node", "node_modules/@playwright/test/cli.js", "test", "e2e/collection.spec.ts",
                   "--grep", "keeps filters usable and shows the latest query",
                   "--project=chromium", "--project=firefox", "--project=webkit", "--retries=0", "--reporter=line"]
        subprocess.run(command, cwd=ROOT / "frontend", env=env, check=True, timeout=300,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
        return dict(status="passed", collection_entries=fixture.OWN_ENTRIES,
                    browsers=["chromium", "firefox", "webkit"])


def command_output(command):
    return subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8",
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          check=True, timeout=30).stdout.strip()


def report_is_complete(report):
    config = benchmark_profile(report.get("profile", "full"))
    required = {(phase, name, mode) for phase in config["phases"]
                for name, _ in config["scenarios"] for mode in config["concurrency"]}
    actual = [(r["phase"], r["scenario"], r["concurrency"]) for r in report["measurements"]]
    if len(actual) != len(required) or set(actual) != required:
        raise ValueError("Incomplete or duplicated benchmark matrix")
    for result in report["measurements"]:
        check_measurement(result)
    if set(report["plans"]) != set(config["phases"]):
        raise ValueError("Missing SQL evidence for the benchmark profile")
    for phase in config["phases"]:
        check_plans(report["plans"][phase], scenarios=config["scenarios"],
                    enforce_user_scope=phase == "with-foreign")


def write_report(output, report):
    output.mkdir(parents=True, exist_ok=True)
    (output / "results.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    lines = ["# Collection search performance", "", f"Status: **{report['status']}**", "",
             f"Run: `{report['run_id']}`; commit: `{report.get('commit', 'unknown')}`", "",
             f"Profile: **{report.get('profile', 'full')}**", "",
             ("Selected regression scenarios; this smoke check does not establish full acceptance or scaling."
              if report.get("profile") == "smoke" else "Complete acceptance matrix with a scaling comparison."), "",
             "| Dataset | Scenario | Parallel | Samples | p50 ms | p95 ms | p99 ms |",
             "| --- | --- | ---: | ---: | ---: | ---: | ---: |"]
    for row in report["measurements"]:
        lines.append(f"| {row['phase']} | {row['scenario']} | {row['concurrency']} | {row['samples']} | "
                     f"{row['p50_ms']:.3f} | {row['p95_ms']:.3f} | {row['p99_ms']:.3f} |")
    if report.get("failure"):
        lines += ["", "Failure: " + report["failure"]]
    (output / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run(output, skip_build=False, browser_check=False, profile="full"):
    report = dict(run_id=uuid.uuid4().hex, profile=profile, status="failed", measurements=[], plans={},
                  started_at_utc=datetime.now(timezone.utc).isoformat(),
                  contract=dict(p95_limit_ms=LIMIT_MS, samples=SAMPLES, warmup=WARMUP, concurrency=MODES,
                                browser_check_required=browser_check))
    stack = None
    try:
        config = benchmark_profile(profile)
        scenarios = config["scenarios"]
        report["contract"].update(concurrency=config["concurrency"], phases=config["phases"],
                                  scenarios=[name for name, _ in scenarios])
        report["commit"] = command_output(["git", "rev-parse", "HEAD"])
        report["working_tree_dirty"] = bool(command_output(["git", "status", "--porcelain"]))
        catalogue = fixture.issues()
        collection = fixture.entries(catalogue)
        report["fixture"] = fixture.manifest(catalogue, collection)
        write_report(output, report)
        references = {name: fixture.expected(catalogue, collection, params) for name, params in scenarios}
        stack = Stack()
        stack.start(skip_build=skip_build)
        password = secrets.token_urlsafe(32)
        stack.seed(catalogue, collection, password)
        report["environment"] = stack.metadata()
        for phase in config["phases"]:
            if phase == "with-foreign":
                stack.sql(fixture.foreign_sql())
                stack.validate_counts(fixture.FOREIGN_ENTRIES)
            cookie = stack.login(password)
            report["plans"][phase] = stack.plans(cookie, references, scenarios=scenarios)
            write_report(output, report)
            check_plans(report["plans"][phase], scenarios=scenarios,
                        enforce_user_scope=phase == "with-foreign")
            # Refresh only between scenarios, never retry an individual measured request.
            login_at = time.monotonic()
            for name, params in scenarios:
                if time.monotonic() - login_at > 1800:
                    cookie = stack.login(password)
                    login_at = time.monotonic()
                for mode in config["concurrency"]:
                    report["active"] = dict(phase=phase, scenario=name, concurrency=mode)
                    write_report(output, report)
                    result = dict(phase=phase, scenario=name, params=params,
                                  **measure(stack.port, cookie, params, references[name], mode))
                    report["measurements"].append(result)
                    write_report(output, report)
                    print(f"{phase} {name} c={mode}: p95={result['p95_ms']:.3f} ms", flush=True)
                    check_measurement(result)
        report_is_complete(report)
        if browser_check:
            report["active"] = dict(phase="browser", browsers=["chromium", "firefox", "webkit"])
            write_report(output, report)
            report["browser_check"] = stack.browser_check(password)
        report.pop("active", None)
        report["status"] = "passed"
    except (Exception, KeyboardInterrupt) as error:
        # Subprocess exceptions can include arguments; print only the category and safe detail.
        report["failure"] = (str(error) if isinstance(error, ValueError) else type(error).__name__)
        captured = [getattr(error, name, None) for name in ("stdout", "stderr")]
        details = "\n".join(value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value
                            for value in captured if value)
        if details and stack is not None:
            for value in stack.secret_values:
                details = details.replace(value, "[redacted]")
            report["failure_details"] = details[-8000:]
        print("Benchmark failed: " + report["failure"], file=sys.stderr, flush=True)
    finally:
        if stack is not None:
            try:
                stack.close()
            except Exception as error:
                report["status"] = "failed"
                report["failure"] = "Cleanup failed: " + type(error).__name__
        report["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        write_report(output, report)
    return 0 if report["status"] == "passed" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "performance-results" / "collection")
    parser.add_argument("--skip-build", action="store_true", help="Use a revision-labelled image from a clean CI checkout")
    parser.add_argument("--profile", choices=("full", "smoke"), default="full",
                        help="Full acceptance (default) or 12 five-worker regression scenarios with foreign data")
    parser.add_argument("--browser-check", action="store_true", help="Also verify the large collection in all three Playwright browsers")
    args = parser.parse_args()
    return run(args.output.resolve(), skip_build=args.skip_build, browser_check=args.browser_check, profile=args.profile)


if __name__ == "__main__":
    sys.exit(main())
