#!/usr/bin/env python3
"""Small common/core check entry point; native hosts keep their own tools."""
from __future__ import annotations

import argparse
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(*command: str) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def links() -> None:
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "*.md"], cwd=ROOT)
    count = 0
    for name in set(paths.decode().split("\0")) - {""}:
        if name.startswith("fixtures/markdown/"):
            continue
        path = ROOT / name
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", path.read_text(encoding="utf-8")):
            if "://" in target or target.startswith("#"):
                continue
            target = target.split("#")[0]
            if not (path.parent / target).exists():
                raise ValueError(f"{name}: missing local target {target}")
            count += 1
    print(f"Checked {count} local link paths (anchors/network URLs not checked)")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("suite", choices=("common", "core", "all"), default="all", nargs="?")
    args = parser.parse_args()
    if args.suite in ("common", "all"):
        for script in ("validate_fixtures.py", "validate_capabilities.py",
                       "test_path_fixtures.py", "test_validate_capabilities.py", "test_ci_paths.py"):
            run(sys.executable, "tools/" + script)
        links()
        npm = shutil.which("npm")
        if not npm:
            raise SystemExit("Node/npm required: install dev dependencies with npm ci")
        run(npm, "run", "lint:markdown")
        run("git", "diff", "--check")
    if args.suite in ("core", "all"):
        run("cargo", "fmt", "--all", "--", "--check")
        run("cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings")
        run("cargo", "test", "--workspace", "--locked")
        run("cargo", "test", "--workspace", "--locked", "--release")
        run(sys.executable, "tools/check_dependencies.py")
        run(sys.executable, "tools/test_check_dependencies.py")


if __name__ == "__main__":
    main()
