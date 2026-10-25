#!/usr/bin/env python3
"""Assemble fresh local Mac development bundles with the normal Rust dylib."""
from __future__ import annotations

import hashlib
import json
import os
import platform
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006/macos"


def main() -> None:
    if sys.platform != "darwin":
        raise SystemExit("Native macOS required")
    parent = BUILD / "packages"
    parent.mkdir(parents=True, exist_ok=True)
    package = Path(tempfile.mkdtemp(prefix="development-", dir=parent))
    standard = BUILD / "standard"
    symbols = subprocess.check_output(["nm", "-gU", str(standard / "libfeas006_core.dylib")], text=True)
    if "_md_probe_" in symbols:
        raise RuntimeError("Refusing to package probe exports")
    for name in ("SwiftCoreProbe", "libfeas006_core.dylib"):
        shutil.copy2(standard / name, package / name)
    abi = json.loads(subprocess.check_output([str(package / "SwiftCoreProbe"), "--abi"], cwd=package))
    if abi["probe_hooks"] != 0:
        raise RuntimeError("Normal library reports probe hooks")
    shutil.copy2(ROOT / "spikes/feas006/include/md_core.h", package)
    shutil.copytree(ROOT / "fixtures", package / "fixtures")
    for mode in ("tk1", "tk2", "blocks"):
        contents = package / f"MarkdownCore-{mode}.app/Contents"
        for folder in ("MacOS", "Frameworks", "Resources"):
            (contents / folder).mkdir(parents=True, exist_ok=True)
        shutil.copy2(standard / "MacCoreProbe", contents / "MacOS/MacCoreProbe")
        shutil.copy2(standard / "libfeas006_core.dylib", contents / "Frameworks")
        shutil.copy2(ROOT / "fixtures/markdown/foundation/gfm-tour.md", contents / "Resources/gfm-tour.md")
        shutil.copy2(ROOT / "fixtures/licenses/markdown-viewer-APACHE-2.0.txt", contents / "Resources/LICENSE-fixture.txt")
        info = {"CFBundleExecutable": "MacCoreProbe", "CFBundleIdentifier": "dev.markdown.feas006." + mode,
                "CFBundleName": "MarkdownCore-" + mode, "CFBundlePackageType": "APPL", "CFBundleVersion": "1",
                "NSHighResolutionCapable": True, "LSMinimumSystemVersion": "13.0", "ProbeMode": mode,
                "FixtureOriginCommit": "0ab59c0f7bf91730bf9683b47a5eb2658f331b00"}
        (contents / "Info.plist").write_bytes(plistlib.dumps(info))
    licenses = package / "licenses"
    licenses.mkdir()
    lock = tomllib.loads((ROOT / "spikes/feas006/rust/Cargo.lock").read_text())
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    if not (cargo_home / "registry/src").exists():
        cargo_home = ROOT / "build/feas002-macos/toolchains/cargo"
    dependencies = []
    for entry in lock["package"]:
        if "source" not in entry:
            continue
        source = next((cargo_home / "registry/src").glob(f"*/{entry['name']}-{entry['version']}"))
        destination = licenses / (entry["name"] + "-" + entry["version"])
        destination.mkdir()
        files = [p for p in source.iterdir() if p.is_file() and p.name.lower().startswith(("license", "copying", "notice", "unlicense"))]
        if not files:
            raise RuntimeError("Missing locked dependency license")
        for file in files:
            shutil.copy2(file, destination / file.name)
        dependencies.append(entry)
    metadata = {"abi": abi, "rust_packages": dependencies, "architecture": platform.machine(),
                "core_sha256": hashlib.sha256((package / "libfeas006_core.dylib").read_bytes()).hexdigest(),
                "probe_exports": False, "identity_signed": False, "notarized": False,
                "swift_markdown_dependency": False}
    (package / "dependencies.json").write_text(json.dumps(metadata, indent=2))
    (package / "README.txt").write_text(
        "FEAS-006 Mac development checkpoint; experimental ABI/model.\n"
        "Tested only on arm64 macOS 27.0.1. Declared macOS 13 floor is unverified.\n"
        "Normal library uses @rpath; console resolves adjacent dylib, apps resolve Contents/Frameworks.\n"
        "No Swift Markdown runtime dependency or md_probe_* exports. Apple frameworks/Swift runtime required.\n"
        "Default linker ad-hoc signatures only; no identity signing, sealing, notarization, install or publication.\n"
        "Run: ./SwiftCoreProbe fixtures/markdown/foundation/gfm-tour.md 5\n"
        "Fixture provenance/license: fixtures/manifest.yaml and fixtures/licenses/.\n")
    archive = shutil.make_archive(str(package), "zip", root_dir=package)
    print(json.dumps({"folder": str(package), "zip": archive}))


if __name__ == "__main__":
    main()
