"""Protect acceptance isolation, cleanup ownership and credential-free diagnostics."""

import http.client
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import operations_acceptance as acceptance


class ClientTests(unittest.TestCase):
    def test_client_tracks_cookies_and_attaches_same_origin_only_for_the_application(self):
        response = MagicMock(status=200)
        response.read.return_value = b'{"ok":true}'
        response.getheader.return_value = "application/json"
        response.getheaders.return_value = [("Set-Cookie", "access_token=test-only; HttpOnly; Path=/")]
        with patch.object(http.client, "HTTPConnection") as connection:
            connection.return_value.getresponse.return_value = response
            client = acceptance.Client(18080)
            self.assertEqual(client.request("/api", "POST", {"test": True}), {"ok": True})
            client.request("/api")
            headers = connection.return_value.request.call_args.args[3]
            self.assertIn("access_token=test-only", headers["Cookie"])
            self.assertEqual(headers["Origin"], "http://localhost:18080")
            acceptance.Client(18025, app_origin=False).request("/api")
            self.assertNotIn("Origin", connection.return_value.request.call_args.args[3])

    def test_http_errors_do_not_expose_response_bodies_or_token_queries(self):
        response = MagicMock(status=403)
        response.read.return_value = b'{"private":"response-secret"}'
        response.getheaders.return_value = []
        with patch.object(http.client, "HTTPConnection") as connection:
            connection.return_value.getresponse.return_value = response
            with self.assertRaises(RuntimeError) as raised:
                acceptance.Client(18080).request("/api/verify?token=token-secret")
            self.assertNotIn("secret", str(raised.exception))
            connection.return_value.close.assert_called_once()

    def test_binary_content_is_preserved(self):
        response = MagicMock(status=200)
        response.read.return_value = b"image-bytes"
        response.getheader.return_value = "image/jpeg"
        response.getheaders.return_value = []
        with patch.object(http.client, "HTTPConnection") as connection:
            connection.return_value.getresponse.return_value = response
            self.assertEqual(acceptance.Client(18080).request("/photo"), b"image-bytes")


class IsolationTests(unittest.TestCase):
    def test_explicit_host_port_pool_is_consumed_and_validated(self):
        with patch.dict(os.environ, {"LILLY_ACCEPTANCE_PORTS": "18080,18025"}):
            self.assertEqual(acceptance.free_port(), 18080)
            self.assertEqual(acceptance.free_port(), 18025)
            with self.assertRaises(RuntimeError):
                acceptance.free_port()
        with patch.dict(os.environ, {"LILLY_ACCEPTANCE_PORTS": "80"}), self.assertRaises(RuntimeError):
            acceptance.free_port()

    def test_controller_uses_a_dedicated_volume_and_always_removes_it(self):
        for code in (0, 1):
            calls = []
            volume = "lilly-ops-controller-" + "f" * 12
            def execute(arguments, **kwargs):
                calls.append(arguments)
                inspected = [{"Mountpoint": "/var/lib/docker/volumes/test/_data",
                              "Labels": {"lilly.operations.controller": volume}}]
                return SimpleNamespace(stdout=json.dumps(inspected).encode())
            with patch.object(acceptance.uuid, "uuid4", return_value=SimpleNamespace(hex="f" * 32)), \
                 patch.object(acceptance, "command", side_effect=execute), \
                 patch.object(acceptance.subprocess, "run", return_value=SimpleNamespace(returncode=code)) as run:
                self.assertEqual(acceptance.windows_controller(["--backend-image", "candidate"]), code)
                self.assertIn("type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock", run.call_args.args[0])
                self.assertEqual(calls[-1], ["docker", "volume", "rm", volume])

    def test_controller_refuses_to_remove_an_unowned_volume(self):
        response = SimpleNamespace(stdout=json.dumps([{"Mountpoint": "/test", "Labels": {}}]).encode())
        with patch.object(acceptance, "command", return_value=response) as run, \
             patch.object(acceptance.subprocess, "run", return_value=SimpleNamespace(returncode=0)):
            with self.assertRaises(RuntimeError):
                acceptance.windows_controller([])
            self.assertFalse(any(call.args[0][:3] == ["docker", "volume", "rm"] for call in run.call_args_list))

    def test_stack_maps_host_mounts_and_refuses_foreign_volume_cleanup(self):
        with tempfile.TemporaryDirectory() as directory, \
             patch.dict(os.environ, {"LILLY_ACCEPTANCE_STATE": directory, "LILLY_ACCEPTANCE_HOST_ROOT": "/host/state"}):
            stack = acceptance.Stack("backend", "frontend")
            expected = str(Path("/host/state") / stack.root.name / "shared/erasure-ledger")
            self.assertEqual(stack.environment()["LILLY_LEDGER_PATH"], expected)
            self.assertEqual(stack.environment()["LILLY_PROJECT_NAME"], stack.project)
            stack.release.mkdir(parents=True)
            (stack.release / "docker-compose.production.yml").write_text('{"volumes":{"data":{"name":"other_app_data"}}}')
            stack.model_ready = True
            with patch.object(acceptance, "command") as run, self.assertRaises(RuntimeError):
                stack.cleanup()
            run.assert_not_called()
            self.assertTrue(stack.root.exists())

    def test_failed_startup_is_reported_and_cleanup_failure_cannot_produce_success(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.json"
            with patch.object(acceptance, "Stack") as stack, \
                 patch.object(acceptance, "command", return_value=SimpleNamespace(stdout=b"test-image-id")):
                stack.return_value.initialize.side_effect = RuntimeError("synthetic startup failure")
                stack.return_value.cleanup.side_effect = OSError("cleanup failure")
                self.assertEqual(acceptance.acceptance("backend", "frontend", report), 1)
                stack.return_value.cleanup.assert_called_once()
            result = json.loads(report.read_text())
            self.assertEqual(result["status"], "failed")
            self.assertEqual(result["failure"], "RuntimeError")
            self.assertEqual(result["cleanup_failure"], "OSError")
            self.assertEqual(result["checks"], [])


class CommandTests(unittest.TestCase):
    def test_linux_cli_builds_requested_candidates_before_acceptance(self):
        fake_os = SimpleNamespace(name="posix", environ={})
        with patch.object(acceptance, "os", fake_os), \
             patch.object(sys, "argv", ["operations_acceptance.py", "--build"]), \
             patch.object(acceptance, "command") as run, \
             patch.object(acceptance, "acceptance", return_value=0) as verify:
            self.assertEqual(acceptance.main(), 0)
        self.assertEqual(run.call_count, 2)
        verify.assert_called_once()

    def test_windows_cli_forwards_images_report_and_coverage(self):
        fake_os = SimpleNamespace(name="nt", environ={})
        report = acceptance.ROOT / ".tmp/test-report.json"
        with patch.object(acceptance, "os", fake_os), \
             patch.object(sys, "argv", ["operations_acceptance.py", "--coverage", "--build", "--report", str(report)]), \
             patch.object(acceptance, "command") as run, \
             patch.object(acceptance, "windows_controller", return_value=0) as controller:
            self.assertEqual(acceptance.main(), 0)
        self.assertEqual(run.call_count, 2)
        self.assertIn("/workspace/.tmp/test-report.json", controller.call_args.args[0])
        self.assertTrue(controller.call_args.kwargs["coverage"])

    def test_linux_coverage_cli_does_not_recurse(self):
        fake_os = SimpleNamespace(name="posix", environ={})
        with patch.object(acceptance, "os", fake_os), \
             patch.object(sys, "argv", ["operations_acceptance.py", "--coverage"]), \
             patch.object(acceptance.subprocess, "run", return_value=SimpleNamespace(returncode=0)) as run:
            self.assertEqual(acceptance.main(), 0)
        self.assertNotIn("--coverage", run.call_args.args[0])
        self.assertIn("coverage", run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
