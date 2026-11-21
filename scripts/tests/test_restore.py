"""Check recovery after an application rollback against the live database image."""

import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
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
        for directory in (self.previous, self.infrastructure / "scripts", self.shared / "erasure-ledger", self.backup, commands):
            directory.mkdir(parents=True)
        for release, image in ((self.previous, OLD_IMAGE), (self.infrastructure, NEW_IMAGE)):
            (release / "docker-compose.production.yml").write_text(f"services:\n  db:\n    image: {image}\n")
        (self.deployment_root / "current").symlink_to(self.previous, target_is_directory=True)
        (self.deployment_root / "infrastructure").symlink_to(self.infrastructure, target_is_directory=True)
        (self.shared / ".env.production").write_text("JWT_SECRET=test-only\n")
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
            "TEST_INSPECT_FAILURE": "0",
            "TEST_REPLAY_FAILURE": "0",
        }

        def executable(name, source):
            target = commands / name
            target.write_text(source, encoding="utf-8")
            target.chmod(0o755)

        executable("flock", '#!/bin/bash\n[[ "$TEST_LOCK_BUSY" != 1 ]]\n')
        executable("curl", "#!/bin/bash\nexit 0\n")
        executable("docker", r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import re
import sys

args = sys.argv[1:]
with open(os.environ["TEST_DOCKER_LOG"], "a") as log:
    log.write(json.dumps(args) + "\n")
database = Path(os.environ["TEST_DATABASE_IMAGE"])
if args[0] == "inspect":
    if os.environ["TEST_INSPECT_FAILURE"] == "1":
        sys.exit(1)
    assert args[2] == "{{.Config.Image}}", args
    print(database.read_text())
elif args[0] == "compose":
    config_index = args.index("--file")
    config = Path(args[config_index + 1])
    command = args[config_index + 2:]
    image = re.search(r"image:\s*(\S+)", config.read_text()).group(1)
    if command[0] == "config":
        assert command[1:] == ["--images", "db"], command
        print(image)
    elif command[0] == "ps":
        if os.environ["TEST_DATABASE_STATE"] == "running":
            print("test-db")
    elif command[0] == "up":
        database.write_text(image)
    elif command[0] == "exec":
        sys.stdin.buffer.read()
    elif command[0] == "run":
        if os.environ["TEST_REPLAY_FAILURE"] == "1":
            sys.exit(1)
    else:
        assert command[0] == "stop", command
elif args[0] == "run":
    sys.stdin.buffer.read()
else:
    raise AssertionError(args)
''')
        script = Path(__file__).resolve().parents[2] / "deploy/scripts/restore.sh"
        self.script = self.infrastructure / "scripts/restore.sh"
        self.script.write_text(script.read_text(), encoding="utf-8")

    def restore(self):
        return subprocess.run(
            ["bash", str(self.script), "--backup", str(self.backup), "--confirm", "RESTORE_LILLY"],
            env=self.env, input="", capture_output=True, text=True, timeout=20,
        )

    def docker_calls(self):
        if not self.command_log.exists():
            return []
        return [json.loads(line) for line in self.command_log.read_text().splitlines()]

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
        self.assertTrue(any("replay-erasure-ledger" in args for args in calls))
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

    def test_failed_erasure_replay_keeps_application_offline(self):
        self.env["TEST_REPLAY_FAILURE"] = "1"
        result = self.restore()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.database_image.read_text(), NEW_IMAGE)
        calls = self.docker_calls()
        replay = next(index for index, args in enumerate(calls) if "replay-erasure-ledger" in args)
        self.assertFalse(any("up" in args for args in calls[replay + 1:]))


if __name__ == "__main__":
    unittest.main()
