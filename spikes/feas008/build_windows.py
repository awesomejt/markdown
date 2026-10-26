#!/usr/bin/env python3
"""Build the FEAS-008 Windows hosts: the Rust model DLL, the C# WinUI host
(JIT build and Native AOT publish), the C++/WinRT host and the WebView2
baseline. Outputs go under build/feas008/windows/<variant>/."""
from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPIKE = ROOT / "spikes/feas008"
BUILD = ROOT / "build/feas008"
OUT = BUILD / "windows"
VSWHERE = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"


def run(*command: str, cwd: Path = ROOT) -> None:
    print("+", subprocess.list2cmdline(command), flush=True)
    # The Native AOT linker locates MSVC through vswhere on PATH.
    env = dict(os.environ, PATH=str(VSWHERE.parent) + os.pathsep + os.environ.get("PATH", ""))
    subprocess.run(list(command), check=True, cwd=cwd, env=env)


def ffi() -> Path:
    run("cargo", "build", "--manifest-path", str(SPIKE / "Cargo.toml"), "--locked", "--release", "-p", "feas008-ffi")
    return SPIKE / "target/release"


def place(output: Path, rust: Path, exe: str) -> None:
    for name in ("feas008_ffi.dll", "feas008_ffi.pdb"):
        shutil.copy2(rust / name, output / name)
    print(f"{exe}: {output / exe}")


def csharp_jit(rust: Path) -> None:
    project = SPIKE / "windows/csharp/jit/Feas008WinUI.csproj"
    output = OUT / "csharp-jit"
    run("dotnet", "build", str(project), "-c", "Release", "-p:Platform=x64", "--nologo", "-o", str(output))
    place(output, rust, "Feas008WinUI.exe")


def csharp_aot(rust: Path) -> None:
    project = SPIKE / "windows/csharp/aot/Feas008WinUIAot.csproj"
    output = OUT / "csharp-aot"
    run("dotnet", "publish", str(project), "-c", "Release", "-p:Platform=x64", "--nologo", "-o", str(output))
    place(output, rust, "Feas008WinUIAot.exe")


CPPWINRT_VERSION = "3.0.260818.1"
CPPWINRT_SHA256 = "a8993608ae9263a8288e1a1d9ee2684a5632f4acc59e1e89ddd86b3b889a7367"
NUGET = Path.home() / ".nuget/packages"
FOUNDATION = NUGET / "microsoft.windowsappsdk.foundation/2.3.12"


def cppwinrt_headers() -> Path:
    """Projection headers from the same pinned generator and WinUI metadata
    the C# build restores (FEAS-003 used the same versions)."""
    tools = BUILD / "toolchains"
    tools.mkdir(parents=True, exist_ok=True)
    package = tools / "cppwinrt.nupkg"
    if not package.exists():
        url = f"https://api.nuget.org/v3-flatcontainer/microsoft.windows.cppwinrt/{CPPWINRT_VERSION}/microsoft.windows.cppwinrt.{CPPWINRT_VERSION}.nupkg"
        urllib.request.urlretrieve(url, package)
    if hashlib.sha256(package.read_bytes()).hexdigest() != CPPWINRT_SHA256:
        raise SystemExit("C++/WinRT tool package hash mismatch")
    extracted = tools / "cppwinrt"
    if not extracted.exists():
        with zipfile.ZipFile(package) as archive:
            archive.extractall(extracted)
    headers = BUILD / "cppwinrt-headers"
    if not (headers / "winrt/Microsoft.UI.Xaml.Controls.h").exists():
        metadata = [
            Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Windows Kits/10/UnionMetadata/10.0.26100.0/Windows.winmd",
            NUGET / "microsoft.windowsappsdk.winui/2.3.9/metadata",
            FOUNDATION / "metadata",
            NUGET / "microsoft.windowsappsdk.interactiveexperiences/2.1.9/metadata/10.0.18362.0",
            NUGET / "microsoft.web.webview2/1.0.3719.77/lib/Microsoft.Web.WebView2.Core.winmd",
        ]
        command = [str(extracted / "bin/cppwinrt.exe")]
        for path in metadata:
            command += ["-input", str(path)]
        run(*command, "-output", str(headers))
    return headers


def msvc(commands: list[list[str]], workdir: Path) -> None:
    vs = subprocess.check_output([str(VSWHERE), "-latest", "-products", "*", "-requires",
                                  "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], text=True).strip()
    batch = workdir / "build.cmd"
    lines = [subprocess.list2cmdline(c) + "\nif errorlevel 1 exit /b %errorlevel%\n" for c in commands]
    batch.write_text('@echo off\ncall "' + vs + '\\VC\\Auxiliary\\Build\\vcvars64.bat" >nul\n' + "".join(lines), encoding="utf-8")
    run("cmd.exe", "/d", "/c", str(batch), cwd=workdir)


def cpp(rust: Path) -> None:
    headers = cppwinrt_headers()
    output = OUT / "cpp"
    obj = BUILD / "cpp-obj"
    output.mkdir(parents=True, exist_ok=True)
    obj.mkdir(parents=True, exist_ok=True)
    source = SPIKE / "windows/cpp/Feas008Cpp.cpp"
    msvc([["cl", "/nologo", "/std:c++20", "/EHsc", "/O2", "/Zi", "/MD", "/permissive-", "/bigobj", "/utf-8",
           "/I" + str(headers), "/I" + str(FOUNDATION / "include"), "/I" + str(NUGET / "microsoft.windowsappsdk.winui/2.3.9/include"),
           "/I" + str(SPIKE / "ffi"), str(source),
           "/Fe:" + str(output / "Feas008Cpp.exe"), "/Fo:" + str(obj / "Feas008Cpp.obj"), "/Fd:" + str(obj / "Feas008Cpp-compile.pdb"),
           "/link", "/SUBSYSTEM:WINDOWS", "/DEBUG", "/PDB:" + str(output / "Feas008Cpp.pdb"), "/MANIFEST:EMBED",
           "/MANIFESTINPUT:" + str(SPIKE / "windows/csharp/app.manifest"),
           str(FOUNDATION / "lib/native/x64/Microsoft.WindowsAppRuntime.Bootstrap.lib"), str(rust / "feas008_ffi.dll.lib"),
           "windowsapp.lib", "user32.lib", "shell32.lib", "psapi.lib", "dwmapi.lib"]], obj)
    shutil.copy2(FOUNDATION / "runtimes/win-x64/native/Microsoft.WindowsAppRuntime.Bootstrap.dll", output)
    place(output, rust, "Feas008Cpp.exe")


def webview(rust: Path) -> None:
    project = SPIKE / "windows/webview/Feas008WebView.csproj"
    output = OUT / "webview"
    run("dotnet", "publish", str(project), "-c", "Release", "-p:Platform=x64", "--nologo", "-o", str(output))
    place(output, rust, "Feas008WebView.exe")


def uia(rust: Path) -> None:
    output = OUT / "uia"
    output.mkdir(parents=True, exist_ok=True)
    msvc([["cl", "/nologo", "/std:c++20", "/EHsc", "/O2", "/utf-8", str(SPIKE / "windows/uia/UiaProbe.cpp"),
           "/Fe:" + str(output / "UiaProbe.exe"), "/Fo:" + str(output / "UiaProbe.obj"),
           "/link", "uiautomationcore.lib", "ole32.lib", "oleaut32.lib", "user32.lib"]], output)
    print(f"UiaProbe.exe: {output / 'UiaProbe.exe'}")


def main() -> int:
    if sys.platform != "win32":
        raise SystemExit("Use native Windows Python for the FEAS-008 Windows hosts")
    parser = argparse.ArgumentParser()
    parser.add_argument("targets", nargs="*", default=["csharp-jit", "csharp-aot", "cpp", "webview"])
    args = parser.parse_args()
    rust = ffi()
    builders = {"csharp-jit": csharp_jit, "csharp-aot": csharp_aot, "cpp": cpp, "webview": webview, "uia": uia}
    for target in args.targets:
        builders[target](rust)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
