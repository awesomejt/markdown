#!/usr/bin/env python3
"""Build and run the real AppKit bundle smoke in debug and release."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from build import APP_ROOT, REPO_ROOT, build
from package_core import prepare_core


subprocess.run(["swift", "format", "lint", "--strict", "--recursive",
                str(APP_ROOT / "Package.swift"), str(APP_ROOT / "Sources"),
                str(APP_ROOT / "Tests")], check=True)
subprocess.run([sys.executable, "tools/generate_performance_fixtures.py"], cwd=REPO_ROOT, check=True)
env = {k: v for k, v in os.environ.items() if not k.startswith("DYLD_")}
env["MARKDOWN_TEST_ROOT"] = str(REPO_ROOT)
for configuration in ("debug", "release"):
    prepare_core(configuration)
    app = build(configuration, prepared=True)
    subprocess.run(["swift", "test", "--package-path", str(APP_ROOT), "--configuration",
                    configuration, "--no-parallel", "-Xswiftc", "-warnings-as-errors"],
                   env=env, check=True, timeout=300)
    # A local static package must not turn into a runtime build-tree dependency.
    dependencies = subprocess.check_output(["otool", "-L", str(app / "Contents/MacOS/Markdown")], text=True)
    if "libmarkdown_ffi" in dependencies or str(REPO_ROOT) in "\n".join(dependencies.splitlines()[1:]):
        raise SystemExit("unexpected build-tree/dynamic Rust dependency")
    try:
        with tempfile.TemporaryDirectory(prefix="markdown-relocated-") as directory:
            relocated = Path(directory) / "Markdown.app"
            shutil.copytree(app, relocated)
            result = subprocess.run([str(relocated / "Contents/MacOS/Markdown"), "--smoke-test"],
                                    cwd=directory, env=env, text=True, capture_output=True, timeout=30)
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
    if "shell-smoke: pass" not in result.stdout:
        raise SystemExit(f"{configuration}: app exited without completing its smoke")
    print(f"{configuration}: AppKit bundle smoke passed", flush=True)

    # Real state restoration: quit with the window kept, relaunch, expect the
    # document back at its place. The verify launch closes its windows, so
    # the app's saved state is left empty.
    with tempfile.TemporaryDirectory(prefix="markdown-relaunch-") as directory:
        document = Path(directory) / "live.md"
        document.write_text("# Live\n\n" + "\n".join(
            f"## Part {index}\n\nProse for part {index}, with a *marked* word.\n"
            for index in range(120)), encoding="utf-8")
        for phase, marker in (("prepare", "relaunch-check: prepared"),
                              ("verify", "relaunch-check: pass")):
            result = subprocess.run(
                [str(app / "Contents/MacOS/Markdown"), "--relaunch-check", phase, str(document),
                 "-NSQuitAlwaysKeepsWindows", "YES"],
                cwd=directory, env=env, text=True, capture_output=True, timeout=30)
            print(result.stdout, end="", flush=True)
            if result.returncode != 0 or marker not in result.stdout:
                print(result.stderr, end="", flush=True)
                raise SystemExit(f"{configuration}: relaunch check {phase} failed")
    print(f"{configuration}: relaunch restoration passed", flush=True)

    if configuration == "release":
        binary_dir = Path(subprocess.check_output(
            ["swift", "build", "--package-path", str(APP_ROOT), "--configuration", "release",
             "--show-bin-path"], text=True).strip())
        with tempfile.TemporaryDirectory(prefix="markdown-core-consumer-") as directory:
            executable = Path(directory) / "CoreProbe"
            shutil.copy2(binary_dir / "CoreProbe", executable)
            # Synthetic input, no repository/header/library at runtime, no loader overrides.
            subprocess.run([str(executable), "single"], cwd=directory, env=env,
                           check=True, timeout=30)
