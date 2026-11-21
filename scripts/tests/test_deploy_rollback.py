"""Exercise deployment failures without allowing an upgraded database to be downgraded."""

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
            for path in (previous / "scripts", candidate, shared, commands):
                path.mkdir(parents=True)
            for release in (previous, candidate):
                (release / "docker-compose.production.yml").write_text("services: {}\n")
                (release / "Caddyfile").touch()
            (deployment_root / "current").symlink_to(previous, target_is_directory=True)
            (shared / ".env.production").write_text("JWT_SECRET=test-only\n")
            previous_environment = "LILLY_IMAGE_TAG=" + "a" * 40 + "\n"
            (shared / ".deployment.env").write_text(previous_environment)

            def executable(path, source):
                path.write_text(source, encoding="utf-8")
                path.chmod(0o755)

            executable(previous / "scripts/backup.sh", '#!/bin/bash\nprintf backup > "$TEST_BACKUP"\n')
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
if (( starts )) && [[ "$config" == "$TEST_CANDIDATE" ]]; then
  [[ -f "$TEST_BACKUP" ]] || exit 90
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

    def test_compose_failure_restores_only_the_application(self):
        self.run_failed_deployment("compose")

    def test_http_health_failure_restores_only_the_application(self):
        self.run_failed_deployment("http")


if __name__ == "__main__":
    unittest.main()
