#!/usr/bin/env python3
"""Build Windows-only integration probes; the Rust crate itself has no OS adapter."""
from __future__ import annotations
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006"


def run(*command: str) -> None:
    subprocess.run(list(command), check=True, cwd=ROOT)


def main() -> int:
    if sys.platform != "win32":
        raise SystemExit("Use native Windows Python for this host integration builder")
    manifest = "spikes/feas006/rust/Cargo.toml"
    target = BUILD / "rust"
    run("cargo", "build", "--manifest-path", manifest, "--locked", "--release", "--features", "probe-hooks", "--target-dir", str(target))
    run("cargo", "build", "--manifest-path", manifest, "--locked", "--release", "--target-dir", str(BUILD / "rust-standard"))
    run("dotnet", "build", "spikes/feas006/dotnet/ConsoleProbe/ConsoleProbe.csproj", "-c", "Release", "--nologo")
    run("dotnet", "build", "spikes/feas006/dotnet/WinUiProbe/WinUiProbe.csproj", "-c", "Release", "-p:Platform=x64", "--nologo")
    outputs = [next((BUILD / "console/Release").rglob("ConsoleProbe.exe")).parent,
               next((BUILD / "winui/x64/Release").rglob("Feas006WindowsProbe.exe")).parent]
    for output in outputs:
        for name in ("feas006_core.dll", "feas006_core.pdb"):
            shutil.copy2(target / "release" / name, output / name)
    vswhere = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    vs = subprocess.check_output([str(vswhere), "-latest", "-products", "*", "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], text=True).strip()
    native = BUILD / "native"; native.mkdir(parents=True, exist_ok=True)
    commands = []
    for source, name, extra in [(ROOT / "spikes/feas006/native/Client.cpp", "FfiNativeClient", []),
                               (ROOT / "spikes/feas003/cpp/DebugProbe.cpp", "DebugFfiProbe", ["/DFEAS006"])]:
        command = ["cl", "/nologo", "/std:c++20", "/EHsc", "/O2", "/Zi", "/MD", *extra,
            "/I" + str(ROOT / "spikes/feas006/include"), str(source),
            "/Fe:" + str(native / (name + ".exe")), "/Fo:" + str(native / (name + ".obj")),
            "/Fd:" + str(native / (name + "-compile.pdb")), "/link", "/DEBUG",
            "/PDB:" + str(native / (name + ".pdb")), "psapi.lib", "dbghelp.lib"]
        commands.append(subprocess.list2cmdline(command) + "\nif errorlevel 1 exit /b %errorlevel%\n")
    batch = native / "build.cmd"
    batch.write_text('@echo off\ncall "' + vs + '\\VC\\Auxiliary\\Build\\vcvars64.bat" >nul\n' + ''.join(commands), encoding="utf-8")
    run("cmd.exe", "/d", "/c", str(batch))
    print("Built Rust probe/standard DLLs, managed console/WinUI and native ABI/debug clients.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
