"""Regression checks for dependency-policy failures that must block CI."""
import unittest

from check_dependencies import CRATES_IO, REVIEWED, ROOT, violations


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

    def external(self, source):
        graph = self.package()
        graph["packages"].append(dict(id="external", name="external", version="1.0.0",
            license="MIT", source=source,
            manifest_path=str(ROOT / "build/registry/external/Cargo.toml")))
        return graph

    def test_reviewed_model_dependency_license_alternatives_are_allowed(self):
        for (name, version), expression in REVIEWED.items():
            graph = self.external(CRATES_IO)
            graph["packages"][-1].update(name=name, version=version, license=expression)
            with self.subTest(name=name, version=version):
                self.assertEqual(violations(graph), [])

    def test_reviewed_licenses_do_not_approve_other_packages_versions_or_expressions(self):
        for (name, version), expression in REVIEWED.items():
            for changes in ({"name": "unreviewed"}, {"version": "99.0.0"},
                            {"license": "Unicode-3.0"}):
                graph = self.external(CRATES_IO)
                graph["packages"][-1].update(name=name, version=version, license=expression)
                graph["packages"][-1].update(changes)
                with self.subTest(name=name, changes=changes):
                    self.assertTrue(any("SPDX license" in e for e in violations(graph)))

    def test_crates_io_dependency_is_allowed(self):
        self.assertEqual(violations(self.external(CRATES_IO)), [])

    def test_git_alternate_registry_and_path_sources_need_review(self):
        for source in (None, "git+https://example.invalid/repo#commit",
                       "registry+https://example.invalid/index"):
            with self.subTest(source=source):
                self.assertTrue(any("needs policy review" in e
                                    for e in violations(self.external(source))))

    def test_app_path_dependency_is_rejected_even_with_an_allowed_license(self):
        graph = self.external(None)
        graph["packages"][-1]["manifest_path"] = str(ROOT / "apps/windows/adapter/Cargo.toml")
        self.assertTrue(any("needs policy review" in e for e in violations(graph)))

    def test_workspace_source_and_location_are_restricted(self):
        self.assertTrue(violations(self.package(source=CRATES_IO)))
        self.assertTrue(violations(self.package(manifest_path=str(ROOT / "build/other/Cargo.toml"))))


if __name__ == "__main__":
    unittest.main()
