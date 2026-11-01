#!/usr/bin/env python3
"""Stage the core packages, lint, license-check, then build, run the core consumer tests
and the real GTK shell smoke in debug and release."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from package_core import prepare_core, verify

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]
TARGET_DIR = REPO_ROOT / "build" / "linux" / "target"
SMOKES = {"--smoke-test": "shell-smoke: pass (application",
          "--smoke-test=last-window": "shell-smoke: pass (closing the last window"}


def run(*command: str, cwd: Path = APP_ROOT) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def cargo(command: str, *arguments: str) -> None:
    run("cargo", command, "--locked", "--target-dir", str(TARGET_DIR), *arguments)


def build(configuration: str) -> Path:
    profile = ["--release"] if configuration == "release" else []
    cargo("build", "--workspace", *profile)
    # Release-mode and debug-mode consumers of the matching core package.
    cargo("test", "--workspace", *profile)
    return TARGET_DIR / configuration / "markdown-linux"


def smoke(binary: Path, flag: str, marker: str) -> None:
    try:
        environment = os.environ.copy()
        # Native fallback is controllable by the smoke; real portal selection is manual.
        environment["GDK_DEBUG"] = ",".join(filter(None, [environment.get("GDK_DEBUG"), "no-portals"]))
        environment["GSETTINGS_BACKEND"] = "memory"
        with tempfile.TemporaryDirectory(prefix="markdown-shell-smoke-") as scratch:
            for name in ("DATA", "CONFIG", "CACHE"):
                directory = Path(scratch) / name.lower()
                directory.mkdir()
                environment[f"XDG_{name}_HOME"] = str(directory)
            result = subprocess.run([str(binary), flag], text=True, capture_output=True,
                                    timeout=30, env=environment)
    except subprocess.TimeoutExpired as error:
        # TimeoutExpired can carry bytes even when text=True was requested.
        for output in (error.stdout, error.stderr):
            if output:
                print(output.decode(errors="replace") if isinstance(output, bytes) else output,
                      end="", flush=True)
        raise
    print(result.stdout, end="", flush=True)
    if result.stderr:
        print(result.stderr, end="", flush=True)
    result.check_returncode()
    # The pass marker precedes the closing command; the exit line proves the loop then ended.
    if marker not in result.stdout or "shell-smoke: event loop exited" not in result.stdout:
        raise SystemExit(f"{binary} {flag}: exited without completing its smoke")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    if not (os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY")):
        raise SystemExit("A graphical session is required: run under Wayland, X11 or xvfb-run")
    run(sys.executable, "tools/generate_performance_fixtures.py", cwd=REPO_ROOT)
    for configuration in ("debug", "release"):
        prepare_core(configuration)
    run("cargo", "fmt", "--all", "--check")
    cargo("clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
    run(sys.executable, "tools/check_dependencies.py", "linux", cwd=REPO_ROOT)
    for configuration in ("debug", "release"):
        binary = build(configuration)
        verify(configuration)
        for flag, marker in SMOKES.items():
            smoke(binary, flag, marker)
        run(sys.executable, str(APP_ROOT / "tools" / "check_association.py"), str(binary))
        print(f"{configuration}: core/document tests, GTK shell and isolated association smokes passed", flush=True)
    # The measurement probe refuses anything but the release package; one short run.
    run(str(TARGET_DIR / "release" / "core-probe"), "single",
        str(REPO_ROOT / "build" / "fixtures" / "performance" / "prose-1mib.md"))


if __name__ == "__main__":
    main()
