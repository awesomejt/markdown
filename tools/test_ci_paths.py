"""Guard against skipping a host affected by a shared or native change."""
import io
import json
import os
from pathlib import Path
import subprocess
import unittest
from unittest import mock

import yaml

from ci_paths import checks_green, classify, decide


class HostSelectionTests(unittest.TestCase):
    def test_shared_files_select_every_host(self):
        for path in ("core/a.rs", "Cargo.lock", "fixtures/a.md", "tools/check.py", ".github/workflows/ci.yml"):
            self.assertTrue(all(classify([path]).values()), path)

    def test_platform_files_select_their_host(self):
        self.assertEqual(classify(["apps/windows/App.cs"]),
                         dict(linux=False, windows=True, macos=False))
        self.assertEqual(classify(["apps/linux/main.rs"]),
                         dict(linux=True, windows=False, macos=False))
        self.assertEqual(classify(["apps/ios/App.swift"]),
                         dict(linux=False, windows=False, macos=True))
        for path in ("apps/macos/Sources/Markdown/MarkdownApp.swift",
                     "apps/macos/Package.swift", "apps/macos/Info.plist",
                     "apps/macos/tools/check.py"):
            self.assertEqual(classify([path]),
                             dict(linux=False, windows=False, macos=True), path)

    def test_docs_only_and_manual_runs(self):
        self.assertFalse(any(classify(["docs/Product.md"]).values()))
        self.assertTrue(all(classify([], full=True).values()))

    def test_unknown_build_paths_select_all_hosts(self):
        for path in (".cargo/config.toml", "rustfmt.toml", ".rustfmt.toml",
                     "clippy.toml", "deny.toml", "new-crate/src/lib.rs",
                     "apps/shared/config.toml", "apps/new-platform/main.rs"):
            with self.subTest(path=path):
                self.assertTrue(all(classify([path]).values()))

    def test_only_known_non_build_files_skip_hosts(self):
        self.assertFalse(any(classify(["README.md", "docs/Guide.md", "project.yaml",
                                       "agents/Decisions.md"]).values()))
        self.assertTrue(all(classify(["core/README.md"]).values()))
        self.assertEqual(classify(["apps/windows/README.md"]),
                         dict(linux=False, windows=True, macos=False))

    def test_event_full_runs_do_not_attempt_a_diff(self):
        with mock.patch("ci_paths.subprocess.check_output", side_effect=AssertionError("unexpected diff")):
            for env in ({}, {"BASE": ""}, {"BASE": "0" * 40},
                        {"BASE": "abc", "EVENT": "workflow_dispatch"}):
                with self.subTest(env=env):
                    self.assertTrue(all(decide(env).values()))

    def test_previous_green_requires_a_successful_actions_check(self):
        def runs(*items):
            response = mock.MagicMock()
            response.__enter__.return_value = io.BytesIO(json.dumps({"check_runs": list(items)}).encode())
            return response
        env = {"REPO": "o/r", "GH_TOKEN": "t"}
        actions = {"slug": "github-actions"}
        cases = [
            (runs({"conclusion": "success", "app": actions}), True),
            (runs({"conclusion": "failure", "app": actions}), False),
            (runs({"conclusion": "cancelled", "app": actions}), False),
            (runs({"conclusion": "success", "app": {"slug": "other-app"}}), False),
            (runs(), False),
            (OSError("offline"), False),
        ]
        for result, expected in cases:
            with self.subTest(expected=expected), \
                    mock.patch("ci_paths.urllib.request.urlopen", side_effect=[result]):
                self.assertEqual(checks_green("abc", env), expected)
        with mock.patch("ci_paths.urllib.request.urlopen", side_effect=AssertionError("no token")):
            self.assertFalse(checks_green("abc", {}))


class CompletionGateTests(unittest.TestCase):
    def test_native_result_is_required_only_when_selected(self):
        workflow = yaml.safe_load((Path(__file__).resolve().parents[1]
                                   / ".github/workflows/ci.yml").read_text())
        job = workflow["jobs"]["complete"]
        self.assertIn("macos-native", job["needs"])
        # A later successful command must not mask any rejected gate state.
        gate = job["steps"][0]["run"] + "\ntrue\n"
        green = dict(CHANGES="success", COMMON="success", CORE="success",
                     MACOS_SELECTED="true", MACOS_NATIVE="success")
        cases = [({}, True), ({"MACOS_SELECTED": "false", "MACOS_NATIVE": "skipped"}, True),
                 ({"MACOS_SELECTED": "", "MACOS_NATIVE": "skipped"}, False)]
        for result in ("failure", "cancelled", "skipped", ""):
            cases.append(({"MACOS_NATIVE": result}, False))
        for name in ("CHANGES", "COMMON", "CORE"):
            cases.append(({name: "failure"}, False))
        for overrides, passed in cases:
            with self.subTest(overrides=overrides):
                result = subprocess.run(["bash", "-c", gate],
                                        env={**os.environ, **green, **overrides})
                self.assertEqual(result.returncode == 0, passed)


if __name__ == "__main__":
    unittest.main()
