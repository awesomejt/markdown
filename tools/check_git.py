"""Whitespace checks for committed changes as well as staged/unstaged work."""
import os
import re
import subprocess

PROBLEM = re.compile(r"^(.+):(\d+): ")


def new_branch_whitespace(*, cwd=None) -> None:
    """Check a branch's first push for problems on lines no other branch has.

    A line is new only when it differs from every other remote branch, so
    lines a merge, conflict resolution or octopus merge inherited from any of
    them pass. `git diff --check` reports positions in HEAD, so a problem
    counts when the diff from each branch's fork point reports it. The pushed
    branch itself (GITHUB_REF_NAME) is already on the remote. Only `origin`
    counts: another remote could hold the pushed commit under another name
    and hide every problem. With no other branch, or a pushed branch that
    cannot be identified, the whole tree is checked rather than risk
    excluding the branch under test.
    """
    def out(*args):
        return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)

    refs = out("for-each-ref", "--format=%(refname)", "refs/remotes/origin").stdout.split()
    pushed = f"refs/remotes/origin/{os.environ.get('GITHUB_REF_NAME', '')}"
    forks = set()
    if pushed in refs:
        for ref in refs:
            if ref not in (pushed, "refs/remotes/origin/HEAD"):
                forks.add(out("merge-base", "HEAD", ref).stdout.strip())
    forks.discard("")
    if not forks:
        empty = subprocess.check_output(
            ["git", "hash-object", "-w", "-t", "tree", "--stdin"], input=b"", cwd=cwd).decode().strip()
        subprocess.run(["git", "diff", "--check", empty, "HEAD", "--"], cwd=cwd, check=True)
        return
    common = None
    for fork in sorted(forks):
        # Renames keep a moved file's lines old whatever the local Git config.
        result = out("-c", "diff.renames=true", "diff", "--check", fork, "HEAD", "--")
        if result.returncode not in (0, 2):
            raise subprocess.CalledProcessError(result.returncode, result.args, result.stdout, result.stderr)
        # Offending content follows each header as a "+" line; never parse it.
        found = {(m[1], m[2]): line for line in result.stdout.splitlines()
                 if not line.startswith("+") and (m := PROBLEM.match(line))}
        common = found if common is None else {k: v for k, v in common.items() if k in found}
    if common:
        print("\n".join(common.values()))
        raise subprocess.CalledProcessError(2, ["git", "diff", "--check", "(new lines)"])


def check_whitespace(base: str = "", *, cwd=None) -> None:
    def git(*args, **kwargs):
        return subprocess.run(["git", *args], cwd=cwd, check=True, **kwargs)

    if base and set(base) != {"0"}:
        git("diff", "--check", f"{base}...HEAD", "--")
    elif base:  # A branch's first push: GitHub sends the all-zero sentinel.
        new_branch_whitespace(cwd=cwd)
    else:
        # Without a supplied range, inspect the latest commit even on a clean checkout.
        git("show", "--check", "--format=", "HEAD", "--")
    git("diff", "--check", "HEAD", "--")
