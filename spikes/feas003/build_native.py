#!/usr/bin/env python3
"""Build the bounded native host smoke using pinned WinUI metadata and MSVC."""
from __future__ import annotations
import hashlib
import json
import os
import shutil
import subprocess
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    if os.name != "nt":
        raise SystemExit("Native Windows/MSVC required")
    build = ROOT / "build/feas003"
    tools = build / "toolchains"
    tools.mkdir(parents=True, exist_ok=True)
    package = tools / "cppwinrt.nupkg"
    version = "3.0.260818.1"
    url = f"https://api.nuget.org/v3-flatcontainer/microsoft.windows.cppwinrt/{version}/microsoft.windows.cppwinrt.{version}.nupkg"
    if not package.exists():
        urllib.request.urlretrieve(url, package)
    if hashlib.sha256(package.read_bytes()).hexdigest() != "a8993608ae9263a8288e1a1d9ee2684a5632f4acc59e1e89ddd86b3b889a7367":
        raise SystemExit("C++/WinRT tool package hash mismatch")
    extracted = tools / "cppwinrt"
    if not extracted.exists():
        with zipfile.ZipFile(package) as archive:
            archive.extractall(extracted)
    nuget = Path.home() / ".nuget/packages"
    metadata = [
        Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Windows Kits/10/UnionMetadata/10.0.26100.0/Windows.winmd",
        nuget / "microsoft.windowsappsdk.winui/2.3.9/metadata",
        nuget / "microsoft.windowsappsdk.foundation/2.3.12/metadata",
        nuget / "microsoft.windowsappsdk.interactiveexperiences/2.1.9/metadata/10.0.18362.0",
        nuget / "microsoft.web.webview2/1.0.3719.77/lib/Microsoft.Web.WebView2.Core.winmd",
    ]
    command = [str(extracted / "bin/cppwinrt.exe")]
    for path in metadata:
        command += ["-input", str(path)]
    command += ["-output", str(build / "native-headers")]
    subprocess.run(command, check=True)
    vswhere = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    vs = subprocess.check_output([str(vswhere), "-latest", "-products", "*", "-requires",
                                  "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], text=True).strip()
    native = build / "native"
    native.mkdir(exist_ok=True)
    cl = ["cl", "/nologo", "/std:c++20", "/EHsc", "/O2", "/Zi", "/MD",
          "/I" + str(build / "native-headers"),
          "/I" + str(nuget / "microsoft.windowsappsdk.foundation/2.3.12/include"),
          str(ROOT / "spikes/feas003/cpp/NativeProbe.cpp"),
          "/Fe:" + str(native / "NativeProbe.exe"), "/Fo:" + str(native / "NativeProbe.obj"),
          "/Fd:" + str(native / "NativeProbe-compile.pdb"), "/link", "/SUBSYSTEM:WINDOWS", "/DEBUG",
          "/PDB:" + str(native / "NativeProbe.pdb"), "/MANIFEST:EMBED",
          "/MANIFESTINPUT:" + str(ROOT / "spikes/feas003/csharp/app.manifest"),
          str(nuget / "microsoft.windowsappsdk.foundation/2.3.12/lib/native/x64/Microsoft.WindowsAppRuntime.Bootstrap.lib"),
          "windowsapp.lib", "user32.lib", "shell32.lib", "psapi.lib"]
    batch = native / "build.cmd"
    debug_cl = ["cl", "/nologo", "/std:c++20", "/EHsc", "/O2", "/Zi", "/MD",
                str(ROOT / "spikes/feas003/cpp/DebugProbe.cpp"),
                "/Fe:" + str(native / "DebugProbe.exe"), "/Fo:" + str(native / "DebugProbe.obj"),
                "/Fd:" + str(native / "DebugProbe-compile.pdb"), "/link", "/DEBUG",
                "/PDB:" + str(native / "DebugProbe.pdb"), "dbghelp.lib"]
    batch.write_text('@echo off\ncall "' + vs + '\\VC\\Auxiliary\\Build\\vcvars64.bat" >nul\n' +
                     subprocess.list2cmdline(cl) + '\nif errorlevel 1 exit /b %errorlevel%\n' +
                     subprocess.list2cmdline(debug_cl) + '\n', encoding="utf-8")
    subprocess.run(["cmd.exe", "/d", "/c", str(batch)], check=True, cwd=native)
    output = next((build / "csharp/bin/x64/Release").rglob("WindowsRenderingProbe.exe")).parent
    shutil.copy2(native / "NativeProbe.exe", output)
    shutil.copy2(native / "NativeProbe.pdb", output)
    (native / "toolchain.json").write_text(json.dumps({"cppwinrt_version": version,
        "nuget_sha256": hashlib.sha256(package.read_bytes()).hexdigest(),
        "msvc_installation": vs}, indent=2), encoding="utf-8")
    print("Native smoke executable:", output / "NativeProbe.exe")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
