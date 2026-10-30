#!/usr/bin/env python3
"""Build with SwiftPM and assemble a local, unsigned development .app."""
import argparse
from pathlib import Path
import shutil
import subprocess

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]


def build(configuration: str) -> Path:
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
    return app


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", choices=("debug", "release"), default="debug")
    print(build(parser.parse_args().configuration))
