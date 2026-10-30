#!/usr/bin/env python3
"""Build and run the real AppKit bundle smoke in debug and release."""
import subprocess

from build import APP_ROOT, build


subprocess.run(["swift", "format", "lint", "--strict", "--recursive",
                str(APP_ROOT / "Package.swift"), str(APP_ROOT / "Sources")], check=True)
for configuration in ("debug", "release"):
    app = build(configuration)
    result = subprocess.run([str(app / "Contents" / "MacOS" / "Markdown"), "--smoke-test"],
                            text=True, capture_output=True, timeout=30)
    print(result.stdout, end="", flush=True)
    if result.stderr:
        print(result.stderr, end="", flush=True)
    result.check_returncode()
    if "shell-smoke: pass" not in result.stdout:
        raise SystemExit(f"{configuration}: app exited without completing its smoke")
    print(f"{configuration}: AppKit bundle smoke passed", flush=True)
