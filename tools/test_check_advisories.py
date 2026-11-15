"""Regression tests for dependency advisory scanning and dispositions."""
import unittest

from check_advisories import DISPOSITIONED, evaluate_advisories


class AdvisoryCheckTests(unittest.TestCase):
    def test_clean_packages_pass(self):
        errors, notes = evaluate_advisories(
            [("serde", "1.0.228")],
            [[]],
        )
        self.assertEqual(errors, [])
        self.assertEqual(notes, [])

    def test_unresolved_advisory_fails(self):
        errors, notes = evaluate_advisories(
            [("example", "1.2.3")],
            [[{"id": "RUSTSEC-2099-0001"}]],
        )
        self.assertEqual(notes, [])
        self.assertEqual(len(errors), 1)
        self.assertIn("RUSTSEC-2099-0001", errors[0])
        self.assertIn("example 1.2.3", errors[0])

    def test_dispositioned_advisory_is_scoped_to_exact_package_and_version(self):
        errors, notes = evaluate_advisories(
            [("pulldown-cmark", "0.13.5")],
            [[{"id": "RUSTSEC-2026-0433"}]],
            dispositioned=DISPOSITIONED,
        )
        self.assertEqual(errors, [])
        self.assertEqual(len(notes), 1)
        self.assertIn("RUSTSEC-2026-0433", notes[0])

        # Different version must fail if still flagged.
        errors, _ = evaluate_advisories(
            [("pulldown-cmark", "0.13.4")],
            [[{"id": "RUSTSEC-2026-0433"}]],
            dispositioned=DISPOSITIONED,
        )
        self.assertEqual(len(errors), 1)

        # Different advisory on the same version must fail.
        errors, _ = evaluate_advisories(
            [("pulldown-cmark", "0.13.5")],
            [[{"id": "RUSTSEC-2099-9999"}]],
            dispositioned=DISPOSITIONED,
        )
        self.assertEqual(len(errors), 1)


if __name__ == "__main__":
    unittest.main()
