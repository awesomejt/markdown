"""Whitespace checks for committed changes as well as staged/unstaged work."""
import subprocess


def check_whitespace(base: str = "", *, cwd=None) -> None:
    def git(*args, **kwargs):
        return subprocess.run(["git", *args], cwd=cwd, check=True, **kwargs)

    if base and set(base) != {"0"}:
        git("diff", "--check", f"{base}...HEAD", "--")
    elif base:  # Initial push: the base is GitHub's all-zero sentinel.
        empty = subprocess.check_output(
            ["git", "hash-object", "-w", "-t", "tree", "--stdin"], input=b"", cwd=cwd).decode().strip()
        git("diff", "--check", empty, "HEAD", "--")
    else:
        # Without a supplied range, inspect the latest commit even on a clean checkout.
        git("show", "--check", "--format=", "HEAD", "--")
    git("diff", "--check", "HEAD", "--")
