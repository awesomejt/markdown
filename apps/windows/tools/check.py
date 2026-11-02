#!/usr/bin/env python3
"""Stage the Windows core packages, verify formatting, and publish/exercise the Native AOT shell."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from check_association import check_association
from package_core import prepare_core, verify

ROOT = Path(__file__).resolve().parents[3]
PROJECT = ROOT / "apps/windows/Markdown.Windows.csproj"


def run(command: list[str], *, cwd: Path, env: dict[str, str], timeout: int | None = None) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True, timeout=timeout)


def run_app(command: list[str], *, cwd: Path, env: dict[str, str], timeout: int) -> str:
    print("+ " + " ".join(command), flush=True)
    # Markdown.Windows.exe is a WinExe (GUI subsystem) binary; piping stdout/stderr
    # attaches valid standard handles so Console output is captured and logged.
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        timeout=timeout,
    )
    if result.stdout:
        print(result.stdout, end="", flush=True)
    result.check_returncode()
    return result.stdout


def main() -> None:
    if sys.platform != "win32":
        raise SystemExit("Native Windows Python is required")
    installer = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
    env = dict(os.environ)
    env["PATH"] = str(installer / "Microsoft Visual Studio/Installer") + os.pathsep + env["PATH"]

    run([sys.executable, "tools/generate_performance_fixtures.py"], cwd=ROOT, env=env)
    for configuration in ("debug", "release"):
        prepare_core(configuration)

    run(
        ["dotnet", "restore", str(PROJECT), "--locked-mode", "-p:Platform=x64"],
        cwd=PROJECT.parent,
        env=env,
    )
    run(
        ["dotnet", "format", str(PROJECT), "--no-restore", "--verify-no-changes", "--severity", "warn"],
        cwd=PROJECT.parent,
        env=env,
    )

    for profile in ("Debug", "Release"):
        configuration = profile.lower()
        output = ROOT / "build/windows" / profile
        run(
            [
                "dotnet",
                "publish",
                str(PROJECT),
                "--no-restore",
                "-c",
                profile,
                "-p:Platform=x64",
                "--nologo",
                "-o",
                str(output),
            ],
            cwd=PROJECT.parent,
            env=env,
        )
        verify(configuration)
        # No .NET runtime or globally installed App SDK is needed by this output.
        if (output / "coreclr.dll").exists():
            raise SystemExit("Expected Native AOT output, found CoreCLR")
        if not (output / "markdown_ffi.dll").is_file():
            raise SystemExit(f"Missing staged markdown_ffi.dll in {output}")

        exe = str(output / "Markdown.Windows.exe")
        run_app([exe, "--core-tests"], cwd=output, env=env, timeout=180)
        run_app([exe, "--document-tests"], cwd=output, env=env, timeout=120)
        run_app([exe, "--smoke"], cwd=output, env=env, timeout=60)
        check_association(output / "Markdown.Windows.exe")
        print(
            f"{profile}: core consumer tests, document tests, Native AOT smoke, and association check passed",
            flush=True,
        )

    release_output = ROOT / "build/windows/Release"
    with tempfile.TemporaryDirectory(prefix="markdown-windows-relocated-") as temp_root:
        relocated = Path(temp_root) / "app"
        shutil.copytree(release_output, relocated)
        relocated_exe = str(relocated / "Markdown.Windows.exe")
        run_app([relocated_exe, "--document-tests"], cwd=relocated, env=env, timeout=120)
        run_app([relocated_exe, "--smoke"], cwd=relocated, env=env, timeout=60)
        check_association(relocated / "Markdown.Windows.exe")
        run_app([relocated_exe, "--core-probe", "single"], cwd=relocated, env=env, timeout=60)
        print("Release: relocated Native AOT document tests, smoke, association, and core-probe passed", flush=True)


if __name__ == "__main__":
    main()
