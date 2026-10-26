"""Mutation checks for the capability matrix validator against the real document."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from validate_capabilities import (
    DEFAULT_DOCUMENT,
    DEFAULT_PROJECT,
    ROOT,
    load_task_keys,
    validate,
)


TEXT = DEFAULT_DOCUMENT.read_text(encoding="utf-8")
TASK_KEYS = load_task_keys(DEFAULT_PROJECT)
CAP01_OVERVIEW = (
    "| CAP-01 | Open a single file through picker or association | R1.1 "
    "| Supported | Supported | Supported | Supported |"
)
CAP01_WINDOWS_START = "| Windows | Supported | Measured | Native WinUI processes opened"
CAP01_WINDOWS_SOURCES = "| M01, A01, A04 |"


def errors_for(text: str) -> list[str]:
    return validate(text, DEFAULT_DOCUMENT, TASK_KEYS)


def mutate(old: str, new: str) -> list[str]:
    assert TEXT.count(old) == 1, f"mutation anchor is not unique: {old!r}"
    return errors_for(TEXT.replace(old, new))


class CapabilityMatrixTests(unittest.TestCase):
    def assertRejected(self, errors: list[str], fragment: str) -> None:
        self.assertTrue(
            any(fragment in error for error in errors),
            f"expected an error containing {fragment!r}, got {errors}",
        )

    def test_checked_in_document_is_valid(self) -> None:
        self.assertEqual(errors_for(TEXT), [])

    def test_task_keys_come_from_project_state(self) -> None:
        self.assertIn("GATE-001", TASK_KEYS)
        self.assertIn("IOS-001", TASK_KEYS)

    def test_overview_status_must_match_details(self) -> None:
        errors = mutate(CAP01_OVERVIEW, CAP01_OVERVIEW.replace("| Supported |", "| Deferred |", 1))
        self.assertRejected(errors, "CAP-01: overview statuses differ")

    def test_invalid_status_and_evidence(self) -> None:
        self.assertRejected(
            mutate(CAP01_WINDOWS_START, CAP01_WINDOWS_START.replace("Supported", "Partial")),
            "invalid status 'Partial'",
        )
        self.assertRejected(
            mutate(CAP01_WINDOWS_START, CAP01_WINDOWS_START.replace("Measured", "Tested")),
            "invalid evidence 'Tested'",
        )

    def test_supported_cannot_rest_on_unverified(self) -> None:
        errors = mutate(
            CAP01_WINDOWS_START, CAP01_WINDOWS_START.replace("Measured", "Unverified")
        )
        self.assertRejected(errors, "only Deferred may rest on Unverified")

    def test_evidence_needs_matching_source_kind(self) -> None:
        errors = mutate(CAP01_WINDOWS_SOURCES, "| A01, A04 |")
        self.assertRejected(errors, "Measured evidence needs a M source")

    def test_unknown_source_is_rejected(self) -> None:
        errors = mutate(CAP01_WINDOWS_SOURCES, "| M01, A01, A99 |")
        self.assertRejected(errors, "unknown source 'A99'")

    def test_missing_platform_row(self) -> None:
        row = next(line for line in TEXT.splitlines() if line.startswith(CAP01_WINDOWS_START))
        errors = mutate(row + "\n", "")
        self.assertRejected(errors, "CAP-01: rows must be exactly")

    def test_missing_detail_section(self) -> None:
        errors = mutate(
            "### CAP-30 — Store or sandboxed package", "### Store or sandboxed package"
        )
        self.assertRejected(errors, "CAP-30: missing detail section")

    def test_duplicate_overview_id(self) -> None:
        errors = mutate("| CAP-02 | Open and browse", "| CAP-01 | Open and browse")
        self.assertRejected(errors, "expected CAP-02, found CAP-01")

    def test_next_check_owner_must_be_a_project_task(self) -> None:
        errors = mutate(
            "| WIN-001: picker, association", "| WIN-009: picker, association"
        )
        self.assertRejected(errors, "owner WIN-009 is not a project task")
        errors = mutate("| WIN-001: picker, association", "| picker, association")
        self.assertRejected(errors, "must start with an owner task key")

    def test_placeholder_cell(self) -> None:
        errors = mutate("| Command-line and drag-open paths |", "| TBD |")
        self.assertRejected(errors, "Fallback is empty or a placeholder")

    def test_measured_source_path_must_exist(self) -> None:
        errors = mutate(
            "(Windows-Rendering-Evaluation.md)", "(Windows-Rendering-Missing.md)"
        )
        self.assertRejected(errors, "M01 path does not exist")

    def test_authoritative_source_must_be_https(self) -> None:
        errors = mutate("(https://katex.org/docs/options)", "(http://katex.org/docs/options)")
        self.assertRejected(errors, "A75 must link an HTTPS reference")

    def test_unverified_source_has_no_link(self) -> None:
        errors = mutate(
            "| U07 | Unverified | No WinUI PDF writer",
            "| U07 | Unverified | [gap](https://example.com) No WinUI PDF writer",
        )
        self.assertRejected(errors, "U07 must describe a gap without a link")

    def test_source_kind_must_match_prefix(self) -> None:
        errors = mutate("| M07 | Measured |", "| M07 | Authoritative |")
        self.assertRejected(errors, "M07 kind must be Measured")

    def test_orphan_source_is_rejected(self) -> None:
        errors = mutate(
            "| U09 | Unverified |",
            "| U10 | Unverified | Unused gap | Nothing |\n| U09 | Unverified |",
        )
        self.assertRejected(errors, "U10 is never cited")

    def test_missing_required_section(self) -> None:
        errors = mutate("## iOS/iPadOS synthesis", "## Mobile notes")
        self.assertRejected(errors, "missing section: ## iOS/iPadOS synthesis")

    def test_cli_exit_codes(self) -> None:
        script = ROOT / "tools" / "validate_capabilities.py"
        passed = subprocess.run(
            [sys.executable, str(script), str(DEFAULT_DOCUMENT)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(passed.returncode, 0, passed.stderr)
        with tempfile.TemporaryDirectory() as temporary:
            broken = Path(temporary) / "docs" / DEFAULT_DOCUMENT.name
            broken.parent.mkdir()
            broken.write_text(TEXT.replace(CAP01_WINDOWS_SOURCES, "| A01, A04 |"), encoding="utf-8")
            failed = subprocess.run(
                [sys.executable, str(script), str(broken)],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(failed.returncode, 1)
        self.assertIn("error:", failed.stderr)


if __name__ == "__main__":
    unittest.main()
