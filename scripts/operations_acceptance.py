#!/usr/bin/env python3
"""Verify installation, local backups and recovery using disposable production containers."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import http.client
from http.cookies import SimpleCookie
import json
import os
import platform
from pathlib import Path
import re
import secrets
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit
import uuid
import zlib

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "deploy/scripts"))
import operations as ops

MAILPIT = "axllent/mailpit:v1.34.0@sha256:23a2ef85bc50853ab3093645d1d64679afd8887e89da5e48a88940c172591a4e"
MEDIA_IMAGE = "caddy:2.11.9-alpine@sha256:003a97808e781aa9f7e75cc2f43d34d461099031faf5e20faefb9f61126836f0"
SMTP_TEST_PASSWORD = 'literal $HOME ${UNSET_VARIABLE} $$ \\ quote"'


def command(arguments, *, env=None, input=None, check=True):
    return subprocess.run([str(value) for value in arguments], cwd=ROOT, env=env,
                          input=input, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=check)


def free_port():
    if "LILLY_ACCEPTANCE_PORTS" in os.environ:
        value, _, remaining = os.environ["LILLY_ACCEPTANCE_PORTS"].partition(",")
        try:
            port = int(value)
        except ValueError as error:
            raise RuntimeError("The reserved host-port pool is empty or invalid") from error
        if not 1024 <= port <= 65535:
            raise RuntimeError("Invalid reserved host port")
        os.environ["LILLY_ACCEPTANCE_PORTS"] = remaining
        return port
    with socket.socket() as connection:
        connection.bind(("127.0.0.1", 0))
        return connection.getsockname()[1]


class Client:
    def __init__(self, port, app_origin=True):
        self.port = port
        self.cookies = SimpleCookie()
        self.app_origin = app_origin

    def request(self, path, method="GET", body=None, expected=200, content_type=None):
        headers = {"Cookie": "; ".join(f"{key}={value.value}" for key, value in self.cookies.items())}
        if self.app_origin:
            headers["Origin"] = f"http://localhost:{self.port}"
        if body is not None and not isinstance(body, bytes):
            body = json.dumps(body).encode()
            content_type = "application/json"
        if content_type:
            headers["Content-Type"] = content_type
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        try:
            connection.request(method, path, body, headers)
            response = connection.getresponse()
            data = response.read()
            for name, value in response.getheaders():
                if name.lower() == "set-cookie":
                    self.cookies.load(value)
            if response.status != expected:
                # Bodies and cookies may contain credentials; never include them in diagnostics.
                code = ""
                try:
                    candidate = json.loads(data).get("code")
                    if isinstance(candidate, str) and re.fullmatch(r"[A-Z_]{1,64}", candidate):
                        code = " (" + candidate + ")"
                except (ValueError, AttributeError):
                    pass
                raise RuntimeError(f"{method} {urlsplit(path).path}: expected HTTP {expected}, got {response.status}{code}")
            return json.loads(data) if "application/json" in response.getheader("Content-Type", "") else data
        finally:
            connection.close()


def png():
    def chunk(kind, value):
        return struct.pack("!I", len(value)) + kind + value + struct.pack("!I", zlib.crc32(kind + value))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!2I5B", 2, 2, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\x00\xff\x00\x00\x00\xff\x00" * 2)) + chunk(b"IEND", b""))


class Stack:
    def __init__(self, backend, frontend):
        self.project = "lilly-ops-" + uuid.uuid4().hex[:12]
        self.base = Path(os.environ.get("LILLY_ACCEPTANCE_STATE", tempfile.gettempdir())).resolve()
        self.root = Path(tempfile.mkdtemp(prefix=self.project + "-", dir=self.base))
        self.release_id = uuid.uuid4().hex + uuid.uuid4().hex[:8]
        self.release = self.root / "releases" / self.release_id
        self.shared = self.root / "shared"
        self.port, self.mail_port, self.smtp_port = free_port(), free_port(), free_port()
        self.backend, self.frontend = backend, frontend
        self.network = self.project + "-proxy"
        self.network_created = False
        self.model_ready = False

    def host_path(self, path):
        host_root = os.environ.get("LILLY_ACCEPTANCE_HOST_ROOT")
        return str(Path(host_root) / path.resolve().relative_to(self.base)) if host_root else str(path)

    def environment(self):
        return {**os.environ, "LILLY_ROOT": str(self.root), "LILLY_PROJECT_NAME": self.project,
                "LILLY_PROXY_NETWORK": self.network, "LILLY_ISOLATED_TEST": "1",
                "LILLY_LEDGER_PATH": self.host_path(self.shared / "erasure-ledger")}

    def compose(self, *arguments, input=None, check=True):
        return command(ops.compose_command(self.root, self.release / "docker-compose.production.yml", self.project) + list(arguments),
                       env=self.environment(), input=input, check=check)

    def sql(self, statement):
        result = self.compose("exec", "-T", "db", "sh", "-ec",
                              'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb --batch --raw --skip-column-names "$MARIADB_DATABASE"',
                              input=statement.encode())
        return result.stdout.decode().strip()

    def ledger(self, *arguments):
        return command(["docker", "run", "--rm", "--volume",
                        self.host_path(self.shared / "erasure-ledger") + ":/ledger", MEDIA_IMAGE, *arguments])

    def ledger_permissions(self):
        self.ledger("sh", "-ec", "chown 999:999 /ledger/account-erasure.log; chgrp 999 /ledger; chmod 0770 /ledger; chmod 0600 /ledger/account-erasure.log")

    def initialize(self):
        shutil.copytree(ROOT / "deploy", self.release, ignore=shutil.ignore_patterns(".env.production", ".env.operations", ".env.smtp", "backups", "__pycache__"))
        self.shared.mkdir(mode=0o700)
        smtp = self.root / "smtp.env"
        smtp_values = {"SMTP_HOST": "127.0.0.1", "SMTP_PORT": str(self.smtp_port), "SMTP_TLS_MODE": "none",
                       "SMTP_FROM": "lilly@example.test", "SMTP_USER": "acceptance-only",
                       "SMTP_PASSWORD": SMTP_TEST_PASSWORD}
        ops.private_write(smtp, "".join(ops.env_line(key, value) for key, value in smtp_values.items()))
        import pwd
        command([sys.executable, self.release / "scripts/configure-environment.py", "--smtp-config", smtp,
                 "--output", self.shared / ".env.production", "--owner", pwd.getpwuid(os.getuid()).pw_name,
                 "--app-base-url", f"http://localhost:{self.port}"], env=self.environment())
        ops.private_write(self.shared / ".env.operations", f'BACKUP_DIR={json.dumps(str(self.root / "backups"))}\nBACKUP_ALERT_EMAILS=ops@example.test\n')
        ops.private_write(self.shared / "erasure-ledger/account-erasure.log", b"")
        self.ledger_permissions()
        self.materialize()

    def materialize(self):
        ops.private_write(self.shared / ".deployment.env",
                          f"LILLY_IMAGE_TAG={self.release_id}\nLILLY_RESOURCE_PREFIX={self.project}\nLILLY_BIND_ADDRESS=127.0.0.1\nLILLY_HOST_PORT={self.port}\n")
        # Only fixture transport/namespace settings differ from the production model.
        model = json.loads(command(ops.compose_command(self.root, ROOT / "deploy/docker-compose.production.yml", self.project)
                                   + ["config", "--format", "json"], env=self.environment()).stdout)
        model.pop("name", None)
        model["services"]["backend"]["image"] = self.backend
        model["services"]["frontend"]["image"] = self.frontend
        model["services"]["backend"]["environment"].update(SMTP_HOST="mailpit", SMTP_PORT="1025", SMTP_TLS_MODE="none")
        model["services"]["caddy"]["labels"] = {"traefik.enable": "false"}
        for service in model["services"].values():
            for volume in service.get("volumes", []):
                if volume["type"] == "bind":
                    source = Path(volume["source"])
                    if volume["target"] == "/etc/caddy/Caddyfile":
                        volume["source"] = self.host_path(self.release / "Caddyfile")
                    elif source.is_relative_to(self.root):
                        volume["source"] = self.host_path(source)
        model["services"]["mailpit"] = {"image": MAILPIT, "networks": ["app"],
            "command": ["--smtp-auth-accept-any", "--smtp-auth-allow-insecure", "--disable-version-check"],
            "ports": [{"target": 8025, "published": str(self.mail_port), "host_ip": "127.0.0.1"},
                      {"target": 1025, "published": str(self.smtp_port), "host_ip": "127.0.0.1"}]}
        ops.private_write(self.release / "docker-compose.production.yml", json.dumps(model))
        command(["docker", "network", "create", "--label", "lilly.operations=" + self.project, self.network])
        self.network_created = True
        self.model_ready = True
        command([sys.executable, self.release / "scripts/configure-operations.py", "--root", self.root,
                 "--systemd-directory", self.root / "units"], env=self.environment())

    def register(self, email):
        client = Client(self.port)
        password = secrets.token_urlsafe(32) + "!aA7"
        options = client.request("/api/v1/auth/options")
        client.request("/api/v1/auth/register", "POST", {
            "email": email, "display_name": "Operations Acceptance", "password": password,
            "password_confirmation": password, "privacy_consent": True,
            "privacy_policy_version": options["privacy_policy"]["version"]}, expected=201)
        messages = Client(self.mail_port, app_origin=False).request("/api/v1/messages")["messages"]
        message = next(item for item in messages if any(recipient["Address"] == email for recipient in item["To"]))
        detail = Client(self.mail_port, app_origin=False).request("/api/v1/message/" + message["ID"])
        token = re.search(r"/api/v1/auth/verify\?token=([A-Za-z0-9_-]+)", detail["HTML"])
        if token is None:
            raise RuntimeError("Registration did not deliver a verification link")
        client.request("/api/v1/auth/verify?token=" + token[1], expected=303)
        client.request("/api/v1/auth/login", "POST", {"email": email, "password": password})
        return client, password

    def photo(self, client, entry_id):
        boundary = "lilly-operations-" + uuid.uuid4().hex
        body = (f'--{boundary}\r\nContent-Disposition: form-data; name="photo"; filename="photo.png"\r\nContent-Type: image/png\r\n\r\n'.encode()
                + png() + f"\r\n--{boundary}--\r\n".encode())
        return client.request(f"/api/v1/me/collection/{entry_id}/photos", "POST", body, 201,
                              "multipart/form-data; boundary=" + boundary)

    def backup(self, check=True):
        return command(["bash", self.release / "scripts/backup.sh", "manual"], env=self.environment(), check=check)

    def cleanup(self):
        if self.model_ready:
            model = json.loads((self.release / "docker-compose.production.yml").read_text())
            if any(not volume["name"].startswith(self.project + "_") for volume in model["volumes"].values()):
                raise RuntimeError("Refusing cleanup of unowned volumes")
            self.compose("down", "--volumes", "--remove-orphans")
        if self.network_created:
            network = json.loads(command(["docker", "network", "inspect", self.network]).stdout)[0]
            if network["Labels"].get("lilly.operations") != self.project:
                raise RuntimeError("Refusing cleanup of an unowned network")
            command(["docker", "network", "rm", self.network])
        if self.root.resolve().parent != self.base or not self.root.name.startswith(self.project + "-"):
            raise RuntimeError("Refusing cleanup outside the acceptance directory")
        shutil.rmtree(self.root)


def verify_recovery(source, target, admin, password, entry, photo, erased_email):
    snapshot = next(path for path in (source.root / "backups").iterdir() if (path / "COMPLETE").exists())
    latest = source.root / "latest-live-ledger"
    ops.private_write(latest, source.ledger("cat", "/ledger/account-erasure.log").stdout)
    ops.recover_configuration(snapshot, target.root, latest)
    target.release_id = ops.verify_snapshot(snapshot)["infrastructure_release"]
    target.release = target.root / "releases" / target.release_id
    target.shared = target.root / "shared"
    target.ledger_permissions()
    environment = ops.read_env(target.shared / ".env.production", compose=True)
    environment.update(APP_BASE_URL=f"http://localhost:{target.port}", SMTP_PORT=str(target.smtp_port))
    ops.private_write(target.shared / ".env.production", "".join(ops.env_line(key, value, compose=True) for key, value in environment.items()))
    target.materialize()
    copied = target.root / "backups" / snapshot.name
    copied.parent.mkdir(exist_ok=True)
    shutil.copytree(snapshot, copied)
    target.compose("up", "-d", "--wait", "db")
    command(["bash", target.release / "scripts/restore.sh", "--backup", copied, "--confirm", "RESTORE_LILLY"], env=target.environment())
    if not (target.root / "current").is_symlink() or (target.root / "current").resolve() != target.release:
        raise RuntimeError("Fresh-host restore did not record its healthy application configuration")
    client = Client(target.port)
    client.request("/api/v1/auth/login", "POST", {"email": admin, "password": password})
    entries = client.request("/api/v1/me/collection")["data"]
    restored = next(item for item in entries if item["id"] == entry["id"])
    if restored["notes"] != "before-backup" or restored["edition_label"] != "Acceptance edition":
        raise RuntimeError("Restored collection differs from the independent baseline")
    content = client.request(photo["content_url"])
    if hashlib.sha256(content).hexdigest() != photo["expected_hash"]:
        raise RuntimeError("Restored photo differs from the independent baseline")
    if target.sql("SELECT COUNT(*) FROM users WHERE email='" + erased_email + "';") != "0":
        raise RuntimeError("Ledger replay resurrected an erased account")
    if target.sql("SELECT COUNT(*) FROM collection_photos;") != "1":
        raise RuntimeError("Ledger replay failed to remove erased-user photos")
    stored_files = command(["docker", "run", "--rm", "--volume", target.project + "_media_data:/media:ro",
                            MEDIA_IMAGE, "find", "/media", "-type", "f", "-name", "*.jpg"]).stdout.splitlines()
    if len(stored_files) != 1:
        raise RuntimeError("Ledger replay left erased-user photo files in storage")
    client.request("/api/v1/admin/series")


def verify_failed_deployment(stack):
    """Run the actual deploy script against Docker; only an unavailable init bus is simulated."""
    candidate_id = uuid.uuid4().hex + uuid.uuid4().hex[:8]
    candidate = stack.root / "releases" / candidate_id
    shutil.copytree(ROOT / "deploy", candidate,
                    ignore=shutil.ignore_patterns(".env.production", ".env.operations", ".env.smtp", "backups", "__pycache__"))
    model = json.loads((stack.release / "docker-compose.production.yml").read_text())
    model["services"]["backend"].update(command=["/bin/false"], restart="no")
    model["services"]["backend"]["healthcheck"].update(interval="1s", timeout="1s", retries=1, start_period="0s")
    ops.private_write(candidate / "docker-compose.production.yml", json.dumps(model))
    environment = stack.environment()
    if not Path("/run/systemd/system").is_dir():
        # Docker Desktop's controller is not a systemd host. The unit-inspection
        # failure/success paths are tested separately; all Docker operations stay real.
        binaries = stack.root / "controller-bin"
        binaries.mkdir()
        stub = binaries / "systemctl"
        stub.write_text('#!/bin/bash\n[[ "$2" == --property=LoadState ]] || exit 1\nprintf "not-found\\n"\n')
        stub.chmod(0o700)
        environment["PATH"] = str(binaries) + os.pathsep + environment["PATH"]
    before = stack.sql("SELECT VERSION();")
    result = command(["bash", candidate / "scripts/deploy.sh", candidate_id], env=environment, check=False)
    if result.returncode == 0:
        raise RuntimeError("A failed backend candidate was published as successful")
    if ((stack.root / "current").resolve() != stack.release
            or (stack.root / "infrastructure").resolve() != candidate
            or (stack.shared / "backup-tools").resolve() != candidate / "scripts"):
        raise RuntimeError("Application rollback lost the infrastructure or backup-tool reference")
    if stack.sql("SELECT VERSION();") != before:
        raise RuntimeError("Application rollback changed the database server")
    Client(stack.port).request("/api/v1/health")
    command(["bash", stack.shared / "backup.sh", "manual"], env=stack.environment())


def acceptance(backend, frontend, report):
    stacks = []
    checks = []
    result = {"started_utc": datetime.now(timezone.utc).isoformat(), "checks": checks,
              "backend_image": backend, "frontend_image": frontend, "status": "failed",
              "python_version": platform.python_version(), "system": platform.system(),
              "host_commissioning": "not_performed"}
    try:
        source = Stack(backend, frontend)
        stacks.append(source)
        result["backend_image_id"] = command(["docker", "image", "inspect", backend, "--format", "{{.Id}}"]).stdout.decode().strip()
        result["frontend_image_id"] = command(["docker", "image", "inspect", frontend, "--format", "{{.Id}}"]).stdout.decode().strip()
        source.initialize()
        source.compose("up", "-d", "--wait", "--wait-timeout", "180")
        Client(source.port).request("/api/v1/health")
        actual_password = source.compose("exec", "-T", "backend", "printenv", "SMTP_PASSWORD").stdout.decode().rstrip("\n")
        if actual_password != SMTP_TEST_PASSWORD:
            raise RuntimeError("SMTP configuration changed during Compose interpolation")
        for reference in ("current", "infrastructure"):
            (source.root / reference).symlink_to(source.release, target_is_directory=True)
        checks.append("fresh production-container startup, literal SMTP credentials and configured timer calendar")
        admin_email, erased_email = "operator@example.test", "erased@example.test"
        admin, password = source.register(admin_email)
        source.compose("exec", "-T", "backend", "./lilly-backend", "admin", "promote", "--email", admin_email)
        admin.request("/api/v1/auth/login", "POST", {"email": admin_email, "password": password})
        admin.request("/api/v1/admin/series")
        erased, erased_password = source.register(erased_email)
        checks.append("registration, real verification mail, login and administrator bootstrap")
        # Catalogue-only fixture. Accounts, collection entries and photos always use the API.
        source.sql("INSERT INTO series(id,name,slug,active,total_issues) VALUES(1,'Operations fixture','operations-fixture',1,1);\n"
                   "INSERT INTO issues(id,series_id,issue_number,title,published_at) VALUES(1,1,1,'Operations fixture', '2000-01-01');\n"
                   "INSERT INTO persons(id,name) VALUES(1,'Fixture author');\nINSERT INTO issue_persons VALUES(1,1,'author');\n")
        entry = admin.request("/api/v1/me/collection", "POST", {"issue_id": 1, "condition_grade": "Z1", "status": "owned", "notes": "before-backup", "edition_label": "Acceptance edition"}, 201)
        photo = source.photo(admin, entry["id"])
        photo["expected_hash"] = hashlib.sha256(admin.request(photo["content_url"])).hexdigest()
        erased_entry = erased.request("/api/v1/me/collection", "POST", {"issue_id": 1, "condition_grade": "Z2", "status": "owned"}, 201)
        source.photo(erased, erased_entry["id"])
        source.compose("restart", "backend")
        source.compose("up", "-d", "--wait", "--wait-timeout", "120")
        admin.request(photo["content_url"])
        checks.append("persistent collection and actual photo upload survive service restart")
        source.backup()
        admin.request(f'/api/v1/me/collection/{entry["id"]}', "PATCH", {"notes": "after-backup"})
        erased.request("/api/v1/auth/reauth/password", "POST", {"password": erased_password})
        erased.request("/api/v1/me/account-deletion", "POST", {"confirmation": "KONTO LÖSCHEN"}, 202)
        # Advance only the synthetic deletion job clock; production grace periods stay intact.
        source.sql("UPDATE account_erasure_jobs SET scheduled_for=UTC_TIMESTAMP()-INTERVAL 1 DAY WHERE user_id IS NOT NULL;")
        source.compose("restart", "backend")
        source.compose("up", "-d", "--wait", "--wait-timeout", "120")
        deadline = time.monotonic() + 15
        while source.sql("SELECT COUNT(*) FROM users WHERE email='" + erased_email + "';") != "0":
            if time.monotonic() >= deadline:
                raise RuntimeError("Synthetic account erasure did not finish before recovery")
            time.sleep(0.1)
        target = Stack(backend, frontend)
        stacks.append(target)
        verify_recovery(source, target, admin_email, password, entry, photo, erased_email)
        checks.append("fresh-target database and media restore, independent photo hash, current ledger replay")
        source.compose("stop", "db")
        health = Client(source.port)
        health.request("/api/v1/health", expected=503)
        health.request("/api/v1/health/live")
        failed = source.backup(check=False)
        if failed.returncode == 0:
            raise RuntimeError("Backup incorrectly succeeded without its database")
        messages = Client(source.mail_port, app_origin=False).request("/api/v1/messages")["messages"]
        if not any(item["Subject"] == "LILLY backup failed" and item["To"][0]["Address"] == "ops@example.test" for item in messages):
            raise RuntimeError("Database failure did not deliver an operations email")
        source.compose("up", "-d", "--wait", "--wait-timeout", "120")
        health.request("/api/v1/health")
        checks.append("readiness failure/recovery, independent liveness and SMTP alert while database is stopped")
        verify_failed_deployment(source)
        checks.append("actual failed candidate rollback preserves database, infrastructure and working backup tools")
        result["status"] = "passed"
    except Exception as error:
        result["failure"] = type(error).__name__
        print(f"Operations acceptance failed: {type(error).__name__}", file=sys.stderr)
        if isinstance(error, RuntimeError):
            print(str(error), file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            diagnostic = (error.stderr or b"").decode("utf-8", errors="replace")
            for stack in stacks:
                configuration = stack.shared / ".env.production"
                if configuration.exists():
                    for key, value in ops.read_env(configuration, compose=True).items():
                        if value and any(part in key for part in ("PASSWORD", "SECRET", "TOKEN", "DATABASE_URL")):
                            diagnostic = diagnostic.replace(value.replace("$", "$$"), "[redacted]")
                            diagnostic = diagnostic.replace(value, "[redacted]")
            print(diagnostic[-3000:], file=sys.stderr)
    finally:
        for stack in reversed(stacks):
            try:
                stack.cleanup()
            except Exception as error:
                result["status"] = "failed"
                result["cleanup_failure"] = type(error).__name__
        result["finished_utc"] = datetime.now(timezone.utc).isoformat()
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f'Operations acceptance: {result["status"]}; {len(checks)} verified stages. Report: {report}')
    return 0 if result["status"] == "passed" else 1


def windows_controller(arguments, coverage=False):
    """Run Linux host scripts on Windows using a dedicated disposable state volume."""
    image = "lilly-operations-tools"
    command(["docker", "build", "-f", "deploy/tests/Dockerfile", "-t", image, "."])
    volume = "lilly-ops-controller-" + uuid.uuid4().hex[:12]
    command(["docker", "volume", "create", "--label", "lilly.operations.controller=" + volume, volume])
    try:
        mountpoint = json.loads(command(["docker", "volume", "inspect", volume]).stdout)[0]["Mountpoint"]
        # Select in the Windows host namespace: Linux's ephemeral range can include
        # ports reserved by Hyper-V, which the Docker daemon cannot publish.
        ports = set()
        while len(ports) < 6:
            ports.add(free_port())
        invocation = ["python"]
        if coverage:
            invocation += ["-m", "coverage", "run", "--rcfile=deploy/tests/coverage.ini"]
        return subprocess.run(["docker", "run", "--rm", "--network", "host",
            "--mount", f"type=bind,source={ROOT},target=/workspace",
            "--mount", f"type=volume,source={volume},target=/state",
            "--mount", "type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock",
            "--env", "LILLY_ACCEPTANCE_STATE=/state", "--env", "LILLY_ACCEPTANCE_HOST_ROOT=" + mountpoint,
            "--env", "COVERAGE_FILE=/workspace/.tmp/.coverage-operations",
            "--env", "LILLY_ACCEPTANCE_PORTS=" + ",".join(str(port) for port in ports),
            image, *invocation, "scripts/operations_acceptance.py", *arguments], cwd=ROOT, check=False).returncode
    finally:
        inspected = json.loads(command(["docker", "volume", "inspect", volume]).stdout)[0]
        if inspected["Labels"].get("lilly.operations.controller") != volume:
            raise RuntimeError("Refusing cleanup of an unowned controller volume")
        command(["docker", "volume", "rm", volume])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", action="store_true", help="Build candidate backend and frontend images")
    parser.add_argument("--coverage", action="store_true", help="Measure host-tool and acceptance coverage")
    parser.add_argument("--backend-image", default="lilly-backend:operations")
    parser.add_argument("--frontend-image", default="lilly-frontend:operations")
    parser.add_argument("--report", type=Path, default=ROOT / ".tmp/operations-acceptance.json")
    args = parser.parse_args()
    if os.name == "nt":
        if args.build:
            command(["docker", "build", "--target", "production", "-f", "backend/Dockerfile", "-t", args.backend_image, "."])
            command(["docker", "build", "-f", "frontend/Dockerfile", "-t", args.frontend_image, "frontend"])
        forwarded = ["--backend-image", args.backend_image, "--frontend-image", args.frontend_image,
                     "--report", "/workspace/" + args.report.resolve().relative_to(ROOT).as_posix()]
        return windows_controller(forwarded, coverage=args.coverage)
    if args.coverage:
        arguments = [argument for argument in sys.argv[1:] if argument != "--coverage"]
        return subprocess.run([sys.executable, "-m", "coverage", "run", "--rcfile=deploy/tests/coverage.ini",
                               "scripts/operations_acceptance.py", *arguments], cwd=ROOT,
                              env={**os.environ, "COVERAGE_FILE": str(ROOT / ".tmp/.coverage-operations")}, check=False).returncode
    if args.build:
        command(["docker", "build", "--target", "production", "-f", "backend/Dockerfile", "-t", args.backend_image, "."])
        command(["docker", "build", "-f", "frontend/Dockerfile", "-t", args.frontend_image, "frontend"])
    return acceptance(args.backend_image, args.frontend_image, args.report)


if __name__ == "__main__":
    sys.exit(main())
