"""Guard against skipping a host affected by a shared or native change."""
import unittest

from ci_paths import classify


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

    def test_docs_only_and_manual_runs(self):
        self.assertFalse(any(classify(["docs/Product.md"]).values()))
        self.assertTrue(all(classify([], full=True).values()))


if __name__ == "__main__":
    unittest.main()
