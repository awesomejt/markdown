"""Regression checks for generated paths, isolation, and source integrity."""

from __future__ import annotations

import hashlib
import shutil
import tempfile
import unittest
from pathlib import Path

from generate_path_fixtures import ROOT, materialize_paths, safe_component


class PathFixtureTests(unittest.TestCase):
    def test_names_bytes_and_separate_directories(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            records = materialize_paths(ROOT / "fixtures", Path(temporary))
            self.assertEqual(len(records), 3)
            for record in records:
                self.assertEqual(
                    hashlib.sha256(Path(record["path"]).read_bytes()).hexdigest(),
                    record["sha256"],
                )
            forms = {r["normalization"]: r for r in records if r["case"] == "nfd-filename"}
            self.assertEqual(forms["NFC"]["requested_name"], "caf\u00e9.md")
            self.assertEqual(forms["NFD"]["requested_name"], "cafe\u0301.md")
            self.assertNotEqual(Path(forms["NFC"]["path"]).parent,
                                Path(forms["NFD"]["path"]).parent)

    def test_existing_output_is_not_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            records = materialize_paths(ROOT / "fixtures", output)
            first = Path(records[0]["path"])
            first.write_bytes(b"user changes")
            with self.assertRaises(FileExistsError):
                materialize_paths(ROOT / "fixtures", output)
            self.assertEqual(first.read_bytes(), b"user changes")

    def test_corrupted_source_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            fixtures = base / "fixtures"
            shutil.copytree(ROOT / "fixtures", fixtures)
            (fixtures / "markdown/paths/unicode-filename.md").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "source hash mismatch"):
                materialize_paths(fixtures, base / "output")

    def test_unsafe_components_are_rejected(self) -> None:
        for name in ("", ".", "..", "../escape", "a/b", "a\\b", "C:escape",
                     "a\x00b", "name.", "name ", "CON.md", "nul", "LPT1.txt"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                safe_component(name)


if __name__ == "__main__":
    unittest.main()
