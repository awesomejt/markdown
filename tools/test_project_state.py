"""Native-host coordination regression tests, using disposable state only."""

from __future__ import annotations

import json
import errno
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

import project_state as wrapper
from patch_project_state_backend import patch_source


ROOT = Path(__file__).resolve().parents[1]
WRAPPER = Path(wrapper.__file__).resolve()
BACKEND = wrapper.backend_path().resolve()


class StateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.state = self.directory / "project.yaml"
        self.backend = wrapper.load_backend()

    def run_cli(self, *args: str, script: Path = WRAPPER, expected: int = 0) -> dict:
        result = subprocess.run([sys.executable, str(script), "--file", str(self.state), *args],
                                capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(result.returncode, expected, result.stderr + result.stdout)
        return json.loads(result.stdout)

    def initialize(self, script: Path = WRAPPER) -> None:
        self.run_cli("init", "--project-ref", "test", "--workflow-id", "test",
                     "--phases", "test", script=script)

    def request(self, name: str, revision: int, data: dict) -> Path:
        path = self.directory / f"{name}.json"
        path.write_text(json.dumps({
            "contract_version": "project-state/v1", "project_ref": "test",
            "actor": "test", "request_id": name, "expected_revision": revision,
            "input": data,
        }), encoding="utf-8")
        return path

    def test_platform_detection_and_inspect_does_not_mutate(self) -> None:
        info = json.loads(subprocess.check_output([sys.executable, str(WRAPPER), "--platform-info"]))
        self.assertEqual(info["os_name"], os.name)
        self.assertEqual(info["platform"], sys.platform)
        self.assertEqual(info["directory_fsync"], os.name == "posix")
        self.assertEqual(info["locking"], "msvcrt-exclusive-byte-range" if os.name == "nt"
                         else "fcntl-flock")
        self.initialize()
        before = self.state.read_bytes()
        self.run_cli("inspect")
        self.assertEqual(self.state.read_bytes(), before)
        self.assertTrue(self.state.with_name("project.yaml.lock").exists())

    def test_json_and_yaml_roundtrip(self) -> None:
        for script in (BACKEND, WRAPPER):
            with self.subTest(script=script):
                self.state = self.directory / (script.stem + str(script == WRAPPER) + ".yaml")
                self.initialize(script)
                self.assertEqual(self.run_cli("inspect", script=script)["revision"], 1)
                self.assertEqual(yaml.safe_load(self.state.read_text())["revision"], 1)
                if script == WRAPPER:
                    self.assertTrue(self.state.read_text().startswith("contract_version:"))

    def test_concurrent_writers_and_idempotent_retry(self) -> None:
        self.initialize()
        requests = [self.request(f"writer-{i}", 1, {
            "task_key": f"T-{i}", "title": "Concurrent test", "phase": "test",
        }) for i in range(2)]
        workers = [subprocess.Popen([sys.executable, str(WRAPPER), "--file", str(self.state),
                    "apply", "create_task", "--request-file", str(p)],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE) for p in requests]
        results = []
        try:
            for worker in workers:
                stdout, stderr = worker.communicate(timeout=20)
                self.assertFalse(stderr, stderr)
                results.append((worker.returncode, json.loads(stdout)))
        finally:
            for worker in workers:
                if worker.poll() is None:
                    worker.kill()
                    worker.wait()
        self.assertEqual(sorted(code for code, _ in results), [0, 2])
        winner = next(i for i, (code, _) in enumerate(results) if code == 0)
        failure = results[1 - winner][1]
        self.assertEqual(failure["error"]["code"], "state_conflict")
        before = self.state.read_bytes()
        replay = self.run_cli("apply", "create_task", "--request-file", str(requests[winner]))
        self.assertEqual(replay, results[winner][1])
        self.assertEqual(self.state.read_bytes(), before)
        view = self.run_cli("inspect")
        self.assertEqual(view["revision"], 2)
        self.assertEqual(view["queue_counts"]["total"], 1)

    def test_start_finish_lifecycle(self) -> None:
        self.initialize()
        request = self.request("create", 1, {"task_key": "T-1", "title": "Test", "phase": "test"})
        self.run_cli("apply", "create_task", "--request-file", str(request))
        request = self.request("start", 2, {"task_ref": "T-1", "objective": "Test lifecycle"})
        record = self.run_cli("apply", "start_pass", "--request-file", str(request))["data"]["pass"]
        request = self.request("finish", 3, {"pass_ref": record["id"], "outcome": "completed",
                                            "summary": "Passed disposable lifecycle"})
        self.run_cli("apply", "finish_pass", "--request-file", str(request))
        view = self.run_cli("inspect")
        self.assertEqual(view["revision"], 4)
        for key in ("active_passes", "stale_passes", "working_tasks_without_active_lease", "warnings"):
            self.assertFalse(view[key])

    def test_remove_dependency_and_update_task(self) -> None:
        self.run_cli("init", "--project-ref", "test", "--workflow-id", "test",
                     "--phases", "first,second")
        for revision, key in ((1, "T-1"), (2, "T-2")):
            request = self.request(f"create-{key}", revision,
                                   {"task_key": key, "title": "Test", "phase": "first"})
            self.run_cli("apply", "create_task", "--request-file", str(request))
        edge = {"predecessor_task_ref": "T-1", "successor_task_ref": "T-2"}
        self.run_cli("apply", "add_dependency", "--request-file", str(self.request("add", 3, edge)))
        self.assertEqual([t["task_key"] for t in self.run_cli("inspect")["eligible_tasks"]], ["T-1"])
        self.run_cli("apply", "remove_dependency",
                     "--request-file", str(self.request("remove", 4, edge)))
        view = self.run_cli("inspect")
        self.assertEqual(sorted(t["task_key"] for t in view["eligible_tasks"]), ["T-1", "T-2"])
        missing =self.run_cli("apply", "remove_dependency", "--request-file",
                               str(self.request("remove-again", 5, edge)), expected=2)
        self.assertEqual(missing["error"]["code"], "not_found")

        update = {"task_ref": "T-2", "phase": "second", "description": "Moved", "priority": "70"}
        self.run_cli("apply", "update_task", "--request-file", str(self.request("update", 5, update)))
        task = next(t for t in yaml.safe_load(self.state.read_text())["tasks"]
                    if t["task_key"] == "T-2")
        self.assertEqual((task["phase"], task["description"], task["priority"]),
                         ("second", "Moved", 70))
        self.assertEqual([t["task_key"] for t in self.run_cli("inspect")["eligible_tasks"]], ["T-1"])
        for name, data in (("bad-field", {"task_ref": "T-2", "status": "completed"}),
                           ("bad-phase", {"task_ref": "T-2", "phase": "nowhere"}),
                           ("empty", {"task_ref": "T-2"})):
            with self.subTest(name=name):
                error = self.run_cli("apply", "update_task", "--request-file",
                                     str(self.request(name, 6, data)), expected=2)["error"]
                self.assertIn(error["code"], {"validation_error", "not_found"})

        start = self.request("start", 6, {"task_ref": "T-1", "objective": "Hold a pass"})
        self.run_cli("apply", "start_pass", "--request-file", str(start))
        error = self.run_cli("apply", "update_task", "--request-file", str(self.request(
            "during-pass", 7, {"task_ref": "T-1", "description": "Edited"})), expected=2)["error"]
        self.assertEqual(error["message"], "Task has an active pass")
        self.assertEqual(self.run_cli("inspect")["revision"], 7)

    def test_failures_before_replace_preserve_original_and_clean_temporary(self) -> None:
        self.state.write_bytes(b"original")
        for operation in ("replace", "fsync"):
            with self.subTest(operation=operation):
                with mock.patch.object(self.backend.os, operation, side_effect=OSError("injected")):
                    with self.assertRaises(OSError):
                        self.backend.atomic_write_text(self.state, "replacement")
                self.assertEqual(self.state.read_bytes(), b"original")
                self.assertEqual(list(self.directory.glob("*.tmp")), [])

    def test_lock_blocks_other_process_and_releases_after_exception(self) -> None:
        code = '''import importlib.util,sys
from pathlib import Path
s=importlib.util.spec_from_file_location("backend",sys.argv[1]); m=importlib.util.module_from_spec(s); s.loader.exec_module(m)
print("ready",flush=True)
with m.state_lock(Path(sys.argv[2]),exclusive=False):
    print("acquired",flush=True)
'''
        worker = None
        try:
            with self.backend.state_lock(self.state, exclusive=True):
                worker = subprocess.Popen([sys.executable, "-c", code, str(BACKEND), str(self.state)],
                                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                self.assertEqual(worker.stdout.readline().strip(), "ready")
                with self.assertRaises(subprocess.TimeoutExpired):
                    worker.wait(timeout=0.3)
                raise ValueError("force lock release")
        except ValueError:
            pass
        finally:
            if worker is not None:
                try:
                    stdout, stderr = worker.communicate(timeout=10)
                    self.assertEqual(worker.returncode, 0, stderr)
                    self.assertEqual(stdout.strip(), "acquired")
                finally:
                    if worker.poll() is None:
                        worker.kill()
                        worker.wait()

    def test_patch_refuses_unrecognized_backend(self) -> None:
        with self.assertRaisesRegex(ValueError, "Unrecognized backend"):
            patch_source("# unrelated local implementation\n")

    @unittest.skipUnless(os.name == "nt", "Windows lock timeout")
    def test_windows_lock_timeout_is_structured(self) -> None:
        code = '''import importlib.util,sys
from pathlib import Path
s=importlib.util.spec_from_file_location("backend",sys.argv[1]); m=importlib.util.module_from_spec(s); s.loader.exec_module(m)
m.LOCK_TIMEOUT_SECONDS=0.15
try:
    with m.state_lock(Path(sys.argv[2]),exclusive=False):
        raise RuntimeError("lock unexpectedly acquired")
except m.StateError as exc:
    assert exc.code=="lease_conflict" and exc.retryable
    print("bounded contention")
'''
        with self.backend.state_lock(self.state, exclusive=True):
            result = subprocess.run([sys.executable, "-c", code, str(BACKEND), str(self.state)],
                                    capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "bounded contention")
        with mock.patch.object(self.backend.msvcrt, "locking",
                               side_effect=OSError(errno.EBADF, "bad descriptor")) as locking:
            with self.assertRaises(OSError):
                with self.backend.state_lock(self.state, exclusive=True):
                    self.fail("unexpected acquisition")
            self.assertEqual(locking.call_count, 1)

    @unittest.skipUnless(os.name == "posix", "POSIX permissions and directory sync")
    def test_posix_preserves_permissions_and_syncs_file_and_directory(self) -> None:
        self.state.write_text("original", encoding="utf-8")
        self.state.chmod(0o640)
        with mock.patch.object(self.backend.os, "fsync", wraps=os.fsync) as sync:
            self.backend.atomic_write_text(self.state, "updated")
        self.assertEqual(sync.call_count, 2)
        self.assertEqual(self.state.stat().st_mode & 0o777, 0o640)
        self.assertEqual(self.state.read_text(), "updated")


if __name__ == "__main__":
    unittest.main()
