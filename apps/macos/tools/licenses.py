"""Inventory and notices for the libraries linked into the app (MAC-001I)."""
import json
from pathlib import Path
import subprocess
import sys

from package_core import REPO_ROOT

sys.path.insert(0, str(REPO_ROOT / "tools"))
from check_dependencies import ALLOWED, CRATES_IO, REVIEWED  # noqa: E402

TARGET = "aarch64-apple-darwin"
FEATURES = ["model-v2"]
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")
# Notices the repository's own crates carry besides the project LICENSE.
WORKSPACE_NOTICES = {"markdown-core": [REPO_ROOT / "licenses/Unicode-3.0.txt"]}
SYSTEM_NOTE = ("The app also uses the Swift runtime and Apple system frameworks supplied by "
               "macOS. They are part of the operating system and are not included in this "
               "bundle.")


def runtime_packages() -> list[dict]:
    """Packages statically linked through markdown-ffi, as the core package builds it.

    Only normal dependencies are followed. Proc-macro crates run in the compiler
    and are not linked, so they and their own dependencies are left out; build
    and dev dependencies are never followed.
    """
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET,
         "--features", ",".join(f"markdown-ffi/{feature}" for feature in FEATURES)],
        cwd=REPO_ROOT, text=True))
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    members = set(metadata["workspace_members"])
    root = next(i for i in members if packages[i]["name"] == "markdown-ffi")
    linked, pending = set(), [root]
    while pending:
        package_id = pending.pop()
        if package_id in linked:
            continue
        if any("proc-macro" in target["kind"] for target in packages[package_id]["targets"]):
            continue
        linked.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"]
                       if any(kind["kind"] is None for kind in dependency["dep_kinds"]))
    inventory, errors = [], []
    for package_id in sorted(linked, key=lambda i: (packages[i]["name"], packages[i]["version"])):
        package = packages[package_id]
        name, version = package["name"], package["version"]
        expression = " ".join((package.get("license") or "").split())
        if expression not in ALLOWED and expression != REVIEWED.get((name, version)):
            errors.append(f"{name} {version}: unapproved or missing license {expression!r}")
        if package_id in members:
            files = WORKSPACE_NOTICES.get(name, [])
            source = "workspace"
        else:
            if package.get("source") != CRATES_IO:
                errors.append(f"{name} {version}: non-crates.io source")
            directory = Path(package["manifest_path"]).parent
            files = sorted(path.resolve() for path in directory.iterdir()
                           if path.is_file() and path.name.upper().startswith(LICENSE_PREFIXES))
            if package.get("license_file"):
                declared = (directory / package["license_file"]).resolve()
                if declared.is_file() and declared not in files:
                    files.append(declared)
            if not files:
                errors.append(f"{name} {version}: no license text in the package")
            source = "crates.io"
        inventory.append({"name": name, "version": version, "license": expression,
                          "source": source, "files": files})
    if errors:
        raise SystemExit("license inventory failed:\n" + "\n".join(errors))
    return inventory


def write_notices(resources: Path) -> list[dict]:
    """Write LICENSE, THIRD-PARTY-NOTICES.txt and inventory.json to Resources/Licenses."""
    inventory = runtime_packages()
    directory = resources / "Licenses"
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "LICENSE").write_bytes((REPO_ROOT / "LICENSE").read_bytes())
    rule = "=" * 72
    lines = ["Markdown third-party notices", "",
             "Markdown is licensed under the Apache License, Version 2.0 (see LICENSE).",
             f"It statically links the following Rust packages ({TARGET}, features: "
             f"{', '.join(FEATURES)}).", "", SYSTEM_NOTE, ""]
    for package in inventory:
        origin = ("part of Markdown" if package["source"] == "workspace" else
                  f"https://crates.io/crates/{package['name']}/{package['version']}")
        lines += [rule, f"{package['name']} {package['version']}",
                  f"License: {package['license']}", f"Source: {origin}"]
        for path in package["files"]:
            lines += [f"----- {path.name} -----",
                      path.read_text(encoding="utf-8", errors="replace").rstrip(), ""]
        lines.append("")
    (directory / "THIRD-PARTY-NOTICES.txt").write_text("\n".join(lines), encoding="utf-8")
    record = [dict(package, files=[path.name for path in package["files"]])
              for package in inventory]
    (directory / "inventory.json").write_text(json.dumps(
        {"target": TARGET, "features": FEATURES, "packages": record, "system": SYSTEM_NOTE},
        indent=2) + "\n")
    return record
