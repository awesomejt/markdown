#!/usr/bin/env python3
"""Create and self-verify a relocatable normal-library Linux development package."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006/linux"


def output(command: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stderr=subprocess.STDOUT,
    )


def packaged_elfs(package: Path) -> list[Path]:
    return [
        path
        for path in package.rglob("*")
        if path.is_file() and path.read_bytes()[:4] == b"\x7fELF"
    ]


def verify_private_paths_removed(package: Path) -> list[str]:
    root_bytes = str(ROOT).encode()
    checked = []
    for elf in packaged_elfs(package):
        contents = elf.read_bytes()
        if root_bytes in contents:
            raise RuntimeError(f"Packaged ELF contains checkout path bytes: {elf}")
        sections = output(["readelf", "-S", str(elf)], package)
        if ".debug_" in sections:
            raise RuntimeError(f"Packaged ELF retains DWARF debug sections: {elf}")
        checked.append(str(elf.relative_to(package)))
    if not checked:
        raise RuntimeError("Package contains no ELF artifacts")
    return checked


def copy_licenses(package: Path) -> list[dict]:
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    registry = cargo_home / "registry/src"
    dependencies: dict[tuple[str, str], dict] = {}
    for lock_path in (ROOT / "spikes/feas006/rust/Cargo.lock", ROOT / "spikes/feas006/linux/Cargo.lock"):
        lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
        for entry in lock["package"]:
            if "source" in entry:
                dependencies[(entry["name"], entry["version"])] = entry
    licenses = package / "licenses"
    licenses.mkdir()
    for (name, version), entry in sorted(dependencies.items()):
        destination = licenses / f"{name}-{version}"
        destination.mkdir()
        matches = list(registry.glob(f"*/{name}-{version}"))
        copied = 0
        if len(matches) == 1:
            files = [
                path for path in matches[0].iterdir()
                if path.is_file() and path.name.lower().startswith(("license", "copying", "notice", "unlicense"))
            ]
            for path in files:
                shutil.copy2(path, destination / path.name)
                copied += 1
        elif matches:
            raise RuntimeError(f"Ambiguous cached source for {name} {version}")
        else:
            archives = list((cargo_home / "registry/cache").glob(f"*/{name}-{version}.crate"))
            if len(archives) != 1:
                raise RuntimeError(f"Missing or ambiguous cached crate for {name} {version}")
            with tarfile.open(archives[0], "r:gz") as archive:
                for member in archive.getmembers():
                    filename = Path(member.name)
                    if len(filename.parts) == 2 and filename.name.lower().startswith(
                        ("license", "copying", "notice", "unlicense")
                    ):
                        source = archive.extractfile(member)
                        if source is not None:
                            (destination / filename.name).write_bytes(source.read())
                            copied += 1
        if not copied:
            raise RuntimeError(f"Missing locked dependency license for {name} {version}")
    return list(dependencies.values())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-gtk", action="store_true", help="Skip native GTK relocation smoke")
    args = parser.parse_args()
    if not sys.platform.startswith("linux"):
        raise SystemExit("Native Linux required")
    standard = BUILD / "standard"
    host_source = standard / "feas006-linux-host"
    core_source = standard / "libfeas006_core.so"
    symbols = output(["nm", "-D", "--defined-only", str(core_source)], ROOT)
    if "md_probe_" in symbols:
        raise RuntimeError("Refusing to package probe exports")
    expected = {
        "md_get_abi_v1", "md_parse_v1", "md_snapshot_free_v1",
        "md_cancel_create_v1", "md_cancel_request_v1", "md_cancel_free_v1",
    }
    if not expected.issubset(set(symbols.split())):
        raise RuntimeError("Normal core is missing required public ABI symbols")

    parent = BUILD / "packages"
    parent.mkdir(parents=True, exist_ok=True)
    package = Path(tempfile.mkdtemp(prefix="development-", dir=parent))
    for source in (host_source, core_source):
        shutil.copy2(source, package / source.name)
        subprocess.run(
            [
                "objcopy",
                "--strip-debug",
                "--remove-section=.debug_gdb_scripts",
                str(package / source.name),
            ],
            cwd=ROOT,
            check=True,
        )
    shutil.copy2(ROOT / "spikes/feas006/include/md_core.h", package)
    shutil.copytree(ROOT / "fixtures", package / "fixtures")
    for manifest in (ROOT / "spikes/feas006/rust/Cargo.toml", ROOT / "spikes/feas006/linux/Cargo.toml"):
        subprocess.run(
            ["cargo", "fetch", "--locked", "--manifest-path", str(manifest)],
            cwd=ROOT,
            check=True,
        )
    dependencies = copy_licenses(package)
    abi = json.loads(output([str(package / "feas006-linux-host"), "--abi"], package))
    if abi["probe_hooks"] != 0:
        raise RuntimeError("Normal library reports probe hooks")
    abi.pop("core_path", None)
    normal = json.loads(output([str(package / "feas006-linux-host"), "--normal-check"], package))
    if not normal["normal_check"]["passed"]:
        raise RuntimeError("Normal-package lifecycle check failed")
    dynamic = output(["readelf", "-d", str(package / "feas006-linux-host")], package)
    linked = output(["ldd", str(package / "feas006-linux-host")], package)
    if "libfeas006_core" in dynamic or str(ROOT) in dynamic or str(ROOT) in linked:
        raise RuntimeError("Host has a core DT_NEEDED or checkout path")
    metadata = {
        "abi": abi,
        "normal_check": normal["normal_check"],
        "architecture": platform.machine(),
        "platform": platform.platform(),
        "host_sha256": hashlib.sha256((package / "feas006-linux-host").read_bytes()).hexdigest(),
        "core_sha256": hashlib.sha256((package / "libfeas006_core.so").read_bytes()).hexdigest(),
        "probe_exports": False,
        "signed": False,
        "installed": False,
        "relocated_commands_remove_ld_library_path": True,
        "debug_artifacts": "Retained separately under ignored build/feas006/linux/debug; not packaged",
        "rust_packages": dependencies,
        "system_runtime": "GTK 4 and platform libraries are external runtime dependencies",
        "readelf_dynamic": dynamic,
        "ldd": linked,
    }
    metadata["private_path_scan_elfs"] = verify_private_paths_removed(package)
    (package / "dependencies.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    (package / "README.txt").write_text(
        "FEAS-006 Linux development checkpoint; experimental ABI/model.\n"
        "Requires x86-64 Linux with GTK 4 and compatible system libraries. No distro floor is established.\n"
        "The host loads only the adjacent libfeas006_core.so by default; no LD_LIBRARY_PATH is needed.\n"
        "The packaged library has no md_probe_* exports. No signing, install, sandbox, publishing or clean-machine proof.\n"
        "Package ELFs have debug sections removed and are scanned for checkout paths; separate local debug builds are not packaged.\n"
        "Run: ./feas006-linux-host fixtures/markdown/foundation/gfm-tour.md 5\n"
        "GTK: ./feas006-linux-host --gtk fixtures/markdown/foundation/gfm-tour.md\n"
        "Fixture origin/license: fixtures/manifest.yaml and fixtures/licenses/.\n",
        encoding="utf-8",
    )

    with tempfile.TemporaryDirectory(prefix="feas006-linux-relocated-") as temporary:
        relocated = Path(temporary) / "package"
        shutil.copytree(package, relocated)
        env = os.environ.copy()
        env.pop("LD_LIBRARY_PATH", None)
        output(
            [str(relocated / "feas006-linux-host"), "--normal-check"],
            relocated,
            env,
        )
        output([
            str(relocated / "feas006-linux-host"),
            "fixtures/markdown/foundation/gfm-tour.md", "1",
        ], relocated, env)
        gtk_checkpoint = relocated / "gtk-smoke.json"
        if not args.skip_gtk:
            subprocess.run([
                str(relocated / "feas006-linux-host"), "--gtk",
                "fixtures/markdown/foundation/gfm-tour.md",
                "--automated", str(gtk_checkpoint),
            ], cwd=relocated, env=env, check=True, timeout=30)
            gtk = json.loads(gtk_checkpoint.read_text(encoding="utf-8"))
            if gtk.get("phase") != "first-useful-tick":
                raise RuntimeError("Relocated GTK smoke did not reach first useful tick")
    archive = shutil.make_archive(str(package), "gztar", root_dir=package)
    print(json.dumps({"folder": str(package), "archive": archive, "gtk_smoke": not args.skip_gtk}))


if __name__ == "__main__":
    main()
