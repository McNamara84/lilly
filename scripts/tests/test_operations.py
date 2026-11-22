"""Exercise host configuration, snapshot integrity, rotation and SMTP failure handling."""

from datetime import datetime, timedelta, timezone
from email.message import EmailMessage
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import smtplib
import subprocess
import sys
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "deploy/scripts"))
import operations as ops


class OperationsFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="lilly-operations-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve() / "lilly"
        self.shared = self.root / "shared"
        self.shared.mkdir(parents=True)
        self.backups = self.root / "backups"
        self.backups.mkdir()
        self.config = self.shared / ".env.operations"
        self.configure()
        ops.private_write(self.shared / ".env.production", 'SMTP_HOST=smtp.example.test\nSMTP_FROM=lilly@example.test\nSMTP_PASSWORD="test-password-sentinel"\n')
        ops.private_write(self.shared / ".deployment.env", "LILLY_IMAGE_TAG=" + "a" * 40 + "\n")

    def configure(self, **values):
        values = {"BACKUP_ALERT_EMAILS": "ops@example.test", **values}
        ops.private_write(self.config, "".join(f"{key}={json.dumps(value)}\n" for key, value in values.items()))

    def settings(self):
        return ops.Settings.load(self.root)

    def cli(self, *arguments, root=None):
        with patch.object(sys, "argv", ["operations.py", "--root", str(root or self.root), *map(str, arguments)]), \
             patch.object(sys, "stdout", new_callable=io.StringIO) as output:
            result = ops.main()
        return result, output.getvalue()


class ConfigurationTests(OperationsFixture):
    def test_public_entrypoints_offer_help_without_reading_production_configuration(self):
        scripts = Path(__file__).resolve().parents[2] / "deploy/scripts"
        for name in ("operations.py", "configure-operations.py", "notify-backup-failure.py"):
            result = subprocess.run([sys.executable, str(scripts / name), "--help"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("usage:", result.stdout)
            if name != "operations.py":
                spec = importlib.util.spec_from_file_location(name.replace("-", "_"), scripts / name)
                spec.loader.exec_module(importlib.util.module_from_spec(spec))

    def test_settings_rotation_and_timer_cli_have_observable_results(self):
        result, output = self.cli("settings")
        self.assertEqual(result, 0)
        self.assertEqual(output.splitlines(), [str(self.backups), "14"])
        old = self.backups / "20200101T000000Z-daily"
        old.mkdir()
        self.assertEqual(self.cli("rotate")[0], 0)
        self.assertFalse(old.exists())
        with patch.object(ops.subprocess, "run"):
            self.assertEqual(self.cli("configure", "--systemd-directory", self.root / "units", "--apply")[0], 0)
        self.assertTrue((self.root / "units/lilly-backup.timer.d/override.conf").exists())

    def test_compose_project_names_cannot_select_invalid_names(self):
        with self.assertRaises(ops.OperationsError):
            ops.compose_command(self.root, self.root / "compose", "../other-app")

    def test_defaults_and_configurable_values(self):
        settings = self.settings()
        self.assertEqual(settings.backup_dir, self.backups)
        self.assertEqual(settings.retention, 14)
        self.assertEqual(settings.calendar, "*-*-* 03:30:00 Europe/Berlin")
        self.assertEqual(settings.delay, "15m")
        self.assertEqual(settings.timeout, 10)
        self.configure(BACKUP_RETENTION_DAYS="7", BACKUP_ALERT_TIMEOUT_SECONDS="2", BACKUP_RANDOMIZED_DELAY="0")
        self.assertEqual(self.settings().retention, 7)

    def test_rejects_unknown_and_out_of_range_settings(self):
        cases = ({"BACKUP_RETENTION_DAYS": value} for value in ("0", "15", "-1", "NaN", ""))
        cases = [*cases, {"BACKUP_ALERT_TIMEOUT_SECONDS": "61"}, {"BACKUP_ALERT_TIMEOUT_SECONDS": "0"},
                 {"UNEXPECTED": "true"}, {"BACKUP_ON_CALENDAR": ""}, {"BACKUP_ON_CALENDAR": "$(execute)"},
                 {"BACKUP_RANDOMIZED_DELAY": "-1"}, {"BACKUP_ALERT_EMAILS": ""}]
        for values in cases:
            with self.subTest(values=values):
                self.configure(**values)
                with self.assertRaises(ops.OperationsError):
                    self.settings()

    def test_rejects_dangerous_backup_roots(self):
        for value in ("relative/backups", "/", str(self.root), str(self.root.parent),
                      str(self.shared), str(self.shared / "backups"), str(self.root / "releases/new"),
                      "/var/lib/docker/volumes/backups", "/var/lib/mysql/backups"):
            with self.subTest(value=value):
                self.configure(BACKUP_DIR=value)
                with self.assertRaises(ops.OperationsError):
                    self.settings()

    @unittest.skipUnless(os.name == "posix", "Linux file ownership and symlinks")
    def test_rejects_symlinked_destination_and_public_configuration(self):
        external = self.root.parent / "external"
        external.mkdir()
        linked = self.root / "linked"
        linked.symlink_to(external, target_is_directory=True)
        self.configure(BACKUP_DIR=str(linked))
        with self.assertRaises(ops.OperationsError):
            self.settings()
        self.configure()
        self.config.chmod(0o644)
        with self.assertRaises(ops.OperationsError):
            self.settings()

    def test_rejects_a_backup_destination_that_is_a_file(self):
        destination = self.root / "not-a-directory"
        destination.touch()
        self.configure(BACKUP_DIR=str(destination))
        with self.assertRaises(ops.OperationsError):
            self.settings()

    def test_dotenv_round_trips_quotes_unicode_and_literal_shell_text(self):
        expected = {'ONE': '$HOME $(touch forbidden) `whoami` # literal "quote" \\ path', 'TWO': "äé€"}
        path = self.shared / "parser.env"
        path.write_text("# comment\n\n" + "".join(f"{key}={json.dumps(value)}\n" for key, value in expected.items()), encoding="utf-8")
        self.assertEqual(ops.read_env(path), expected)
        path.write_bytes("ONE='Grüße' # comment\nTWO=value # comment\n".encode("cp1252"))
        self.assertEqual(ops.read_env(path), {"ONE": "Grüße", "TWO": "value"})
        path.write_text('ONE="legacy\\$password"\n', encoding="utf-8")
        self.assertEqual(ops.read_env(path), {"ONE": "legacy\\$password"})
        original = "literal $HOME ${UNSET_VARIABLE} $$ password"
        path.write_text(ops.env_line("ONE", original, compose=True))
        self.assertEqual(ops.read_env(path, compose=True), {"ONE": original})

    def test_dotenv_rejects_duplicates_malformed_quotes_and_multiline_values(self):
        for content in ('KEY=a\nKEY=b', 'missing assignment', 'bad-key=x', 'KEY="broken',
                        'KEY="valid" trailing', "KEY='two' 'values'", "KEY='broken", 'KEY=42\nKEY="other"',
                        'KEY="escaped\\nnewline"', 'KEY="nul\\u0000value"', 'KEY=42\0', 'KEY="legacy\\$" "trailing"'):
            with self.subTest(content=content):
                self.config.write_text(content, encoding="utf-8")
                with self.assertRaises(ops.OperationsError):
                    ops.read_env(self.config)

    def test_recipients_are_validated_and_deduplicated(self):
        self.assertEqual(ops.email_addresses("ops@example.test, OPS@example.test, other@example.test"),
                         ("ops@example.test", "other@example.test"))
        for value in ("", "bad", "Display <ops@example.test>", "ops@example.test\nBcc:another@example.test", "ops@example.test,"):
            with self.assertRaises(ops.OperationsError):
                ops.email_addresses(value)

    def test_timer_override_clears_previous_calendar_and_validates_before_writing(self):
        target = self.root / "units"
        with patch.object(ops.subprocess, "run") as run:
            ops.configure(self.settings(), target, True)
        content = (target / "lilly-backup.timer.d/override.conf").read_text()
        self.assertIn("OnCalendar=\nOnCalendar=*-*-* 03:30:00 Europe/Berlin", content)
        self.assertEqual(run.call_args_list[0].args[0], ["systemd-analyze", "calendar", "*-*-* 03:30:00 Europe/Berlin"])
        self.assertIn("daemon-reload", run.call_args_list[1].args[0])
        with patch.object(ops.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "calendar")):
            with self.assertRaises(subprocess.CalledProcessError):
                ops.configure(self.settings(), self.root / "invalid-units", False)
        self.assertFalse((self.root / "invalid-units").exists())


class RotationTests(OperationsFixture):
    def test_retention_uses_utc_names_and_removes_incomplete_sets(self):
        now = datetime(2026, 11, 22, 12, tzinfo=timezone.utc)
        expired = (now - timedelta(days=14)).strftime("%Y%m%dT%H%M%SZ")
        recent = (now - timedelta(days=13)).strftime("%Y%m%dT%H%M%SZ")
        names = [expired + "-daily", "." + expired + "-aabbccdd-manual.incomplete", recent + "-manual",
                 "unrelated", "20269999T000000Z-daily"]
        for name in names:
            (self.backups / name).mkdir()
            (self.backups / name / "data").write_text("snapshot")
        os.utime(self.backups / names[2], (0, 0))
        self.assertCountEqual(ops.rotate(self.settings(), now), names[:2])
        self.assertTrue((self.backups / names[2]).exists())
        self.assertTrue((self.backups / "unrelated").exists())

    @unittest.skipUnless(os.name == "posix", "Linux symlinks")
    def test_rotation_never_follows_symlinks(self):
        outside = self.root.parent / "outside"
        outside.mkdir()
        (outside / "keep").touch()
        (self.backups / "20200101T000000Z-daily").symlink_to(outside, target_is_directory=True)
        owned = self.backups / "20200102T000000Z-daily"
        owned.mkdir()
        (owned / "link").symlink_to(outside, target_is_directory=True)
        ops.rotate(self.settings())
        self.assertTrue((outside / "keep").exists())
        self.assertTrue((self.backups / "20200101T000000Z-daily").is_symlink())
        self.assertFalse(owned.exists())

    def test_rotation_propagates_cleanup_errors(self):
        (self.backups / "20200101T000000Z-daily").mkdir()
        with patch.object(ops.shutil, "rmtree", side_effect=PermissionError):
            with self.assertRaises(PermissionError):
                ops.rotate(self.settings())

    @unittest.skipUnless(os.name == "posix", "Production notification locking uses Linux flock")
    def test_successful_rotation_expires_delivery_receipts_without_another_failure(self):
        now = datetime(2026, 11, 22, 12, tzinfo=timezone.utc)
        directory = self.shared / "backup-alerts"
        directory.mkdir()
        old, recent = directory / "old.json", directory / "recent.json"
        old.write_text('["ops@example.test"]')
        recent.write_text('["ops@example.test"]')
        os.utime(old, ((now - timedelta(days=15)).timestamp(),) * 2)
        os.utime(recent, ((now - timedelta(days=13)).timestamp(),) * 2)
        outside = self.root / "keep-receipt.json"
        outside.write_text("keep")
        (directory / "linked.json").symlink_to(outside)
        ops.rotate(self.settings(), now)
        self.assertFalse(old.exists())
        self.assertTrue(recent.exists())
        self.assertTrue((directory / "linked.json").is_symlink())
        self.assertEqual(outside.read_text(), "keep")


class SnapshotTests(OperationsFixture):
    def snapshot(self, legacy=False, version=3):
        directory = self.backups / "20261122T033000Z-aabbccdd-daily"
        directory.mkdir()
        (directory / "database.sql.gz").write_bytes(gzip.compress(b"SELECT 1;\n"))
        with tarfile.open(directory / "media.tar.gz", "w:gz") as archive:
            item = tarfile.TarInfo("photo.jpg")
            item.size = 5
            archive.addfile(item, io.BytesIO(b"photo"))
        (directory / "account-erasure.log").write_text("live-ledger\n")
        (directory / "metadata.env").write_text("kind=daily\n")
        if legacy:
            names = ("database.sql.gz", "media.tar.gz", "account-erasure.log")
            (directory / "SHA256SUMS").write_text("".join(f"{ops.file_hash(directory / name)}  {name}\n" for name in names))
        else:
            infrastructure = self.root / "releases" / ("b" * 40)
            (infrastructure / "scripts").mkdir(parents=True)
            (infrastructure / "docker-compose.production.yml").write_text("services: {}")
            (infrastructure / "Caddyfile").touch()
            (infrastructure / "scripts/backup.sh").write_text("#!/bin/bash\n")
            with patch.object(ops.subprocess, "run", return_value=MagicMock(stdout="mariadb:13.0.2\n")):
                ops.finish_snapshot(self.settings(), directory, infrastructure / "docker-compose.production.yml")
            if version == 2:
                manifest = json.loads((directory / "manifest.json").read_text())
                manifest["format"] = 2
                manifest.pop("recovery_tools_sha256")
                (directory / "manifest.json").write_text(json.dumps(manifest))
                (directory / "recovery-tools.tar.gz").unlink()
                checksums = directory / "SHA256SUMS"
                checksums.write_text("\n".join(line for line in checksums.read_text().splitlines()
                                              if not line.endswith("  recovery-tools.tar.gz")) + "\n")
                self.rewrite_hash(directory, "manifest.json")
        (directory / "COMPLETE").touch()
        return directory

    def rewrite_hash(self, directory, name):
        path = directory / "SHA256SUMS"
        lines = path.read_text().splitlines()
        path.write_text("\n".join(f"{ops.file_hash(directory / name)}  {name}" if line.endswith("  " + name) else line for line in lines) + "\n")

    def test_snapshot_contains_recovery_configuration_and_verifies(self):
        directory = self.snapshot()
        manifest = ops.verify_snapshot(directory)
        self.assertEqual(manifest["format"], 3)
        self.assertEqual(manifest["database_image"], "mariadb:13.0.2")
        self.assertEqual((directory / "configuration/production.env").read_bytes(), (self.shared / ".env.production").read_bytes())
        with tarfile.open(directory / "deployment.tar.gz") as archive:
            self.assertIn("docker-compose.production.yml", archive.getnames())
            self.assertNotIn("scripts/backup.sh", archive.getnames())
        with tarfile.open(directory / "recovery-tools.tar.gz") as archive:
            self.assertCountEqual(archive.getnames(), ops.RECOVERY_SCRIPTS)
            self.assertIn(b"backend ./lilly-backend privacy replay-erasure-ledger",
                          archive.extractfile("restore.sh").read())
        if os.name == "posix":
            self.assertEqual((directory / "configuration/production.env").stat().st_mode & 0o777, 0o600)

    def test_legacy_snapshot_is_readable(self):
        self.assertEqual(ops.verify_snapshot(self.snapshot(legacy=True)), {})

    def test_incomplete_or_changed_snapshot_is_rejected(self):
        directory = self.snapshot()
        (directory / "COMPLETE").unlink()
        with self.assertRaises(ops.OperationsError):
            ops.verify_snapshot(directory)
        (directory / "COMPLETE").touch()
        (directory / "media.tar.gz").write_bytes(b"corrupt")
        with self.assertRaisesRegex(ops.OperationsError, "mismatch"):
            ops.verify_snapshot(directory)

    def test_missing_checksum_duplicate_and_path_traversal_are_rejected(self):
        directory = self.snapshot()
        checksums = directory / "SHA256SUMS"
        original = checksums.read_text()
        variants = ("", original + original.splitlines()[0] + "\n", "a" * 64 + "  ../outside\n",
                    "not-a-hash  database.sql.gz\n", original.replace(next(line + "\n" for line in original.splitlines() if line.endswith("manifest.json")), ""))
        for content in variants:
            checksums.write_text(content)
            with self.assertRaises(ops.OperationsError):
                ops.verify_snapshot(directory)

    def test_invalid_gzip_is_detected_even_with_matching_checksum(self):
        directory = self.snapshot()
        (directory / "database.sql.gz").write_bytes(b"not gzip")
        self.rewrite_hash(directory, "database.sql.gz")
        with self.assertRaises(OSError):
            ops.verify_snapshot(directory)

    def test_archive_rejects_paths_links_and_devices_before_extraction(self):
        for member in (tarfile.TarInfo("../escape"), tarfile.TarInfo("/absolute"), tarfile.TarInfo("link"), tarfile.TarInfo("device")):
            if member.name == "link":
                member.type = tarfile.SYMTYPE
                member.linkname = "../outside"
            if member.name == "device":
                member.type = tarfile.CHRTYPE
            archive_path = self.backups / "invalid.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                archive.addfile(member)
            with self.assertRaises(ops.OperationsError):
                ops.validate_archive(archive_path)

    def test_invalid_manifest_is_rejected(self):
        directory = self.snapshot()
        path = directory / "manifest.json"
        valid = json.loads(path.read_text())
        for value in ([], {}, {**valid, "format": 4}, {**valid, "infrastructure_release": None},
                      {**valid, "infrastructure_release": "../outside"},
                      {**valid, "database_image": None}, {**valid, "database_image": "image\nextra"}):
            with self.subTest(value=value):
                path.write_text(json.dumps(value))
                self.rewrite_hash(directory, "manifest.json")
                with self.assertRaises(ops.OperationsError):
                    ops.verify_snapshot(directory)

    def test_finish_rejects_unowned_or_malformed_directory(self):
        with self.assertRaises(ops.OperationsError):
            ops.finish_snapshot(self.settings(), self.root / "unknown", self.root / "compose")

    @unittest.skipUnless(os.name == "posix", "Recovery uses Linux symlinks")
    def test_fresh_installation_recovers_configuration_but_keeps_the_newer_ledger(self):
        directory = self.snapshot()
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        target = self.root.parent / "recovered"
        ops.recover_configuration(directory, target, latest)
        self.assertEqual((target / "shared/erasure-ledger/account-erasure.log").read_bytes(), latest.read_bytes())
        self.assertEqual(ops.read_env(target / "shared/.env.operations")["BACKUP_DIR"], str(target / "backups"))
        self.assertEqual((target / "infrastructure").resolve(), target / "releases" / ("b" * 40))
        self.assertTrue((target / "shared/backup.sh").is_file())
        self.assertTrue((target / "shared/backup-tools/operations.py").is_file())
        self.assertTrue((target / "shared/backup-tools/notify-backup-failure.py").is_file())
        self.assertFalse((target / "infrastructure/scripts/restore.sh").exists())
        self.assertEqual((target / "shared/backup.sh").stat().st_mode & 0o777, 0o700)
        with self.assertRaises(ops.OperationsError):
            ops.recover_configuration(directory, target, latest)

    def test_recovery_rejects_legacy_and_historical_or_invalid_ledgers(self):
        directory = self.snapshot()
        with self.assertRaises(ops.OperationsError):
            ops.recover_configuration(directory, self.root / "new", directory / "account-erasure.log")
        invalid = self.root / "invalid-ledger"
        invalid.write_text("invalid entry\n")
        with self.assertRaises(ops.OperationsError):
            ops.recover_configuration(directory, self.root / "new", invalid)

    def test_fresh_recovery_requires_versioned_configuration_and_preserves_existing_files(self):
        directory = self.snapshot(legacy=True)
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        with self.assertRaisesRegex(ops.OperationsError, "version 2"):
            ops.recover_configuration(directory, self.root.parent / "legacy-target", latest)

    def test_recovery_cannot_overwrite_private_configuration_without_an_infrastructure_link(self):
        directory = self.snapshot()
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        target = self.root.parent / "existing-target"
        ops.private_write(target / "shared/.env.production", "existing configuration")
        with self.assertRaises(ops.OperationsError):
            ops.recover_configuration(directory, target, latest)
        self.assertEqual((target / "shared/.env.production").read_text(), "existing configuration")

    def test_cli_verifies_snapshots_and_sanitizes_failures(self):
        directory = self.snapshot()
        with patch.object(sys, "argv", ["operations.py", "verify", str(directory)]):
            self.assertEqual(ops.main(), 0)
        with patch.object(sys, "argv", ["operations.py", "verify", str(directory / "missing")]), \
             patch.object(sys, "stderr", new_callable=io.StringIO) as output:
            self.assertEqual(ops.main(), 1)
            self.assertNotIn(str(self.root), output.getvalue())
        result, output = self.cli("verify", directory, "--database-image")
        self.assertEqual((result, output.strip()), (0, "mariadb:13.0.2"))

    def test_finish_cli_publishes_recovery_checksums_and_rejects_ambiguous_images(self):
        source = self.snapshot()
        compose = self.root / "releases" / ("b" * 40) / "docker-compose.production.yml"
        for index, image in enumerate(("mariadb:13.0.2", "", "first\nsecond")):
            directory = self.backups / f"20261122T03300{index}Z-manual"
            directory.mkdir()
            for name in ("database.sql.gz", "media.tar.gz", "account-erasure.log", "metadata.env"):
                shutil.copyfile(source / name, directory / name)
            with patch.object(ops.subprocess, "run", return_value=SimpleNamespace(stdout=image)):
                self.assertEqual(self.cli("finish", directory, compose)[0], 0 if index == 0 else 1)
            self.assertEqual((directory / "SHA256SUMS").exists(), index == 0)

    @unittest.skipUnless(os.name == "posix", "Fresh-host recovery uses Linux ownership")
    def test_recovery_cli_restores_files_for_the_configured_owner(self):
        import pwd
        directory = self.snapshot()
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        target = self.root.parent / "owned-recovery"
        account = pwd.getpwnam("nobody") if os.getuid() == 0 else pwd.getpwuid(os.getuid())
        result, _ = self.cli("recover-files", directory, "--live-ledger", latest,
                             "--owner", account.pw_name, root=target)
        self.assertEqual(result, 0)
        for name in (".env.production", "backup.sh", "backup-tools", "backup-tools/operations.py",
                     "backup-tools/notify-backup-failure.py", "recovery-tools"):
            self.assertEqual((target / "shared" / name).lstat().st_uid, account.pw_uid)
        self.assertEqual((target / "infrastructure").stat().st_mode & 0o777, 0o750)

    @unittest.skipUnless(os.name == "posix", "Recovery guard checks Linux symlinks")
    def test_recovery_refuses_existing_releases_and_nonempty_or_linked_ledgers(self):
        directory = self.snapshot()
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        for scenario in ("release", "ledger", "shared-link"):
            target = self.root.parent / ("recovery-" + scenario)
            if scenario == "release":
                (target / "releases" / ("b" * 40)).mkdir(parents=True)
            elif scenario == "ledger":
                ops.private_write(target / "shared/erasure-ledger/account-erasure.log", "e" * 64)
            else:
                target.mkdir()
                (target / "shared").symlink_to(self.shared, target_is_directory=True)
            with self.subTest(scenario=scenario), self.assertRaises(ops.OperationsError):
                ops.recover_configuration(directory, target, latest)

    def test_recovery_extraction_failure_never_publishes_private_configuration(self):
        directory = self.snapshot()
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        target = self.root.parent / "failed-recovery"
        with patch.object(ops.tarfile.TarFile, "extractall", side_effect=OSError), self.assertRaises(OSError):
            ops.recover_configuration(directory, target, latest)
        self.assertFalse((target / "shared/.env.production").exists())
        self.assertEqual(list((target / "releases").iterdir()), [])


    def test_recovery_does_not_overwrite_existing_shared_entrypoints(self):
        directory = self.snapshot()
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        for index, name in enumerate(("backup.sh", "backup-tools", "recovery-tools")):
            for linked in (False, True):
                target = self.root.parent / f"existing-tools-{index}-{linked}"
                existing = target / "shared" / name
                existing.parent.mkdir(parents=True)
                if linked:
                    existing.symlink_to(target / "missing")
                else:
                    existing.write_text("existing tool")
                with self.subTest(name=name, linked=linked), self.assertRaises(ops.OperationsError):
                    ops.recover_configuration(directory, target, latest)
                self.assertFalse((target / "shared/.env.production").exists())
                self.assertTrue(existing.is_symlink() if linked else existing.read_text() == "existing tool")

    def test_v2_recovery_uses_current_tools_instead_of_archived_legacy_scripts(self):
        directory = self.snapshot(version=2)
        with tarfile.open(directory / "deployment.tar.gz", "w:gz") as archive:
            legacy = tarfile.TarInfo("scripts/restore.sh")
            legacy.size = len(b"broken legacy restore")
            archive.addfile(legacy, io.BytesIO(b"broken legacy restore"))
        self.rewrite_hash(directory, "deployment.tar.gz")
        latest = self.root / "latest-ledger"
        latest.write_text("f" * 64 + "\n")
        target = self.root.parent / "v2-recovery"
        ops.recover_configuration(directory, target, latest)
        self.assertEqual((target / "infrastructure/scripts/restore.sh").read_text(), "broken legacy restore")
        self.assertIn("backend ./lilly-backend privacy replay-erasure-ledger",
                      (target / "shared/backup-tools/restore.sh").read_text())

    def test_recovery_tool_archive_requires_all_files_and_matches_its_manifest(self):
        directory = self.snapshot()
        manifest_path = directory / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest_path.write_text(json.dumps({**manifest, "recovery_tools_sha256": "0" * 64}))
        self.rewrite_hash(directory, "manifest.json")
        with self.assertRaisesRegex(ops.OperationsError, "manifest"):
            ops.verify_snapshot(directory)
        with tarfile.open(directory / "recovery-tools.tar.gz", "w:gz"):
            pass
        manifest["recovery_tools_sha256"] = ops.file_hash(directory / "recovery-tools.tar.gz")
        manifest_path.write_text(json.dumps(manifest))
        self.rewrite_hash(directory, "manifest.json")
        self.rewrite_hash(directory, "recovery-tools.tar.gz")
        with self.assertRaisesRegex(ops.OperationsError, "complete"):
            ops.verify_snapshot(directory)

    def test_snapshot_requires_the_complete_current_tool_bundle(self):
        with patch.object(ops, "current_recovery_scripts", return_value=self.root / "missing"):
            with self.assertRaisesRegex(ops.OperationsError, "incomplete"):
                self.snapshot()


class DatabaseRestoreTests(OperationsFixture):
    def test_only_tested_forward_logical_restores_are_accepted(self):
        def image(version, digest="a"):
            return "mariadb:" + version + "@sha256:" + digest * 64
        for source, target in ((image("12.3.3"), image("13.0.2")),
                               (image("13.0.1"), image("13.0.2")),
                               (image("13.0.2"), image("13.0.2", "b")),
                               ("custom:immutable", "custom:immutable"), ("", image("13.0.2"))):
            with self.subTest(source=source, target=target):
                ops.check_database_restore(source, target)
        for source, target in ((image("13.0.2"), image("12.3.3")),
                               (image("13.0.2"), image("13.0.1")),
                               (image("12.2.1"), image("13.0.2")),
                               (image("13.0.2"), image("14.0.1")),
                               ("mysql:9.0", image("13.0.2")),
                               ("custom:mariadb-12.3.3", image("13.0.2")),
                               ("mariadb:12.3.3", image("13.0.2"))):
            with self.subTest(source=source, target=target), self.assertRaises(ops.OperationsError):
                ops.check_database_restore(source, target)
        self.assertEqual(self.cli("check-database-restore", "--source-image", image("12.3.3"),
                                  "--target-image", image("13.0.2"))[0], 0)


@unittest.skipUnless(os.name == "posix", "Production alert locking uses Linux flock")
class NotificationTests(OperationsFixture):
    def test_notifier_rejects_invalid_timeout_before_connecting(self):
        self.configure(BACKUP_ALERT_TIMEOUT_SECONDS="0")
        with self.assertRaises(ops.OperationsError), patch.object(ops, "send_alert") as send:
            ops.notify_failure(self.root, "run", "daily", "service")
        send.assert_not_called()

    def test_partial_delivery_retries_only_failed_recipients_and_deduplicates_invocations(self):
        self.configure(BACKUP_ALERT_EMAILS="ops@example.test, other@example.test, OPS@example.test")
        with patch.object(ops, "send_alert", side_effect=[None, OSError("secret detail"), OSError("secret detail")]) as send:
            self.assertFalse(ops.notify_failure(self.root, "job-1", "daily", "database"))
            self.assertEqual(send.call_count, 3)
            self.assertNotIn("test-password-sentinel", send.call_args.args[2].as_string())
        with patch.object(ops, "send_alert") as send:
            self.assertTrue(ops.notify_failure(self.root, "job-1", "daily", "database"))
            self.assertEqual(send.call_args.args[1], "other@example.test")
            self.assertTrue(ops.notify_failure(self.root, "job-1", "service", "service"))
            self.assertEqual(send.call_count, 1)

    def test_configuration_error_can_be_reported_without_valid_retention_or_database(self):
        self.configure(BACKUP_RETENTION_DAYS="invalid")
        with patch.object(ops, "send_alert") as send:
            self.assertTrue(ops.notify_failure(self.root, "run-1", "daily", "configuration"))
        self.assertIn("configuration", send.call_args.args[2].get_content())

    def test_alerts_sanitize_categories_and_reject_bad_identifiers(self):
        with self.assertRaises(ops.OperationsError):
            ops.notify_failure(self.root, "../outside", "daily", "database")
        with patch.object(ops, "send_alert") as send:
            ops.notify_failure(self.root, "run-2", "sensitive-value", "password=sensitive-value")
        self.assertNotIn("sensitive-value", send.call_args.args[2].as_string())

    def test_old_delivery_states_expire(self):
        status = self.shared / "backup-alerts/old.json"
        ops.private_write(status, "[]")
        os.utime(status, (0, 0))
        with patch.object(ops, "send_alert"):
            ops.notify_failure(self.root, "new", "manual", "snapshot")
        self.assertFalse(status.exists())

    def test_failure_unit_reuses_the_systemd_invocation_identifier(self):
        with patch.dict(os.environ, {"MONITOR_INVOCATION_ID": "f" * 32}), \
             patch.object(sys, "argv", ["notify-backup-failure.py", "--root", str(self.root)]), \
             patch.object(ops, "notify_failure", return_value=False) as notify:
            self.assertEqual(ops.main("notify"), 1)
            self.assertEqual(notify.call_args.args[1], "f" * 32)


class SmtpTests(unittest.TestCase):
    def setUp(self):
        self.mail = {"SMTP_HOST": "smtp.example.test", "SMTP_FROM": "lilly@example.test",
                     "SMTP_USER": "test-user", "SMTP_PASSWORD": "test-password"}
        self.message = EmailMessage()
        self.message["Subject"] = "Test"
        self.message.set_content("test")

    def test_starttls_and_implicit_tls_verify_certificates_and_bound_io(self):
        for mode in ("starttls", "tls"):
            with self.subTest(mode=mode), patch.object(ops.smtplib, "SMTP") as smtp, patch.object(ops.smtplib, "SMTP_SSL") as tls:
                client = (tls if mode == "tls" else smtp).return_value
                client.send_message.return_value = {}
                ops.send_alert({**self.mail, "SMTP_TLS_MODE": mode}, "ops@example.test", self.message, 3)
                factory = tls if mode == "tls" else smtp
                self.assertEqual(factory.call_args.kwargs["timeout"], 3)
                client.login.assert_called_once_with("test-user", "test-password")
                client.close.assert_called_once()
                if mode == "starttls":
                    self.assertTrue(client.starttls.call_args.kwargs["context"].check_hostname)
                else:
                    self.assertTrue(tls.call_args.kwargs["context"].check_hostname)

    def test_cleartext_requires_explicit_isolated_test_mode(self):
        with patch.dict(os.environ, {}, clear=True), self.assertRaises(ops.OperationsError):
            ops.send_alert({**self.mail, "SMTP_TLS_MODE": "none"}, "ops@example.test", self.message, 1)
        with patch.dict(os.environ, {"LILLY_ISOLATED_TEST": "1"}), patch.object(ops.smtplib, "SMTP") as smtp:
            smtp.return_value.send_message.return_value = {}
            ops.send_alert({**self.mail, "SMTP_TLS_MODE": "none"}, "ops@example.test", self.message, 1)
            smtp.return_value.starttls.assert_not_called()

    def test_relay_without_login_and_connect_failures_are_supported(self):
        with patch.object(ops.smtplib, "SMTP") as smtp:
            smtp.return_value.send_message.return_value = {}
            mail = {**self.mail, "SMTP_USER": "", "SMTP_PASSWORD": ""}
            ops.send_alert(mail, "ops@example.test", self.message, 1)
            smtp.return_value.login.assert_not_called()
        with patch.object(ops.smtplib, "SMTP", side_effect=TimeoutError), self.assertRaises(TimeoutError):
            ops.send_alert(self.mail, "ops@example.test", self.message, 1)

    def test_invalid_smtp_configuration_and_recipient_refusal_fail(self):
        for values in ({"SMTP_HOST": ""}, {"SMTP_TLS_MODE": "invalid"}):
            with self.assertRaises(ops.OperationsError):
                ops.send_alert({**self.mail, **values}, "ops@example.test", self.message, 1)
        with patch.object(ops.smtplib, "SMTP") as smtp:
            smtp.return_value.send_message.return_value = {"ops@example.test": (550, b"refused")}
            with self.assertRaises(ops.OperationsError):
                ops.send_alert(self.mail, "ops@example.test", self.message, 1)
            smtp.return_value.close.assert_called_once()
        with patch.object(ops.smtplib, "SMTP"), self.assertRaises(ops.OperationsError):
            ops.send_alert({**self.mail, "SMTP_USER": ""}, "ops@example.test", self.message, 1)


if __name__ == "__main__":
    unittest.main()
