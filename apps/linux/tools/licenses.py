"""Inventory and notices for the Rust libraries linked into the Linux app (LNX-001I)."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

from package_core import APP_ROOT, REPO_ROOT, TARGET

sys.path.insert(0, str(REPO_ROOT / "tools"))
from check_dependencies import ALLOWED, CRATES_IO, LINUX_REVIEWED  # noqa: E402

FEATURES = ["model-v2"]
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")
# Notices the repository's own crates carry besides the project LICENSE.
WORKSPACE_NOTICES = {"markdown-core": [REPO_ROOT / "licenses/Unicode-3.0.txt"]}
SYSTEM_NOTE = (
    "The app also dynamically links GTK 4, GLib, GIO, Pango, Cairo, GDK-Pixbuf, "
    "Graphene, glibc, and standard Linux desktop libraries supplied by the host "
    "operating system. They are part of the operating system and are not included "
    "in this archive."
)


def _collect_graph(cwd: Path, root_name: str, extra_args: list[str]) -> dict[str, dict]:
    """Return normal (non-proc-macro, non-build, non-dev) packages reachable from root_name."""
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--locked",
                "--format-version",
                "1",
                "--filter-platform",
                TARGET,
                *extra_args,
            ],
            cwd=cwd,
            text=True,
        )
    )
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    members = set(metadata["workspace_members"])
    root = next(i for i in members if packages[i]["name"] == root_name)
    linked: set[str] = set()
    pending = [root]
    while pending:
        package_id = pending.pop()
        if package_id in linked:
            continue
        if any("proc-macro" in target["kind"] for target in packages[package_id]["targets"]):
            continue
        linked.add(package_id)
        pending.extend(
            dependency["pkg"]
            for dependency in nodes[package_id]["deps"]
            if any(kind["kind"] is None for kind in dependency["dep_kinds"])
        )
    result: dict[str, dict] = {}
    for package_id in linked:
        package = dict(packages[package_id])
        package["_is_workspace"] = package_id in members
        result[package_id] = package
    return result


def runtime_packages() -> list[dict]:
    """Packages linked into libmarkdown_ffi.so and markdown-linux.

    Only normal dependencies are followed. Proc-macro crates run in the compiler
    and are not linked, so they and their own dependencies are left out; build
    and dev dependencies are never followed.
    """
    graphs = {
        "core": _collect_graph(
            REPO_ROOT,
            "markdown-ffi",
            ["--features", ",".join(f"markdown-ffi/{feature}" for feature in FEATURES)],
        ),
        "app": _collect_graph(APP_ROOT, "markdown-linux", []),
    }
    combined: dict[tuple[str, str], dict] = {}
    for component, pkgs in graphs.items():
        for package in pkgs.values():
            key = (package["name"], package["version"])
            entry = combined.get(key)
            if entry is None:
                combined[key] = {"package": package, "components": {component}}
            else:
                entry["components"].add(component)

    inventory, errors = [], []
    for (name, version), entry in sorted(combined.items()):
        package = entry["package"]
        expression = " ".join((package.get("license") or "").split())
        if expression not in ALLOWED and expression != LINUX_REVIEWED.get((name, version)):
            errors.append(f"{name} {version}: unapproved or missing license {expression!r}")
        if package["_is_workspace"]:
            files = WORKSPACE_NOTICES.get(name, [])
            source = "workspace"
        else:
            if package.get("source") != CRATES_IO:
                errors.append(f"{name} {version}: non-crates.io source")
            directory = Path(package["manifest_path"]).parent
            files = sorted(
                path.resolve()
                for path in directory.iterdir()
                if path.is_file() and path.name.upper().startswith(LICENSE_PREFIXES)
            )
            if package.get("license_file"):
                declared = (directory / package["license_file"]).resolve()
                if declared.is_file() and declared not in files:
                    files.append(declared)
            if not files:
                errors.append(f"{name} {version}: no license text in the package")
            source = "crates.io"
        inventory.append(
            {
                "name": name,
                "version": version,
                "license": expression,
                "source": source,
                "components": sorted(entry["components"]),
                "files": files,
            }
        )
    if errors:
        raise SystemExit("license inventory failed:\n" + "\n".join(errors))
    return inventory


def write_notices(bundle_root: Path) -> list[dict]:
    """Write LICENSE, THIRD-PARTY-NOTICES.txt and inventory.json to bundle_root/Licenses."""
    inventory = runtime_packages()
    directory = bundle_root / "Licenses"
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "LICENSE").write_bytes((REPO_ROOT / "LICENSE").read_bytes())
    rule = "=" * 72
    lines = [
        "Markdown third-party notices",
        "",
        "Markdown is licensed under the Apache License, Version 2.0 (see LICENSE).",
        (
            f"It links the following Rust packages ({TARGET}, core features: "
            f"{', '.join(FEATURES)})."
        ),
        "",
        SYSTEM_NOTE,
        "",
    ]
    for package in inventory:
        origin = (
            "part of Markdown"
            if package["source"] == "workspace"
            else f"https://crates.io/crates/{package['name']}/{package['version']}"
        )
        lines += [
            rule,
            f"{package['name']} {package['version']}",
            f"License: {package['license']}",
            f"Components: {', '.join(package['components'])}",
            f"Source: {origin}",
        ]
        for path in package["files"]:
            lines += [
                f"----- {path.name} -----",
                path.read_text(encoding="utf-8", errors="replace").rstrip(),
                "",
            ]
        lines.append("")
    (directory / "THIRD-PARTY-NOTICES.txt").write_text("\n".join(lines), encoding="utf-8")
    record = [
        dict(package, files=[path.name for path in package["files"]]) for package in inventory
    ]
    (directory / "inventory.json").write_text(
        json.dumps(
            {"target": TARGET, "features": FEATURES, "packages": record, "system": SYSTEM_NOTE},
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return record
