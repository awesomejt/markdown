#!/usr/bin/env python3
"""Build with SwiftPM and assemble a local, unsigned development .app."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

from licenses import write_notices
from package_core import prepare_core

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]


def build(configuration: str, *, prepared: bool = False) -> Path:
    if not prepared:
        prepare_core(configuration)
    manifest = json.loads((APP_ROOT / "CorePackage/manifest.json").read_text())
    if manifest["configuration"] != configuration:
        raise SystemExit("prepare the matching core profile before the Swift build")
    command = ["swift", "build", "--package-path", str(APP_ROOT),
               "--configuration", configuration, "-Xswiftc", "-warnings-as-errors"]
    subprocess.run(command, check=True)
    binary_dir = Path(subprocess.check_output(command + ["--show-bin-path"], text=True).strip())
    app = REPO_ROOT / "build" / "macos" / configuration / "Markdown.app"
    # Only replace this tool's generated bundle; no signing, installation or registration.
    if app.exists():
        shutil.rmtree(app)
    executable_dir = app / "Contents" / "MacOS"
    executable_dir.mkdir(parents=True)
    shutil.copy2(binary_dir / "Markdown", executable_dir / "Markdown")
    shutil.copy2(APP_ROOT / "Info.plist", app / "Contents" / "Info.plist")
    subprocess.run(["plutil", "-lint", str(app / "Contents" / "Info.plist")], check=True)
    resources = app / "Contents" / "Resources"
    write_notices(resources)
    (resources / "BuildInfo.json").write_text(json.dumps(build_info(configuration, manifest), indent=2) + "\n")
    return app


def build_info(configuration: str, manifest: dict) -> dict:
    """What built this bundle: versions and hashes only, never a path."""
    def output(*command: str) -> str:
        return subprocess.check_output(command, cwd=REPO_ROOT, text=True,
                                       stderr=subprocess.DEVNULL).strip()

    dirty = bool(output("git", "status", "--porcelain"))
    return {"configuration": configuration, "source_commit": manifest["source_commit"],
            "source_dirty": dirty, "target": manifest["target"],
            "minimum_macos": manifest["minimum_macos"], "rustc": manifest["rustc"],
            "swift": output("swift", "--version").splitlines()[0],
            "xcode": " ".join(output("xcodebuild", "-version").split()),
            "core_library_sha256": manifest["libraries"][configuration]["sha256"],
            "signed": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", choices=("debug", "release"), default="debug")
    print(build(parser.parse_args().configuration))
