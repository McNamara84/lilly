"""Check recovery image safety and backup/restore coordination."""

import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import shlex
import signal
import subprocess
import tarfile
import tempfile
import time
import unittest


OLD_IMAGE = "mariadb:12.3.3@sha256:" + "a" * 64
NEW_IMAGE = "mariadb:13.0.2@sha256:" + "b" * 64


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "Restore uses Linux Bash")
class RestoreTests(unittest.TestCase):
    def setUp(self):
        self.temporary_directory = tempfile.TemporaryDirectory(prefix="lilly-restore-test-")
        self.addCleanup(self.temporary_directory.cleanup)
        self.root = Path(self.temporary_directory.name)
        self.deployment_root = self.root / "lilly"
        self.previous = self.deployment_root / "releases" / ("a" * 40)
        self.infrastructure = self.deployment_root / "releases" / ("b" * 40)
        self.shared = self.deployment_root / "shared"
        self.backup = self.deployment_root / "backups" / "test-backup"
        commands = self.root / "bin"
        for directory in (self.previous / "scripts", self.infrastructure / "scripts", self.shared / "erasure-ledger", self.backup, commands):
            directory.mkdir(parents=True)
        for release, image in ((self.previous, OLD_IMAGE), (self.infrastructure, NEW_IMAGE)):
            (release / "docker-compose.production.yml").write_text(f"services:\n  db:\n    image: {image}\n")
            (release / "Caddyfile").touch()
        (self.deployment_root / "current").symlink_to(self.previous, target_is_directory=True)
        (self.deployment_root / "infrastructure").symlink_to(self.infrastructure, target_is_directory=True)
        (self.shared / ".env.production").write_text("JWT_SECRET=test-only\n")
        (self.shared / ".env.operations").write_text("BACKUP_ALERT_EMAILS=ops@example.test\n")
        (self.shared / ".env.operations").chmod(0o600)
        self.previous_environment = "LILLY_IMAGE_TAG=" + "a" * 40 + "\n"
        (self.shared / ".deployment.env").write_text(self.previous_environment)
        self.ledger = self.shared / "erasure-ledger/account-erasure.log"
        self.ledger.write_text("live-erasure-ledger\n")
        (self.backup / "COMPLETE").touch()
        (self.backup / "database.sql.gz").write_bytes(gzip.compress(b"SELECT 1;\n"))
        with tarfile.open(self.backup / "media.tar.gz", "w:gz") as archive:
            item = tarfile.TarInfo("example.txt")
            item.size = 5
            archive.addfile(item, io.BytesIO(b"media"))
        (self.backup / "account-erasure.log").write_text("older-erasure-ledger\n")
        (self.backup / "SHA256SUMS").write_text("".join(
            f"{hashlib.sha256((self.backup / name).read_bytes()).hexdigest()}  {name}\n"
            for name in ("database.sql.gz", "media.tar.gz", "account-erasure.log")
        ))
        self.database_image = self.root / "database-image"
        self.database_image.write_text(NEW_IMAGE)
        self.command_log = self.root / "docker.jsonl"
        self.env = {
            **os.environ,
            "PATH": str(commands) + os.pathsep + os.environ["PATH"],
            "LILLY_ROOT": str(self.deployment_root),
            "TEST_DATABASE_IMAGE": str(self.database_image),
            "TEST_DOCKER_LOG": str(self.command_log),
            "TEST_DATABASE_STATE": "running",
            "TEST_LOCK_BUSY": "0",
            "TEST_BACKUP_LOCK_BUSY": "0",
            "TEST_LOCK_ATTEMPTS": str(self.root / "lock-attempts"),
            "TEST_INSPECT_FAILURE": "0",
            "TEST_REPLAY_FAILURE": "0",
            "TEST_SYSTEMD_LOAD_STATE": "loaded",
            "TEST_SYSTEMD_ROUTE": "shared",
            "TEST_SYSTEMD_ACTIVE_STATE": "inactive",
            "TEST_SYSTEMD_FAILURE": "0",
            "TEST_LEGACY_BACKUP": str(self.root / "legacy-backup-used"),
        }

        def executable(name, source):
            target = commands / name
            target.write_text(source, encoding="utf-8")
            target.chmod(0o755)

        executable("flock", r'''#!/bin/bash
if [[ -n "${TEST_REAL_FLOCK:-}" ]]; then
    printf '%s\n' "${@: -1}" >> "$TEST_LOCK_ATTEMPTS"
    exec "$TEST_REAL_FLOCK" "$@"
fi
[[ "$TEST_LOCK_BUSY" != 1 ]] || exit 1
[[ "$TEST_BACKUP_LOCK_BUSY" != 1 || "${@: -1}" != 9 ]]
''')
        executable("curl", "#!/bin/bash\nexit 0\n")
        executable("sleep", "#!/bin/bash\nexit 0\n")
        executable("systemctl", r'''#!/bin/bash
[[ "$TEST_SYSTEMD_FAILURE" != 1 ]] || exit 1
case "$2" in
  --property=LoadState)
    if [[ "${@: -1}" == lilly-backup-failure.service ]]; then
      printf '%s\n' "${TEST_SYSTEMD_HANDLER_LOAD_STATE-loaded}"
    else
      printf '%s\n' "$TEST_SYSTEMD_LOAD_STATE"
    fi
    ;;
  --property=ExecStart)
    if [[ "$TEST_SYSTEMD_ROUTE" == shared ]]; then
      path="$LILLY_ROOT/shared/backup.sh"
    else
      path="$LILLY_ROOT/$TEST_SYSTEMD_ROUTE/scripts/backup.sh"
    fi
    printf '{ path=%s ; argv[]=%s daily ; }\n' "$path" "$path"
    ;;
  --property=ActiveState) printf '%s\n' "$TEST_SYSTEMD_ACTIVE_STATE" ;;
  --property=OnFailure) printf '%s\n' "${TEST_SYSTEMD_FAILURE_HANDLER-lilly-backup-failure.service}" ;;
  *) exit 94 ;;
esac
''')
        executable("docker", r'''#!/usr/bin/env python3
import json
import io
import os
from pathlib import Path
import re
import sys
import tarfile
import time

def wait_for_release(operation):
    ready = os.environ.get("TEST_" + operation + "_READY")
    if not ready:
        return
    Path(ready).touch()
    deadline = time.monotonic() + 30
    while not Path(os.environ["TEST_" + operation + "_RESUME"]).exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("Test did not release " + operation)
        time.sleep(0.01)

args = sys.argv[1:]
with open(os.environ["TEST_DOCKER_LOG"], "a") as log:
    log.write(json.dumps(args) + "\n")
database = Path(os.environ["TEST_DATABASE_IMAGE"])
if args[:2] == ["network", "inspect"]:
    assert args[2] == "proxy", args
elif args[0] == "inspect":
    if os.environ["TEST_INSPECT_FAILURE"] == "1":
        sys.exit(1)
    if args[2] == "{{.Config.Image}}":
        print(database.read_text())
    elif args[2] == "{{.State.Running}}":
        print("true")
    elif args[2] == "{{.State.Paused}}":
        print("false")
    else:
        raise AssertionError(args)
elif args[0] == "compose":
    config_index = args.index("--file")
    config = Path(args[config_index + 1])
    command = args[config_index + 2:]
    image = re.search(r"image:\s*(\S+)", config.read_text()).group(1)
    if command[0] == "config":
        if command[1:] == ["--images", "db"]:
            print(image)
        else:
            assert command[1:] == ["--quiet"], command
    elif command[0] == "ps":
        if os.environ["TEST_DATABASE_STATE"] == "running":
            print("test-" + command[-1])
    elif command[0] == "up":
        if "--no-deps" not in command:
            database.write_text(image)
        if str(config.resolve()) == os.environ.get("TEST_CANDIDATE_COMPOSE"):
            wait_for_release("DEPLOY")
            if os.environ.get("TEST_DEPLOY_FAILURE") == "compose":
                sys.exit(1)
    elif command[0] == "exec":
        if any("DROP DATABASE" in argument for argument in command):
            wait_for_release("RESTORE")
        if any("mariadb-dump" in argument for argument in command):
            wait_for_release("BACKUP")
            if os.environ.get("TEST_BACKUP_FAILURE") == "1":
                sys.exit(1)
            sys.stdout.write("SELECT 1;\n")
        sys.stdin.buffer.read()
    elif command[0] == "run":
        if os.environ["TEST_REPLAY_FAILURE"] == "1":
            sys.exit(1)
    elif command[0] == "unpause" and os.environ.get("TEST_UNPAUSE_FAILURE") == "1":
        sys.exit(1)
    else:
        assert command[0] in ("stop", "pause", "unpause", "pull", "logs"), command
elif args[0] == "run":
    if "tar" in args:
        if os.environ.get("TEST_MEDIA_FAILURE") == "1":
            sys.exit(1)
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w:gz"):
            pass
        sys.stdout.buffer.write(stream.getvalue())
    elif "cat" in args:
        if os.environ.get("TEST_LEDGER_FAILURE") == "1":
            sys.exit(1)
        sys.stdout.buffer.write((Path(os.environ["LILLY_ROOT"]) / "shared/erasure-ledger/account-erasure.log").read_bytes())
    sys.stdin.buffer.read()
else:
    raise AssertionError(args)
''')
        script = Path(__file__).resolve().parents[2] / "deploy/scripts/restore.sh"
        self.script = self.infrastructure / "scripts/restore.sh"
        self.script.write_text(script.read_text(), encoding="utf-8")
        for source in script.parent.glob("*.py"):
            shutil.copyfile(source, self.infrastructure / "scripts" / source.name)
        (self.shared / "backup-tools").symlink_to(self.infrastructure / "scripts", target_is_directory=True)
        backup_source = script.with_name("backup.sh").read_text()
        self.backup_script = self.shared / "backup.sh"
        self.backup_script.write_text(backup_source, encoding="utf-8")
        self.backup_script.chmod(0o700)
        for release in (self.previous, self.infrastructure):
            legacy = release / "scripts/backup.sh"
            legacy.write_text('#!/bin/bash\nprintf legacy > "$TEST_LEGACY_BACKUP"\nexit 99\n')
            legacy.chmod(0o755)

    def restore(self):
        return subprocess.run(
            ["bash", str(self.script), "--backup", str(self.backup), "--confirm", "RESTORE_LILLY"],
            env=self.env, input="", capture_output=True, text=True, timeout=20,
        )

    def docker_calls(self):
        if not self.command_log.exists():
            return []
        return [json.loads(line) for line in self.command_log.read_text().splitlines()]

    def use_real_locks(self):
        real_flock = shutil.which("flock")
        if real_flock is None:
            self.skipTest("Concurrency tests require flock")
        self.env["TEST_REAL_FLOCK"] = real_flock

    def start_restore(self):
        return self.start_operation(["bash", str(self.script), "--backup", str(self.backup), "--confirm", "RESTORE_LILLY"])

    def start_operation(self, arguments):
        process = subprocess.Popen(
            arguments,
            env=self.env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, start_new_session=True,
        )

        def cleanup():
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            process.communicate(timeout=5)

        self.addCleanup(cleanup)
        return process

    def wait_for(self, condition, process):
        # Startup includes several instrumented subprocesses. This bounds a
        # synchronization wait rather than measuring deployment performance.
        deadline = time.monotonic() + 15
        while not condition():
            if process.poll() is not None:
                stdout, stderr = process.communicate()
                self.fail("Operation exited before the expected synchronization point:\n" + stdout + stderr)
            if time.monotonic() >= deadline:
                self.fail("Operation did not reach the expected synchronization point")
            time.sleep(0.01)

    def assert_locks_available(self):
        import fcntl

        for path in (self.shared / ".deploy.lock", self.backup.parent / ".backup.lock"):
            with path.open("a") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def prepare_deployment(self):
        candidate = self.deployment_root / "releases" / ("c" * 40)
        (candidate / "scripts").mkdir(parents=True)
        shutil.copyfile(self.infrastructure / "docker-compose.production.yml", candidate / "docker-compose.production.yml")
        (candidate / "Caddyfile").touch()
        sources = Path(__file__).resolve().parents[2] / "deploy/scripts"
        for source in sources.glob("*.sh"):
            target = candidate / "scripts" / source.name
            target.write_text(source.read_text(), encoding="utf-8")
            target.chmod(0o755)
        for source in sources.glob("*.py"):
            shutil.copyfile(source, candidate / "scripts" / source.name)
        self.env["TEST_CANDIDATE_COMPOSE"] = str(candidate / "docker-compose.production.yml")
        return candidate

    def deploy(self, candidate):
        return subprocess.run(
            ["bash", str(candidate / "scripts/deploy.sh"), candidate.name],
            env=self.env, input="", capture_output=True, text=True, timeout=10,
        )

    def service_backup_arguments(self):
        unit = Path(__file__).resolve().parents[2] / "deploy/systemd/lilly-backup.service"
        command = next(line.removeprefix("ExecStart=") for line in unit.read_text().splitlines() if line.startswith("ExecStart="))
        return ["bash", *shlex.split(command.replace("/opt/lilly", str(self.deployment_root)))]

    def assert_refused_before_changes(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        for args in self.docker_calls():
            if args[0] == "compose":
                command = args[args.index("--file") + 2]
                self.assertIn(command, ("config", "ps"), args)
            else:
                self.assertEqual(args[0], "inspect", args)

    def test_restore_after_application_rollback_keeps_upgraded_database(self):
        result = self.restore()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.database_image.read_text(), NEW_IMAGE)
        self.assertEqual((self.deployment_root / "current").resolve(), self.previous)
        self.assertEqual((self.shared / ".deployment.env").read_text(), self.previous_environment)
        self.assertEqual(self.ledger.read_text(), "live-erasure-ledger\n")
        calls = self.docker_calls()
        replay = next(args for args in calls if "replay-erasure-ledger" in args)
        self.assertEqual(replay[-4:], ["backend", "./lilly-backend", "privacy", "replay-erasure-ledger"])
        for args in calls:
            if args[0] == "compose":
                selected = Path(args[args.index("--file") + 1])
                self.assertEqual(selected.resolve(), self.infrastructure / "docker-compose.production.yml")

    def test_old_configuration_refuses_to_restart_upgraded_database(self):
        pointer = self.deployment_root / "infrastructure"
        pointer.unlink()
        pointer.symlink_to(self.previous, target_is_directory=True)
        self.assert_refused_before_changes(self.restore())
        self.assertEqual(self.database_image.read_text(), NEW_IMAGE)

    def test_fresh_host_restore_records_healthy_configuration_for_the_next_deployment(self):
        (self.deployment_root / "current").unlink()
        result = self.restore()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((self.deployment_root / "current").resolve(), self.infrastructure)
        self.assertEqual((self.shared / ".deployment.env").read_text(), self.previous_environment)

    def test_upgrade_configuration_refuses_to_restart_older_database(self):
        self.database_image.write_text(OLD_IMAGE)
        self.assert_refused_before_changes(self.restore())
        self.assertEqual(self.database_image.read_text(), OLD_IMAGE)

    def test_different_digest_of_same_version_is_rejected(self):
        image = NEW_IMAGE.replace("b" * 64, "c" * 64)
        self.database_image.write_text(image)
        self.assert_refused_before_changes(self.restore())
        self.assertEqual(self.database_image.read_text(), image)

    def test_stopped_or_missing_database_is_not_started_implicitly(self):
        for state in ("stopped", "missing"):
            with self.subTest(state=state):
                self.env["TEST_DATABASE_STATE"] = state
                self.assert_refused_before_changes(self.restore())

    def versioned_snapshot(self, database_image):
        # Version 2 is the already-published pre-upgrade format under review.
        configuration = self.backup / "configuration"
        configuration.mkdir()
        for source, destination in ((".env.production", "production.env"),
                                    (".deployment.env", "deployment.env"), (".env.operations", "operations.env")):
            shutil.copyfile(self.shared / source, configuration / destination)
        (self.backup / "metadata.env").write_text("kind=pre-deploy\n")
        with tarfile.open(self.backup / "deployment.tar.gz", "w:gz") as archive:
            archive.add(self.previous / "docker-compose.production.yml", arcname="docker-compose.production.yml")
        (self.backup / "manifest.json").write_text(json.dumps({"format": 2, "database_image": database_image,
                                                               "infrastructure_release": self.previous.name}))
        names = ("database.sql.gz", "media.tar.gz", "account-erasure.log", "metadata.env", "deployment.tar.gz",
                 "manifest.json", "configuration/production.env", "configuration/deployment.env", "configuration/operations.env")
        (self.backup / "SHA256SUMS").write_text("".join(
            f"{hashlib.sha256((self.backup / name).read_bytes()).hexdigest()}  {name}\n" for name in names))

    def test_preupgrade_v2_backup_is_restored_logically_on_the_matching_new_server(self):
        self.versioned_snapshot(OLD_IMAGE)
        result = self.restore()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.database_image.read_text(), NEW_IMAGE)
        self.assertTrue(any("replay-erasure-ledger" in args for args in self.docker_calls()))
        self.assertEqual((self.deployment_root / "current").resolve(), self.previous)

    def test_versioned_backup_cannot_be_imported_backward_or_from_an_unknown_engine(self):
        self.versioned_snapshot("mariadb:14.0.1@sha256:" + "c" * 64)
        self.assert_refused_before_changes(self.restore())

    def test_missing_infrastructure_does_not_fall_back_to_current(self):
        (self.deployment_root / "infrastructure").unlink()
        self.assert_refused_before_changes(self.restore())
        self.assertEqual(self.docker_calls(), [])

    def test_failed_container_inspection_prevents_restore(self):
        self.env["TEST_INSPECT_FAILURE"] = "1"
        self.assert_refused_before_changes(self.restore())

    def test_active_deployment_prevents_restore(self):
        self.env["TEST_LOCK_BUSY"] = "1"
        self.assert_refused_before_changes(self.restore())
        self.assertEqual(self.docker_calls(), [])

    def test_backup_lock_timeout_prevents_restore(self):
        self.env["TEST_BACKUP_LOCK_BUSY"] = "1"
        result = self.restore()
        self.assert_refused_before_changes(result)
        self.assertIn("backup or restore", result.stderr)
        self.assertEqual(self.docker_calls(), [])

    def test_restore_waits_for_active_backup_before_accessing_database(self):
        import fcntl

        self.use_real_locks()
        attempts = Path(self.env["TEST_LOCK_ATTEMPTS"])
        with (self.backup.parent / ".backup.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            process = self.start_restore()
            self.wait_for(lambda: attempts.exists() and "9" in attempts.read_text().splitlines(), process)
            self.assertEqual(attempts.read_text().splitlines(), ["8", "9"])
            self.assertIsNone(process.poll())
            self.assertEqual(self.docker_calls(), [])
        stdout, stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode, 0, stdout + stderr)
        self.assert_locks_available()

    def test_manual_and_scheduled_backups_refuse_to_run_during_restore(self):
        self.use_real_locks()
        ready = self.root / "restore-ready"
        resume = self.root / "restore-resume"
        self.env.update(TEST_RESTORE_READY=str(ready), TEST_RESTORE_RESUME=str(resume))
        process = self.start_restore()
        self.wait_for(ready.exists, process)
        backup_script = Path(__file__).resolve().parents[2] / "deploy/scripts/backup.sh"
        try:
            for kind in ("manual", "daily"):
                with self.subTest(kind=kind):
                    before = len(self.docker_calls())
                    result = subprocess.run(
                        ["bash", str(backup_script), kind], env=self.env,
                        input="", capture_output=True, text=True, timeout=10,
                    )
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("backup or restore", result.stderr)
                    for args in self.docker_calls()[before:]:
                        if args[0] == "compose":
                            self.assertEqual(args[args.index("--file") + 2], "ps", args)
                        else:
                            self.assertEqual(args[0], "inspect", args)
        finally:
            resume.touch()
        stdout, stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode, 0, stdout + stderr)
        self.assert_locks_available()

    def test_failed_erasure_replay_keeps_application_offline(self):
        self.use_real_locks()
        self.env["TEST_REPLAY_FAILURE"] = "1"
        result = self.restore()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.database_image.read_text(), NEW_IMAGE)
        calls = self.docker_calls()
        replay = next(index for index, args in enumerate(calls) if "replay-erasure-ledger" in args)
        self.assertFalse(any("up" in args for args in calls[replay + 1:]))
        self.assert_locks_available()

    def test_backups_refuse_during_candidate_start_without_unlocking_deployment(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        ready = self.root / "deployment-ready"
        resume = self.root / "deployment-resume"
        self.env.update(TEST_DEPLOY_READY=str(ready), TEST_DEPLOY_RESUME=str(resume))
        process = self.start_operation(["bash", str(candidate / "scripts/deploy.sh"), candidate.name])
        self.wait_for(ready.exists, process)
        try:
            for kind in ("manual", "daily", "pre-deploy"):
                with self.subTest(kind=kind):
                    before = len(self.docker_calls())
                    result = subprocess.run(
                        ["bash", str(self.backup_script), kind], env=self.env,
                        input="", capture_output=True, text=True, timeout=10,
                    )
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("deployment, backup or restore", result.stderr)
                    self.assertEqual(self.docker_calls()[before:], [])
        finally:
            resume.touch()
        stdout, stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode, 0, stdout + stderr)
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        snapshots = list(self.backup.parent.glob("*-pre-deploy"))
        self.assertEqual(len(snapshots), 1)
        metadata = (snapshots[0] / "metadata.env").read_text()
        self.assertIn(f"infrastructure_release={self.infrastructure}\n", metadata)
        self.assertEqual(gzip.decompress((snapshots[0] / "database.sql.gz").read_bytes()), b"SELECT 1;\n")
        self.assert_locks_available()

    def test_deployment_waits_for_standalone_backup(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        ready = self.root / "backup-ready"
        resume = self.root / "backup-resume"
        self.env.update(TEST_BACKUP_READY=str(ready), TEST_BACKUP_RESUME=str(resume))
        backup = self.start_operation(["bash", str(self.backup_script), "manual"])
        self.wait_for(ready.exists, backup)
        before = len(self.docker_calls())
        deployment = self.start_operation(["bash", str(candidate / "scripts/deploy.sh"), candidate.name])
        attempts = Path(self.env["TEST_LOCK_ATTEMPTS"])
        try:
            self.wait_for(lambda: len(attempts.read_text().splitlines()) >= 3, deployment)
            self.assertEqual(attempts.read_text().splitlines(), ["8", "9", "8"])
            self.assertIsNone(deployment.poll())
            self.assertEqual(self.docker_calls()[before:], [])
        finally:
            resume.touch()
        backup_stdout, backup_stderr = backup.communicate(timeout=10)
        self.assertEqual(backup.returncode, 0, backup_stdout + backup_stderr)
        stdout, stderr = deployment.communicate(timeout=10)
        self.assertEqual(deployment.returncode, 0, stdout + stderr)
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        self.assert_locks_available()

    def test_unrelated_inherited_descriptor_does_not_bypass_deployment_lock(self):
        import fcntl

        self.use_real_locks()
        wrapper = self.root / "unrelated-descriptor.sh"
        wrapper.write_text('#!/bin/bash\nexec 8>"$LILLY_ROOT/unrelated.lock"\nexec "$LILLY_ROOT/shared/backup.sh" manual\n')
        with (self.shared / ".deploy.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = subprocess.run(["bash", str(wrapper)], env=self.env, input="", capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.docker_calls(), [])

    def test_standalone_backup_respects_legacy_backup_lock(self):
        import fcntl

        self.use_real_locks()
        with (self.backup.parent / ".backup.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = subprocess.run(
                ["bash", str(self.backup_script), "manual"], env=self.env,
                input="", capture_output=True, text=True, timeout=10,
            )
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("backup or restore", result.stderr)
        self.assertEqual(self.docker_calls(), [])
        self.assert_locks_available()

    def test_backup_service_keeps_updated_implementation_after_failed_deployment(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        self.env["TEST_DEPLOY_FAILURE"] = "compose"
        result = self.deploy(candidate)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual((self.deployment_root / "current").resolve(), self.previous)
        self.assertEqual((self.deployment_root / "infrastructure").resolve(), candidate)
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        self.env["TEST_DEPLOY_FAILURE"] = ""
        self.script = candidate / "scripts/restore.sh"
        ready = self.root / "restore-ready"
        resume = self.root / "restore-resume"
        self.env.update(TEST_RESTORE_READY=str(ready), TEST_RESTORE_RESUME=str(resume))
        restore = self.start_restore()
        self.wait_for(ready.exists, restore)
        try:
            before = len(self.docker_calls())
            result = subprocess.run(self.service_backup_arguments(), env=self.env, input="", capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("backup or restore", result.stderr)
            self.assertEqual(self.docker_calls()[before:], [])
            self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        finally:
            resume.touch()
        stdout, stderr = restore.communicate(timeout=10)
        self.assertEqual(restore.returncode, 0, stdout + stderr)
        result = subprocess.run(self.service_backup_arguments(), env=self.env, input="", capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        snapshots = list(self.backup.parent.glob("*-daily"))
        self.assertEqual(len(snapshots), 1)
        self.assertIn(f"infrastructure_release={candidate}\n", (snapshots[0] / "metadata.env").read_text())
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        self.assert_locks_available()

    def test_legacy_installed_backup_unit_blocks_deployment(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        self.env["TEST_SYSTEMD_ROUTE"] = "current"
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Update lilly-backup.service", result.stderr)
        self.assertEqual((self.deployment_root / "infrastructure").resolve(), self.infrastructure)
        self.assertFalse(any("up" in args or "exec" in args or "run" in args for args in self.docker_calls()))
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())

    def test_running_legacy_backup_must_finish_before_deployment(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        self.env["TEST_SYSTEMD_ACTIVE_STATE"] = "activating"
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("backup service is still running", result.stderr)
        self.assertEqual((self.deployment_root / "infrastructure").resolve(), self.infrastructure)
        self.assertFalse(any("up" in args or "exec" in args or "run" in args for args in self.docker_calls()))

    def test_missing_failure_unit_blocks_deployment_before_changing_data(self):
        candidate = self.prepare_deployment()
        self.env["TEST_SYSTEMD_FAILURE_HANDLER"] = ""
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lilly-backup-failure.service", result.stderr)
        self.assertFalse(any("up" in args or "exec" in args or "run" in args for args in self.docker_calls()))

    def test_failed_backup_unit_inspection_blocks_deployment(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        self.env["TEST_SYSTEMD_FAILURE"] = "1"
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Cannot inspect lilly-backup.service", result.stderr)
        self.assertFalse(any("up" in args or "exec" in args or "run" in args for args in self.docker_calls()))

    def test_configured_but_missing_failure_handler_blocks_deployment_before_tool_changes(self):
        candidate = self.prepare_deployment()
        self.env["TEST_SYSTEMD_HANDLER_LOAD_STATE"] = "not-found"
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Install lilly-backup-failure.service", result.stderr)
        self.assertEqual((self.shared / "backup-tools").resolve(), self.infrastructure / "scripts")
        self.assertEqual((self.deployment_root / "infrastructure").resolve(), self.infrastructure)
        self.assertFalse(any("up" in args or "exec" in args or "run" in args for args in self.docker_calls()))

    def test_deployment_without_installed_backup_unit(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        self.env["TEST_SYSTEMD_LOAD_STATE"] = "not-found"
        result = self.deploy(candidate)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_locks_available()

    def test_first_upgrade_uses_updated_backup_code_with_legacy_configuration(self):
        self.use_real_locks()
        (self.deployment_root / "infrastructure").unlink()
        self.database_image.write_text(OLD_IMAGE)
        candidate = self.prepare_deployment()
        result = self.deploy(candidate)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.database_image.read_text(), NEW_IMAGE)
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        snapshots = list(self.backup.parent.glob("*-pre-deploy"))
        self.assertEqual(len(snapshots), 1)
        self.assertIn(f"infrastructure_release={self.previous}\n", (snapshots[0] / "metadata.env").read_text())
        self.assert_locks_available()

    def test_failed_predeployment_backup_prevents_candidate_start(self):
        self.use_real_locks()
        candidate = self.prepare_deployment()
        self.env["TEST_BACKUP_FAILURE"] = "1"
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((self.deployment_root / "current").resolve(), self.previous)
        self.assertEqual((self.deployment_root / "infrastructure").resolve(), self.infrastructure)
        self.assertEqual((self.shared / ".deployment.env").read_text(), self.previous_environment)
        self.assertFalse(any("up" in args for args in self.docker_calls()))
        self.assertEqual(list(self.backup.parent.glob("*-pre-deploy/COMPLETE")), [])
        self.assertFalse(Path(self.env["TEST_LEGACY_BACKUP"]).exists())
        self.assert_locks_available()

    def test_backup_failures_resume_the_backend_and_never_publish_partial_sets(self):
        for variable in ("TEST_BACKUP_FAILURE", "TEST_MEDIA_FAILURE", "TEST_LEDGER_FAILURE", "TEST_UNPAUSE_FAILURE"):
            with self.subTest(variable=variable):
                self.env[variable] = "1"
                before = len(self.docker_calls())
                result = subprocess.run(["bash", str(self.backup_script), "manual"], env=self.env,
                                        input="", capture_output=True, text=True, timeout=15)
                self.env.pop(variable)
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(any("unpause" in args for args in self.docker_calls()[before:]))
                self.assertEqual(list(self.backup.parent.glob("*-manual/COMPLETE")), [])
                self.assertEqual(list(self.backup.parent.glob(".*.incomplete")), [])

    def test_invalid_invocation_identifier_is_rejected_before_filesystem_or_docker_changes(self):
        self.env["INVOCATION_ID"] = "../../outside"
        result = subprocess.run(["bash", str(self.backup_script), "manual"], env=self.env,
                                input="", capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.docker_calls(), [])
        self.assertEqual(list(self.backup.parent.glob(".*.incomplete")), [])

    def test_public_binding_is_rejected_before_deployment_changes(self):
        candidate = self.prepare_deployment()
        with (self.shared / ".deployment.env").open("a") as stream:
            stream.write("LILLY_BIND_ADDRESS=0.0.0.0\n")
        result = self.deploy(candidate)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Traefik", result.stderr)
        self.assertEqual(self.docker_calls(), [])

    def test_custom_backup_location_and_restore_port_are_shared(self):
        custom = self.deployment_root / "snapshots"
        custom.mkdir()
        (self.shared / ".env.operations").write_text(
            f'BACKUP_DIR={json.dumps(str(custom))}\nBACKUP_ALERT_EMAILS=ops@example.test\n')
        (self.shared / ".env.operations").chmod(0o600)
        shutil.move(str(self.backup), str(custom / self.backup.name))
        self.backup = custom / self.backup.name
        with (self.shared / ".deployment.env").open("a") as stream:
            stream.write("LILLY_HOST_PORT=18991\n")
        curl_log = self.root / "curl.log"
        (self.root / "bin/curl").write_text('#!/bin/bash\nprintf "%s\\n" "$@" >"' + str(curl_log) + '"\n')
        result = self.restore()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("http://127.0.0.1:18991/api/v1/health", curl_log.read_text())


if __name__ == "__main__":
    unittest.main()
