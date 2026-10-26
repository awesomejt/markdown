"""Regression checks for dependency-policy failures that must block CI."""
import unittest

from check_dependencies import ROOT, violations


class DependencyPolicyTests(unittest.TestCase):
    def package(self, **changes):
        package = dict(id="core", name="markdown-core", version="0.1.0",
                       license="Apache-2.0", publish=[],
                       manifest_path=str(ROOT / "core/crates/markdown-core/Cargo.toml"))
        package.update(changes)
        return {"workspace_members": ["core"], "packages": [package]}

    def test_private_apache_workspace_is_allowed(self):
        self.assertEqual(violations(self.package()), [])

    def test_missing_or_unapproved_license_is_rejected(self):
        for license in (None, "", "UNREVIEWED"):
            with self.subTest(license=license):
                self.assertTrue(violations(self.package(license=license)))

    def test_publishable_crate_is_rejected(self):
        self.assertTrue(violations(self.package(publish=None)))

    def test_spike_cannot_enter_production_graph(self):
        graph = self.package()
        graph["packages"].append(dict(id="spike", name="probe", version="0.0.0",
            license="MIT", manifest_path=str(ROOT / "spikes/probe/Cargo.toml")))
        self.assertTrue(any("feasibility spike" in e for e in violations(graph)))

    def test_transitive_dependencies_are_checked(self):
        graph = self.package()
        graph["packages"].append(dict(id="external", name="external", version="1.0.0",
            license=None, manifest_path=str(ROOT / "build/registry/external/Cargo.toml")))
        self.assertTrue(any("external" in e for e in violations(graph)))


if __name__ == "__main__":
    unittest.main()
