#!/usr/bin/env python3
"""Small common/core check entry point; native hosts keep their own tools."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

from check_git import check_whitespace
from check_links import check_paths

ROOT = Path(__file__).resolve().parents[1]


def run(*command: str) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def links() -> None:
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "*.md"], cwd=ROOT)
    count = check_paths(ROOT, [p for p in paths.decode().split("\0") if p])
    print(f"Checked {count} local link paths (anchors/network URLs not checked)")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("suite", choices=("common", "core", "core-extended", "all"), default="all",
                        nargs="?")
    args = parser.parse_args()
    if args.suite in ("common", "all"):
        for script in ("validate_fixtures.py", "validate_capabilities.py", "validate_model.py",
                       "test_path_fixtures.py", "test_validate_capabilities.py", "test_ci_paths.py",
                       "test_check_tools.py"):
            run(sys.executable, "tools/" + script)
        run(sys.executable, "tools/generate_conformance.py", "--check")
        links()
        npm = shutil.which("npm")
        if not npm:
            raise SystemExit("Node/npm required: install dev dependencies with npm ci")
        run(npm, "run", "lint:markdown")
        check_whitespace(os.environ.get("CHECK_BASE", ""), cwd=ROOT)
    if args.suite in ("core", "core-extended", "all"):
        # The markdown-ffi C package smoke compiles against core/include with the
        # host C compiler. It is mandatory where `cc` is standard (macOS/Linux).
        # Windows always skips it until WIN-001B wires MSVC and the import library.
        if sys.platform != "win32":
            os.environ.setdefault("MD_REQUIRE_C_SMOKE", "1")
    if args.suite in ("core", "all"):
        # PR tier (TOOL-002): lint plus the v1-default and full-feature debug builds.
        run("cargo", "fmt", "--all", "--", "--check")
        run("cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings")
        run("cargo", "test", "--workspace", "--locked")
        run("cargo", "test", "--workspace", "--all-features", "--locked")
        run(sys.executable, "tools/check_dependencies.py")
        run(sys.executable, "tools/test_check_dependencies.py")
    if args.suite in ("core-extended", "all"):
        # Main/manual CI tier and every local `all` run: the model-v2-only feature
        # set and the release build, which enables the release-only time guards.
        run("cargo", "test", "--workspace", "--features", "model-v2", "--locked")
        run("cargo", "test", "--workspace", "--all-features", "--locked", "--release")


if __name__ == "__main__":
    main()
