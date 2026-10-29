"""Whitespace checks for committed changes as well as staged/unstaged work."""
import os
import subprocess


def check_whitespace(base: str = "", *, cwd=None) -> None:
    def git(*args, **kwargs):
        return subprocess.run(["git", *args], cwd=cwd, check=True, **kwargs)

    if base and set(base) != {"0"}:
        git("diff", "--check", f"{base}...HEAD", "--")
    elif base:  # A branch's first push: GitHub sends the all-zero sentinel.
        # Check only commits no other remote branch has; the pushed branch itself
        # (GITHUB_REF_NAME) is already on the remote. Merges are checked against
        # their first parent, so whatever a merge introduces is covered. With no
        # other branch, or a pushed branch that cannot be identified, check the
        # whole tree rather than risk excluding the branch under test.
        refs = subprocess.check_output(
            ["git", "for-each-ref", "--format=%(refname)", "refs/remotes"], cwd=cwd).decode().split()
        pushed = f"refs/remotes/origin/{os.environ.get('GITHUB_REF_NAME', '')}"
        others = [r for r in refs if r not in (pushed, "refs/remotes/origin/HEAD")]
        if pushed in refs and others:
            new = subprocess.check_output(
                ["git", "rev-list", "HEAD", "--not", *others], cwd=cwd).decode().split()
            for commit in new:
                git("show", "--check", "--format=", "--diff-merges=first-parent", commit, "--")
        else:
            empty = subprocess.check_output(
                ["git", "hash-object", "-w", "-t", "tree", "--stdin"], input=b"", cwd=cwd).decode().strip()
            git("diff", "--check", empty, "HEAD", "--")
    else:
        # Without a supplied range, inspect the latest commit even on a clean checkout.
        git("show", "--check", "--format=", "HEAD", "--")
    git("diff", "--check", "HEAD", "--")
