"""Exercise deployment binding validation and failures without downgrading the database."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "Deployment uses Linux Bash")
class DeploymentRollbackTests(unittest.TestCase):
    def run_failed_deployment(self, failure_stage):
        with tempfile.TemporaryDirectory(prefix="lilly-deploy-test-") as directory:
            root = Path(directory)
            deployment_root = root / "lilly"
            previous = deployment_root / "releases" / ("a" * 40)
            candidate = deployment_root / "releases" / ("b" * 40)
            shared = deployment_root / "shared"
            commands = root / "bin"
            for path in (previous / "scripts", candidate / "scripts", shared, commands):
                path.mkdir(parents=True)
            for release in (previous, candidate):
                (release / "docker-compose.production.yml").write_text("services: {}\n")
                (release / "Caddyfile").touch()
            (deployment_root / "current").symlink_to(previous, target_is_directory=True)
            (shared / ".env.production").write_text("JWT_SECRET=test-only\n")
            (shared / ".env.operations").write_text("BACKUP_ALERT_EMAILS=ops@example.test\n")
            (shared / ".env.operations").chmod(0o600)
            for source in (Path(__file__).resolve().parents[2] / "deploy/scripts").glob("*.py"):
                shutil.copyfile(source, candidate / "scripts" / source.name)
            previous_environment = "LILLY_IMAGE_TAG=" + "a" * 40 + "\n"
            (shared / ".deployment.env").write_text(previous_environment)

            def executable(path, source):
                path.write_text(source, encoding="utf-8")
                path.chmod(0o755)

            executable(previous / "scripts/backup.sh", "#!/bin/bash\nexit 99\n")
            executable(candidate / "scripts/backup.sh", '#!/bin/bash\n[[ "$LILLY_ROOT/shared/.deploy.lock" -ef "/proc/$$/fd/8" ]] || exit 93\nprintf backup > "$TEST_BACKUP"\n')
            executable(commands / "systemctl", "#!/bin/bash\nprintf 'not-found\\n'\n")
            executable(commands / "flock", "#!/bin/bash\nexit 0\n")
            executable(commands / "sleep", "#!/bin/bash\nexit 0\n")
            executable(commands / "curl", '#!/bin/bash\n[[ "$TEST_FAILURE" != http ]]\n')
            executable(commands / "docker", r'''#!/bin/bash
set -eu
config=""
next_config=0
starts=0
dependencies=1
for arg in "$@"; do
  if (( next_config )); then config="$arg"; next_config=0; fi
  if [[ "$arg" == --file ]]; then next_config=1; fi
  if [[ "$arg" == up ]]; then starts=1; fi
  if [[ "$arg" == --no-deps ]]; then dependencies=0; fi
done
if [[ "$1" == compose ]]; then
  [[ "${LILLY_BIND_ADDRESS:-}" == 127.0.0.1 && "${LILLY_HOST_PORT:-}" == 8091 && "${LILLY_RESOURCE_PREFIX:-}" == lilly ]] || exit 94
  if [[ "$config" == "$TEST_PREVIOUS" ]]; then
    [[ ! -v LILLY_IMAGE_TAG ]] || exit 95
  fi
fi
if [[ "$*" == *"config --format json"* ]]; then
  printf '{"services":{"caddy":{"ports":[{"host_ip":"127.0.0.1","published":"8091","target":80,"protocol":"tcp"}]}}}\n'
fi
if (( starts )) && [[ "$config" == "$TEST_CANDIDATE" ]]; then
  [[ -f "$TEST_BACKUP" ]] || exit 90
  [[ "$(readlink -f -- "$LILLY_ROOT/infrastructure")/docker-compose.production.yml" == "$TEST_CANDIDATE" ]] || exit 92
  printf 13.0 > "$TEST_DATABASE"
  [[ "$TEST_FAILURE" != compose ]] || exit 1
fi
if (( starts )) && [[ "$config" == "$TEST_PREVIOUS" ]]; then
  if (( dependencies )); then printf 12.3 > "$TEST_DATABASE"; fi
  [[ "$*" == *"backend frontend caddy"* ]] || exit 91
  printf previous > "$TEST_APPLICATION"
fi
''')
            env = {
                **os.environ,
                "PATH": str(commands) + os.pathsep + os.environ["PATH"],
                "LILLY_ROOT": str(deployment_root),
                "TEST_BACKUP": str(root / "backup"),
                "TEST_CANDIDATE": str(candidate / "docker-compose.production.yml"),
                "TEST_PREVIOUS": str(previous / "docker-compose.production.yml"),
                "TEST_DATABASE": str(root / "database-version"),
                "TEST_APPLICATION": str(root / "application-version"),
                "TEST_FAILURE": failure_stage,
                "LILLY_BIND_ADDRESS": "0.0.0.0",
                "LILLY_HOST_PORT": "80",
                "LILLY_RESOURCE_PREFIX": "foreign_resources",
                "LILLY_IMAGE_TAG": "caller-mutable",
            }
            script = Path(__file__).resolve().parents[2] / "deploy/scripts/deploy.sh"
            result = subprocess.run(
                ["bash", str(script), "b" * 40], env=env,
                capture_output=True, text=True, timeout=20,
            )
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertEqual((root / "database-version").read_text(), "13.0")
            self.assertEqual((root / "application-version").read_text(), "previous")
            self.assertEqual((shared / ".deployment.env").read_text(), previous_environment)
            self.assertEqual((deployment_root / "current").resolve(), previous)
            self.assertEqual((deployment_root / "infrastructure").resolve(), candidate)

    def test_compose_failure_restores_only_the_application(self):
        self.run_failed_deployment("compose")

    def test_http_health_failure_restores_only_the_application(self):
        self.run_failed_deployment("http")


@unittest.skipUnless(os.name == "posix" and shutil.which("docker"), "Requires Linux and the real Compose parser")
class DeploymentEnvironmentTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="lilly-deploy-env-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.shared = self.root / "shared"
        self.release = self.root / "releases" / ("c" * 40)
        self.shared.mkdir()
        (self.release / "scripts").mkdir(parents=True)
        source = Path(__file__).resolve().parents[2] / "deploy"
        for name in ("docker-compose.production.yml", "Caddyfile"):
            shutil.copyfile(source / name, self.release / name)
        for name in ("deploy.sh", "backup.sh", "operations.py"):
            shutil.copyfile(source / "scripts" / name, self.release / "scripts" / name)
        (self.shared / ".env.production").write_text(
            "MARIADB_ROOT_PASSWORD=unused-root\nMARIADB_PASSWORD=unused\nDATABASE_URL=mysql://unused@db/lilly\n"
            "JWT_SECRET=unused-test-secret\nAPP_BASE_URL=https://example.test\nLILLY_BIND_ADDRESS=0.0.0.0\n")
        self.previous = "LILLY_IMAGE_TAG=" + "a" * 40 + "\nLILLY_BIND_ADDRESS=127.0.0.1\nLILLY_HOST_PORT=18091\nLILLY_RESOURCE_PREFIX=lilly_recorded_resources\n"
        (self.shared / ".deployment.env").write_text(self.previous)
        (self.shared / ".env.operations").write_text("BACKUP_ALERT_EMAILS=ops@example.test\n")
        (self.shared / ".env.operations").chmod(0o600)
        self.report, self.network = self.root / "model.json", self.root / "network-used"
        self.env = {**os.environ, "LILLY_ROOT": str(self.root), "TEST_REAL_DOCKER": shutil.which("docker"),
                    "TEST_MODEL_REPORT": str(self.report), "TEST_NETWORK_USED": str(self.network),
                    "LILLY_BIND_ADDRESS": "0.0.0.0", "LILLY_HOST_PORT": "80",
                    "LILLY_RESOURCE_PREFIX": "foreign_resources", "LILLY_IMAGE_TAG": "caller-mutable"}
        binaries = self.root / "bin"
        binaries.mkdir()
        wrapper = binaries / "docker"
        wrapper.write_text('''#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path
args = sys.argv[1:]
if args[0] == "compose" and args[-3:] == ["config", "--format", "json"]:
    result = subprocess.run([os.environ["TEST_REAL_DOCKER"], *args], capture_output=True)
    if result.returncode:
        sys.stderr.buffer.write(result.stderr)
        sys.exit(result.returncode)
    model = json.loads(result.stdout)
    safe = {"ports": model["services"]["caddy"]["ports"], "image": model["services"]["backend"]["image"],
            "media": model["volumes"]["media_data"]["name"]}
    Path(os.environ["TEST_MODEL_REPORT"]).write_text(json.dumps(safe))
    sys.stdout.buffer.write(result.stdout)
elif args[:2] == ["network", "inspect"]:
    Path(os.environ["TEST_NETWORK_USED"]).touch()
    sys.exit(1)
else:
    raise AssertionError("Unexpected Docker operation")
''')
        wrapper.chmod(0o755)
        self.env["PATH"] = str(binaries) + os.pathsep + self.env["PATH"]

    def deploy(self):
        return subprocess.run(["bash", str(self.release / "scripts/deploy.sh"), self.release.name],
                              env=self.env, capture_output=True, text=True, timeout=20)

    def test_ambient_controls_cannot_change_real_compose_binding_port_prefix_or_image(self):
        result = self.deploy()
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.network.exists(), result.stdout + result.stderr)
        model = json.loads(self.report.read_text())
        self.assertEqual(model["ports"][0]["host_ip"], "127.0.0.1")
        self.assertEqual(str(model["ports"][0]["published"]), "18091")
        self.assertTrue(model["image"].endswith(":" + self.release.name))
        self.assertEqual(model["media"], "lilly_recorded_resources_media_data")
        self.assertEqual((self.shared / ".deployment.env").read_text(), self.previous)

    def test_hardcoded_public_effective_model_fails_before_backups_or_service_changes(self):
        compose = self.release / "docker-compose.production.yml"
        compose.write_text(compose.read_text().replace("${LILLY_BIND_ADDRESS:-127.0.0.1}", "0.0.0.0"))
        result = self.deploy()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Effective Caddy binding", result.stderr)
        self.assertFalse(self.network.exists())
        self.assertFalse((self.shared / "backup.sh").exists())
        self.assertEqual(json.loads(self.report.read_text())["ports"][0]["host_ip"], "0.0.0.0")


if __name__ == "__main__":
    unittest.main()
