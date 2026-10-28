"""Exercise real Git changes and common inline-link edge cases from PR review."""
from pathlib import Path
import subprocess
import tempfile
import unittest

from check_git import check_whitespace
from check_links import check_paths, local_targets
from ci_paths import decide


class GitChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Check fixture")
        self.git("config", "user.email", "check@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "diff.renames", "true")
        self.write("core/example.rs", "clean\n")
        self.commit()
        self.base = self.git("rev-parse", "HEAD")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root,
                                       stderr=subprocess.PIPE).decode().strip()

    def write(self, name, value):
        p = self.root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(value)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "test")

    def test_rename_across_host_boundary_checks_both_hosts(self):
        (self.root / "apps/windows").mkdir(parents=True)
        self.git("mv", "core/example.rs", "apps/windows/example.rs")
        self.commit()
        result = decide({"BASE": self.base, "HEAD": "HEAD", "EVENT": "pull_request"}, cwd=self.root)
        self.assertTrue(all(result.values()))

    def test_rename_from_native_to_docs_does_not_hide_native_removal(self):
        self.write("apps/windows/example.cs", "same content\n")
        self.commit()
        base = self.git("rev-parse", "HEAD")
        (self.root / "docs").mkdir()
        self.git("mv", "apps/windows/example.cs", "docs/example.cs")
        self.commit()
        result = decide({"BASE": base, "HEAD": "HEAD", "EVENT": "push"}, cwd=self.root)
        self.assertTrue(result["windows"])

    def pr_push(self, prev, green=True):
        env = {"BASE": self.base, "HEAD": "HEAD", "EVENT": "pull_request",
               "PREV": prev, "PR_HEAD": self.git("rev-parse", "HEAD")}
        return decide(env, cwd=self.root, green=lambda sha: green and sha == prev)

    def test_state_only_push_after_green_head_skips_hosts(self):
        self.write("core/example.rs", "changed\n")
        self.commit()
        prev = self.git("rev-parse", "HEAD")
        self.write("project.yaml", "state\n")
        self.write("HANDOFF.md", "handoff\n")
        self.commit()
        self.assertFalse(any(self.pr_push(prev).values()))
        # The PR as a whole still changed core: a red or unknown previous run checks it.
        self.assertTrue(all(self.pr_push(prev, green=False).values()))

    def test_build_change_since_previous_head_checks_every_host(self):
        prev = self.git("rev-parse", "HEAD")
        self.write("core/example.rs", "changed\n")
        self.commit()
        self.assertTrue(all(self.pr_push(prev).values()))

    def test_force_push_or_missing_previous_head_uses_the_full_rule(self):
        self.write("core/example.rs", "first\n")
        self.commit()
        prev = self.git("rev-parse", "HEAD")
        self.git("reset", "--quiet", "--hard", self.base)
        self.write("core/example.rs", "rewritten\n")
        self.commit()
        self.assertTrue(all(self.pr_push(prev).values()))
        self.assertTrue(all(self.pr_push("f" * 40).values()))
        self.assertTrue(all(self.pr_push("0" * 40).values()))

    def test_source_fixture_whitespace_exception_does_not_cover_code(self):
        attributes = (Path(__file__).resolve().parents[1] / ".gitattributes").read_text()
        self.write(".gitattributes", attributes)
        fixture = self.root / "specs/semantic-model/v1/fixtures/example.source.txt"
        fixture.parent.mkdir(parents=True)
        fixture.write_bytes(b"line  \r\n")
        self.write("core/bad.rs", "bad " + "\n")
        self.git("add", ".")
        with self.assertRaises(subprocess.CalledProcessError):
            check_whitespace(self.base, cwd=self.root)
        self.write("core/bad.rs", "clean\n")
        self.git("add", ".")
        check_whitespace(self.base, cwd=self.root)

    def test_clean_range_and_clean_worktree_pass(self):
        self.write("clean.txt", "clean\n")
        self.commit()
        check_whitespace(self.base, cwd=self.root)

    def test_staged_and_unstaged_whitespace_are_checked(self):
        self.write("core/example.rs", "bad " + "\n")
        for staged in (False, True):
            if staged:
                self.git("add", ".")
            with self.subTest(staged=staged), self.assertRaises(subprocess.CalledProcessError):
                check_whitespace(self.base, cwd=self.root)

    def test_committed_whitespace_is_checked_in_clean_checkout(self):
        self.write("bad.txt", "bad " + "\n")
        self.commit()
        self.write("later.txt", "clean\n")
        self.commit()
        self.assertEqual(self.git("status", "--porcelain"), "")
        with self.assertRaises(subprocess.CalledProcessError):
            check_whitespace(self.base, cwd=self.root)

    def test_zero_base_and_no_base_check_committed_whitespace(self):
        self.write("bad.txt", "bad " + "\n")
        self.commit()
        for base in ("0" * 40, ""):
            with self.subTest(base=base), self.assertRaises(subprocess.CalledProcessError):
                check_whitespace(base, cwd=self.root)


class LinkChecks(unittest.TestCase):
    def test_uri_schemes_and_fragments_are_not_file_paths(self):
        self.assertEqual(list(local_targets(
            "[mail](mailto:a@example.invalid) [uri](custom:thing) [heading](#name) [net](//example.invalid/x)")), [])

    def test_titles_angles_encoding_and_parentheses(self):
        self.assertEqual(list(local_targets(
            '[x](file.md "Title") [y](<space file.md>) [z](space%20file.md#heading) '
            "[q](file.md 'Title') [p](a(b).md)")),
            ["file.md", "space file.md", "space file.md", "file.md", "a(b).md"])

    def test_code_spans_and_fences_are_ignored(self):
        self.assertEqual(list(local_targets(
            "`[x](ignored.md)`\n```md\n[x](ignored.md)\n```\n"
            "~~~~\n[x](ignored.md)\n~~~\n~~~~\n[x](real.md)")), ["real.md"])

    def test_deleted_documents_skipped_and_missing_targets_aggregated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "doc.md").write_text("[a](missing-a.md) [b](missing-b.md)")
            with self.assertRaises(ValueError) as error:
                check_paths(root, ["deleted.md", "doc.md"])
            self.assertIn("missing-a.md", str(error.exception))
            self.assertIn("missing-b.md", str(error.exception))

    def test_existing_encoded_file_with_title_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "space file.md").write_text("# Example\n")
            (root / "doc.md").write_text('[a](space%20file.md "Title")')
            self.assertEqual(check_paths(root, ["doc.md"]), 1)


if __name__ == "__main__":
    unittest.main()
