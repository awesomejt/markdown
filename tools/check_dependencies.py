"""Check the resolved production Rust graph; spike/tooling graphs are separate."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {"Apache-2.0", "MIT", "Apache-2.0 OR MIT", "MIT OR Apache-2.0"}


def violations(metadata: dict) -> list[str]:
    errors = []
    members = set(metadata["workspace_members"])
    for package in metadata["packages"]:
        name = f"{package['name']} {package['version']}"
        expression = " ".join((package.get("license") or "").split())
        if expression not in ALLOWED:
            errors.append(f"{name}: unapproved or missing SPDX license {expression!r}")
        manifest = Path(package["manifest_path"]).resolve()
        if package["id"] in members:
            if not manifest.is_relative_to(ROOT / "core"):
                errors.append(f"{name}: portable workspace member outside core/")
            if package.get("publish") != []:
                errors.append(f"{name}: production crates must remain unpublished")
        if manifest.is_relative_to(ROOT / "spikes"):
            errors.append(f"{name}: production graph depends on a feasibility spike")
    return errors


def main() -> int:
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT, text=True))
    errors = violations(metadata)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    for package in sorted(metadata["packages"], key=lambda p: p["name"]):
        kind = "workspace" if package["id"] in metadata["workspace_members"] else "third-party"
        print(f"{package['name']} {package['version']}: {package['license']} ({kind})")
    print("Production dependency/license policy passed; tooling and spikes have separate inventories.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
