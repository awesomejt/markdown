#!/usr/bin/env python3
"""Publish a fresh unsigned folder with the normal, non-probe Rust DLL."""
from __future__ import annotations
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006"


def main() -> int:
    if sys.platform != "win32": raise SystemExit("This packages the Windows host only")
    parent = BUILD / "packages";parent.mkdir(parents=True, exist_ok=True)
    package = Path(tempfile.mkdtemp(prefix="development-", dir=parent))
    subprocess.run(["dotnet", "publish", "spikes/feas006/dotnet/WinUiProbe/WinUiProbe.csproj",
                    "-c", "Release", "-p:Platform=x64", "--self-contained", "false", "-o", str(package), "--nologo"],
                   cwd=ROOT, check=True, stdout=sys.stderr)
    standard = BUILD / "rust-standard/release"
    for name in ("feas006_core.dll", "feas006_core.pdb"):
        shutil.copy2(standard / name, package / name)
    shutil.copy2(BUILD / "native/FfiNativeClient.exe", package)
    shutil.copy2(ROOT / "spikes/feas006/include/md_core.h", package)
    shutil.copytree(ROOT / "fixtures", package / "fixtures")
    licenses = package / "licenses"; licenses.mkdir()
    cargo_lock = tomllib.loads((ROOT / "spikes/feas006/rust/Cargo.lock").read_text(encoding="utf-8"))
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    rust_packages = []
    for dependency in cargo_lock["package"]:
        if "source" not in dependency: continue
        name, version = dependency["name"], dependency["version"]
        source = next((cargo_home / "registry/src").glob(f"*/{name}-{version}"))
        destination = licenses / (name + "-" + version);destination.mkdir()
        copied = 0
        for file in source.iterdir():
            if file.is_file() and file.name.lower().startswith(("license", "copying", "notice", "unlicense")):
                shutil.copy2(file, destination / file.name);copied += 1
        if not copied: raise RuntimeError(f"Missing license files for {name}")
        rust_packages.append(dependency)
    nuget = json.loads((ROOT / "spikes/feas006/dotnet/WinUiProbe/packages.lock.json").read_text(encoding="utf-8"))
    for group in nuget["dependencies"].values():
        for name, entry in group.items():
            source = Path.home() / ".nuget/packages" / name.lower() / entry["resolved"]
            destination = licenses / name;destination.mkdir(exist_ok=True)
            for file in source.iterdir():
                if file.is_file() and (file.suffix == ".nuspec" or any(word in file.name.lower() for word in ("license", "notice"))):
                    shutil.copy2(file, destination / file.name)
    metadata = {"rust_packages": rust_packages, "nuget_lock": nuget,
                "core_dll_sha256": hashlib.sha256((package / "feas006_core.dll").read_bytes()).hexdigest(),
                "probe_hooks": False, "signed": False, "self_contained": False}
    (package / "dependencies.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    (package / "README.txt").write_text(
        "FEAS-006 Windows development probe; experimental ABI, no production acceptance.\n"
        "Requires x64 Windows, .NET 10, Windows App SDK 2.5 and MSVC runtimes.\n"
        "Run here: Feas006WindowsProbe.exe blocks fixtures/markdown/foundation/gfm-tour.md\n"
        "The adjacent Rust DLL has probe/panic/breakpoint exports disabled.\n"
        "No signing, installation, MSIX registration, publishing or clean-machine proof.\n"
        "Fixture origin/license: see fixtures/manifest.yaml and fixtures/licenses/.\n",
        encoding="utf-8")
    archive = shutil.make_archive(str(package), "zip", root_dir=package)
    print(json.dumps({"folder": str(package), "zip": archive}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
