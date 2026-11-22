"""Verify independent SMTP setup and atomic, private production configuration."""

import importlib.util
import json
import os
from pathlib import Path
import sys
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "deploy/scripts"))
import operations as ops


@unittest.skipUnless(os.name == "posix", "Production provisioning uses Linux owners")
class ConfigureEnvironmentTests(unittest.TestCase):
    def setUp(self):
        import pwd
        self.owner = pwd.getpwuid(os.getuid()).pw_name
        temporary = tempfile.TemporaryDirectory(prefix="lilly-environment-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / "mail.env"
        self.output = self.root / "production.env"
        spec = importlib.util.spec_from_file_location("configure_environment", Path(__file__).resolve().parents[2] / "deploy/scripts/configure-environment.py")
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.values = {"SMTP_HOST": "smtp.example.test", "SMTP_PORT": "465", "SMTP_TLS_MODE": "tls",
                       "SMTP_USER": "test-user", "SMTP_PASSWORD": "ü $() `literal` \\ quote\"", "SMTP_FROM": "lilly@example.test"}
        self.write_source(self.values)

    def write_source(self, values):
        self.source.write_text("".join(ops.env_line(key, value) for key, value in values.items()), encoding="utf-8")

    def run_main(self, option="--smtp-config"):
        arguments = ["configure-environment.py", option, str(self.source), "--output", str(self.output),
                     "--owner", self.owner, "--app-base-url", "https://lilly.example.test", "--admin-email", "admin@example.test"]
        with patch.object(sys, "argv", arguments), patch("builtins.print") as printed:
            self.module.main()
        return printed

    def test_canonical_smtp_values_and_generated_secrets_round_trip_privately(self):
        printed = self.run_main()
        result = ops.read_env(self.output, compose=True)
        for key, value in self.values.items():
            self.assertEqual(result[key], value)
        self.assertEqual(result["COOKIE_SECURE"], "true")
        self.assertEqual(result["ADMIN_EMAIL"], "admin@example.test")
        self.assertEqual(len(result["MARIADB_PASSWORD"]), 64)
        self.assertNotEqual(result["MARIADB_PASSWORD"], result["MARIADB_ROOT_PASSWORD"])
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o600)
        self.assertNotIn(self.values["SMTP_PASSWORD"], str(printed.call_args_list))

    def test_existing_mail_source_remains_compatible(self):
        self.write_source({"MAIL_HOST": "smtp.example.test", "MAIL_PORT": "465", "MAIL_ENCRYPTION": "ssl",
                           "MAIL_USERNAME": "user", "MAIL_PASSWORD": "password", "MAIL_FROM_ADDRESS": "mail@example.test"})
        self.run_main("--mail-source")
        self.assertEqual(ops.read_env(self.output)["SMTP_TLS_MODE"], "tls")

    @unittest.skipUnless(shutil.which("docker"), "Compose's independent dotenv parser is available in operations CI")
    def test_compose_preserves_literal_smtp_passwords_without_host_interpolation(self):
        password = 'literal $HOME ${UNSET_VARIABLE} $$ \\ quote" ü'
        self.write_source({**self.values, "SMTP_PASSWORD": password})
        self.run_main()
        compose = self.root / "compose.json"
        compose.write_text(json.dumps({"services": {"probe": {"image": "unused",
                            "environment": {"PASSWORD": "${SMTP_PASSWORD}"}}}}))
        result = subprocess.run(["docker", "compose", "--env-file", str(self.output),
                                 "--file", str(compose), "config", "--format", "json"],
                                capture_output=True, check=True)
        # Some Compose versions escape dollars again in reusable model output.
        # Acceptance also compares the actual environment inside the backend.
        self.assertIn(json.loads(result.stdout)["services"]["probe"]["environment"]["PASSWORD"],
                      (password, password.replace("$", "$$")))

    def test_existing_configuration_is_never_overwritten(self):
        self.output.write_text("existing")
        with self.assertRaises(FileExistsError):
            self.run_main()
        self.assertEqual(self.output.read_text(), "existing")

    def test_concurrent_publication_cannot_replace_the_other_configuration(self):
        link = os.link
        def race(source, target):
            target.write_text("concurrent installation")
            link(source, target)
        with patch.object(self.module.os, "link", side_effect=race), self.assertRaises(FileExistsError):
            self.run_main()
        self.assertEqual(self.output.read_text(), "concurrent installation")
        self.assertEqual(sorted(path.name for path in self.root.iterdir()), ["mail.env", "production.env"])

    def test_configuration_write_failure_removes_its_private_temporary_file(self):
        with patch.object(self.module.os, "fsync", side_effect=OSError), self.assertRaises(OSError):
            self.run_main()
        self.assertFalse(self.output.exists())
        self.assertEqual([path.name for path in self.root.iterdir()], ["mail.env"])

    def test_invalid_mail_configuration_fails_before_any_publication(self):
        for key, value in (("SMTP_HOST", ""), ("SMTP_FROM", ""), ("SMTP_PORT", "0"),
                           ("SMTP_TLS_MODE", "none"), ("SMTP_USER", ""), ("SMTP_PORT", "invalid")):
            with self.subTest(key=key, value=value):
                self.write_source({**self.values, key: value})
                with self.assertRaises(ValueError):
                    self.run_main()
                self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
