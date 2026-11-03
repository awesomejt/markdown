"""Inventory and notices for the libraries linked and bundled into the Windows app (WIN-001I)."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT / "tools"))
from check_dependencies import ALLOWED, CRATES_IO, REVIEWED  # noqa: E402

TARGET = "x86_64-pc-windows-msvc"
FEATURES = ["model-v2"]
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")
WORKSPACE_NOTICES = {"markdown-core": [REPO_ROOT / "licenses/Unicode-3.0.txt"]}
SYSTEM_NOTE = (
    "The app uses the Windows App SDK and Windows system libraries supplied by "
    "Windows. They are part of the operating system or App SDK runtime and are not "
    "redistributed as source."
)

# Build-tooling NuGet packages that do not redistribute runtime DLLs into the published app.
BUILD_ONLY_NUGET = {"Microsoft.Windows.SDK.BuildTools"}

NUGET_REVIEWED = {
    ("Microsoft.DotNet.ILCompiler", "10.0.13"): "MIT",
    ("Microsoft.NET.ILLink.Tasks", "10.0.13"): "MIT",
    ("Microsoft.Web.WebView2", "1.0.3719.77"): "Microsoft Software License Terms",
    ("Microsoft.Windows.AI.MachineLearning", "2.1.74"): "Microsoft Software License Terms",
    ("Microsoft.Windows.SDK.BuildTools", "10.0.28000.2705"): "Microsoft Windows SDK Build Tools License",
    ("Microsoft.Windows.SDK.BuildTools.MSIX", "1.7.251221100"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK", "2.5.1"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.AI", "2.5.5"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.Base", "2.0.4"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.DWrite", "2.1.0"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.Foundation", "2.3.12"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.InteractiveExperiences", "2.1.9"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.ML", "2.1.94"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.Runtime", "2.5.1"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.Search", "2.5.5"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.Widgets", "2.0.5"): "Microsoft Software License Terms",
    ("Microsoft.WindowsAppSDK.WinUI", "2.3.9"): "Microsoft Software License Terms",
    ("System.Numerics.Tensors", "9.0.0"): "MIT",
    ("runtime.win-x64.Microsoft.DotNet.ILCompiler", "10.0.13"): "MIT",
}


def runtime_packages() -> list[dict]:
    """Packages statically linked through markdown-ffi, as the core package builds it."""
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
                "--features",
                ",".join(f"markdown-ffi/{feature}" for feature in FEATURES),
            ],
            cwd=REPO_ROOT,
            text=True,
        )
    )
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
        pending.extend(
            dependency["pkg"]
            for dependency in nodes[package_id]["deps"]
            if any(kind["kind"] is None for kind in dependency["dep_kinds"])
        )
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
                "files": files,
            }
        )
    if errors:
        raise SystemExit("Rust license inventory failed:\n" + "\n".join(errors))
    return inventory


def locked_nuget_packages() -> dict[str, str]:
    """All unique package name -> resolved version pairs across all targets in packages.lock.json."""
    lock_file = REPO_ROOT / "apps/windows/packages.lock.json"
    lock_data = json.loads(lock_file.read_text(encoding="utf-8"))
    resolved: dict[str, str] = {}
    for _target, deps in lock_data.get("dependencies", {}).items():
        for name, info in deps.items():
            ver = info.get("resolved")
            if ver:
                if name in resolved and resolved[name] != ver:
                    raise SystemExit(
                        f"Conflicting resolved versions for {name} in packages.lock.json: "
                        f"{resolved[name]} vs {ver}"
                    )
                resolved[name] = ver
    return resolved


def nuget_packages() -> list[dict]:
    """NuGet packages in the locked Windows shell graph and their bundled license notices."""
    nuget_root = Path(os.environ.get("NUGET_PACKAGES", Path.home() / ".nuget/packages"))
    locked = locked_nuget_packages()
    reviewed_keys = {name for (name, _ver) in NUGET_REVIEWED}

    inventory, errors = [], []
    # 1. Verify every locked package is in NUGET_REVIEWED with the exact version
    for name, version in sorted(locked.items()):
        if name not in reviewed_keys:
            errors.append(f"{name} {version}: locked NuGet package missing from review policy")
        elif (name, version) not in NUGET_REVIEWED:
            expected_ver = next(v for (n, v) in NUGET_REVIEWED if n == name)
            errors.append(f"{name}: resolved version {version} != reviewed version {expected_ver}")

    # 2. Verify every package in NUGET_REVIEWED is present in the lock file
    for name, version in sorted(NUGET_REVIEWED.keys()):
        if name not in locked:
            errors.append(f"{name} {version}: reviewed NuGet package not found in packages.lock.json")

    # 3. Collect license files and enforce that runtime packages carry license text
    for (name, version), license_desc in sorted(NUGET_REVIEWED.items()):
        pkg_dir = nuget_root / name.lower() / version
        files = []
        if pkg_dir.exists():
            files = sorted(
                path.resolve()
                for path in pkg_dir.iterdir()
                if path.is_file()
                and any(
                    token in path.name.lower()
                    for token in ("license", "licence", "notice", "copying", "third-party-notices")
                )
            )
        if not files and name not in BUILD_ONLY_NUGET:
            errors.append(f"{name} {version}: no license text in NuGet package")

        inventory.append(
            {
                "name": name,
                "version": version,
                "license": license_desc,
                "source": "nuget.org",
                "files": files,
            }
        )
    if errors:
        raise SystemExit("NuGet license inventory failed:\n" + "\n".join(errors))
    return inventory


def write_notices(directory_root: Path) -> dict:
    """Write LICENSE, THIRD-PARTY-NOTICES.txt and inventory.json to <directory_root>/Licenses."""
    rust_inv = runtime_packages()
    nuget_inv = nuget_packages()
    directory = directory_root / "Licenses"
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "LICENSE").write_bytes((REPO_ROOT / "LICENSE").read_bytes())
    rule = "=" * 72
    lines = [
        "Markdown third-party notices",
        "",
        "Markdown is licensed under the Apache License, Version 2.0 (see LICENSE).",
        f"It statically links the following Rust packages ({TARGET}, features: {', '.join(FEATURES)}):",
        "",
    ]
    for package in rust_inv:
        origin = (
            "part of Markdown"
            if package["source"] == "workspace"
            else f"https://crates.io/crates/{package['name']}/{package['version']}"
        )
        lines += [
            rule,
            f"{package['name']} {package['version']}",
            f"License: {package['license']}",
            f"Source: {origin}",
        ]
        for path in package["files"]:
            lines += [
                f"----- {path.name} -----",
                path.read_text(encoding="utf-8", errors="replace").rstrip(),
                "",
            ]
        lines.append("")

    lines += [
        rule,
        "Windows App SDK and .NET platform packages",
        "",
        "The Windows application bundles the following locked NuGet platform packages:",
        "",
    ]
    for package in nuget_inv:
        lines += [
            rule,
            f"{package['name']} {package['version']}",
            f"License: {package['license']}",
            f"Source: https://www.nuget.org/packages/{package['name']}/{package['version']}",
        ]
        for path in package["files"]:
            lines += [
                f"----- {path.name} -----",
                path.read_text(encoding="utf-8", errors="replace").rstrip(),
                "",
            ]
        lines.append("")

    lines += [rule, SYSTEM_NOTE, ""]
    (directory / "THIRD-PARTY-NOTICES.txt").write_text("\n".join(lines), encoding="utf-8")

    rust_record = [
        dict(package, files=[path.name for path in package["files"]]) for package in rust_inv
    ]
    nuget_record = [
        dict(package, files=[path.name for path in package["files"]]) for package in nuget_inv
    ]
    record = {
        "target": TARGET,
        "features": FEATURES,
        "rust_packages": rust_record,
        "nuget_packages": nuget_record,
        "system": SYSTEM_NOTE,
    }
    (directory / "inventory.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return record


if __name__ == "__main__":
    out_dir = REPO_ROOT / "build/windows/Release"
    record = write_notices(out_dir)
    print(
        f"Wrote licenses to {out_dir / 'Licenses'}: "
        f"{len(record['rust_packages'])} Rust packages, "
        f"{len(record['nuget_packages'])} NuGet packages."
    )
