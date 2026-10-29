"""Whitespace checks for committed changes as well as staged/unstaged work."""
import os
import subprocess


def check_merge(commit: str, git, *, cwd=None) -> None:
    """Check only what a two-parent merge adds beyond Git's automatic merge."""
    parents = subprocess.check_output(
        ["git", "rev-parse", f"{commit}^@"], cwd=cwd).decode().split()
    if len(parents) != 2:  # merge-tree takes two parents; check octopus merges fully
        git("show", "--check", "--format=", "--diff-merges=first-parent", commit, "--")
        return
    merged = subprocess.run(
        ["git", "merge-tree", "--write-tree", "--name-only", "--no-messages", *parents],
        cwd=cwd, capture_output=True, text=True)
    if merged.returncode not in (0, 1):
        raise subprocess.CalledProcessError(merged.returncode, merged.args, merged.stdout, merged.stderr)
    tree, *conflicted = merged.stdout.split("\n")
    git("diff", "--check", tree, commit, "--")
    conflicted = [path for path in conflicted if path]
    if conflicted:
        # Markers committed exactly as Git wrote them match the automatic tree,
        # so conflicted paths are also checked against the first parent.
        git("diff", "--check", parents[0], commit, "--", *conflicted)


def check_whitespace(base: str = "", *, cwd=None) -> None:
    def git(*args, **kwargs):
        return subprocess.run(["git", *args], cwd=cwd, check=True, **kwargs)

    if base and set(base) != {"0"}:
        git("diff", "--check", f"{base}...HEAD", "--")
    elif base:  # A branch's first push: GitHub sends the all-zero sentinel.
        # Check only commits no other remote branch has; the pushed branch itself
        # (GITHUB_REF_NAME) is already on the remote. A merge is checked for what
        # it adds beyond Git's automatic merge, so inherited lines pass. With no
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
                if len(subprocess.check_output(
                        ["git", "rev-parse", f"{commit}^@"], cwd=cwd).split()) > 1:
                    check_merge(commit, git, cwd=cwd)
                else:
                    git("show", "--check", "--format=", commit, "--")
        else:
            empty = subprocess.check_output(
                ["git", "hash-object", "-w", "-t", "tree", "--stdin"], input=b"", cwd=cwd).decode().strip()
            git("diff", "--check", empty, "HEAD", "--")
    else:
        # Without a supplied range, inspect the latest commit even on a clean checkout.
        git("show", "--check", "--format=", "HEAD", "--")
    git("diff", "--check", "HEAD", "--")
