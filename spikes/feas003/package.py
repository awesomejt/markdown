#!/usr/bin/env python3
"""Assemble an unsigned, framework-dependent local development folder and ZIP."""
from __future__ import annotations
import json
import shutil
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    output = ROOT / "build/feas003/packages"
    output.mkdir(parents=True, exist_ok=True)
    package = Path(tempfile.mkdtemp(prefix="development-", dir=output))
    binaries = next((ROOT / "build/feas003/csharp/bin/x64/Release").rglob("WindowsRenderingProbe.exe")).parent
    shutil.copytree(binaries, package, dirs_exist_ok=True)
    shutil.copytree(ROOT / "fixtures", package / "fixtures")
    lock = json.loads((ROOT / "spikes/feas003/csharp/packages.lock.json").read_text(encoding="utf-8"))
    dependencies = {}
    licenses = package / "licenses"
    licenses.mkdir()
    shutil.copytree(ROOT / "spikes/feas003/licenses", licenses, dirs_exist_ok=True)
    for packages in lock["dependencies"].values():
        for name, entry in packages.items():
            version = entry["resolved"]
            dependencies[name] = {"version": version, "content_hash": entry["contentHash"]}
            source = Path.home() / ".nuget/packages" / name.lower() / version
            destination = licenses / name
            destination.mkdir(exist_ok=True)
            for path in source.iterdir():
                if path.suffix == ".nuspec" or any(word in path.name.lower() for word in ("license", "notice", "copyright")):
                    if path.is_file():
                        shutil.copy2(path, destination / path.name)
    (package / "dependencies.json").write_text(json.dumps(dependencies, indent=2), encoding="utf-8")
    (package / "README.txt").write_text(
        "FEAS-003 local development probe; no signing, installation, publishing or production approval.\n"
        "Requires x64 Windows, .NET 10 runtime, Windows App SDK 2.5 runtime and MSVC runtime.\n"
        "From this folder: WindowsRenderingProbe.exe richtext fixtures/markdown/foundation/gfm-tour.md\n"
        "Surfaces: richtext, richedit, blocks. No WebView, HTML execution or resource downloads.\n"
        "NativeProbe.exe is a literal-text C++/WinRT host smoke, not an equivalent Markdown adapter.\n"
        "Fixture origin: markdown-viewer 0ab59c0f7bf91730bf9683b47a5eb2658f331b00 (Apache-2.0).\n"
        "Dependency versions and licensing metadata are included in dependencies.json and licenses/.\n",
        encoding="utf-8")
    archive = shutil.make_archive(str(package), "zip", root_dir=package)
    print(json.dumps({"folder": str(package), "zip": archive}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
