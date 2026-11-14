#!/usr/bin/env python3
"""Package the release Linux app as a local, unsigned development tarball and verify it relocated."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

from licenses import write_notices
from package_core import APP_ROOT, REPO_ROOT, TARGET, build_environment, cargo_homes, prepare_core

ARTIFACTS = REPO_ROOT / "build" / "linux" / "artifacts"
TARGET_DIR = REPO_ROOT / "build" / "linux" / "target"
APP_ID = "io.github.awesomejt.markdown"
SMOKES = {
    "--smoke-test": "shell-smoke: pass (application",
    "--smoke-test=last-window": "shell-smoke: pass (closing the last window",
}


def app_version() -> str:
    """Read the package version from apps/linux/Cargo.toml."""
    cargo_toml = tomllib.loads((APP_ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return cargo_toml["package"]["version"]


def build_release(*, prepared: bool = False) -> Path:
    """Build the release Linux workspace with path remapping and return the binary path."""
    if not prepared:
        prepare_core("release")
    env = build_environment()
    command = [
        "cargo",
        "build",
        "--workspace",
        "--release",
        "--locked",
        "--target-dir",
        str(TARGET_DIR),
    ]
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=APP_ROOT, env=env, check=True)
    return TARGET_DIR / "release" / "markdown-linux"


def build_info(core_library: Path) -> dict:
    """Record toolchain and source metadata without embedding local filesystem paths."""

    def output(*cmd: str) -> str:
        return subprocess.check_output(
            cmd, cwd=REPO_ROOT, text=True, stderr=subprocess.DEVNULL
        ).strip()

    commit = output("git", "rev-parse", "HEAD")
    dirty = bool(output("git", "status", "--porcelain"))
    rustc = output("rustc", "--version")
    gtk4_version = output("pkg-config", "--modversion", "gtk4")
    return {
        "configuration": "release",
        "source_commit": commit,
        "source_dirty": dirty,
        "target": TARGET,
        "rustc": rustc,
        "gtk4": gtk4_version,
        "core_library_sha256": hashlib.sha256(core_library.read_bytes()).hexdigest(),
        "signed": False,
    }


def package(binary: Path) -> Path:
    """Stage the stripped release binary, core library, desktop entry, and licenses into a .tar.gz."""
    if not binary.is_file():
        raise SystemExit(f"missing release binary: {binary}")
    core_package = REPO_ROOT / "build" / "linux" / "core-package" / "release"
    core_library = core_package / "lib" / "libmarkdown_ffi.so"
    if not core_library.is_file():
        raise SystemExit(f"missing release core library: {core_library}")

    version = app_version()
    info = build_info(core_library)
    name = (
        f"Markdown-{version}-{info['source_commit'][:12]}"
        f"{'-dirty' if info['source_dirty'] else ''}-linux-x86_64"
    )
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    archive = ARTIFACTS / f"{name}.tar.gz"
    archive.unlink(missing_ok=True)

    with tempfile.TemporaryDirectory(prefix="markdown-package-") as directory:
        bundle = Path(directory) / name
        (bundle / "bin").mkdir(parents=True)
        (bundle / "lib").mkdir()
        (bundle / "share" / "applications").mkdir(parents=True)

        staged_bin = bundle / "bin" / "markdown-linux"
        staged_lib = bundle / "lib" / "libmarkdown_ffi.so"
        shutil.copy2(binary, staged_bin)
        shutil.copy2(core_library, staged_lib)
        subprocess.run(["strip", "--strip-unneeded", str(staged_bin)], check=True)
        subprocess.run(["strip", "--strip-unneeded", str(staged_lib)], check=True)
        os.chmod(staged_bin, 0o755)
        os.chmod(staged_lib, 0o755)

        template = (APP_ROOT / "resources" / f"{APP_ID}.desktop.in").read_text(encoding="utf-8")
        desktop_file = bundle / "share" / "applications" / f"{APP_ID}.desktop"
        desktop_file.write_text(
            template.replace("@EXEC@", "markdown-linux"),
            encoding="utf-8",
        )

        write_notices(bundle)
        # Recompute core SHA-256 after stripping so BuildInfo.json matches the packaged .so.
        info["core_library_sha256"] = hashlib.sha256(staged_lib.read_bytes()).hexdigest()
        (bundle / "BuildInfo.json").write_text(
            json.dumps(info, indent=2) + "\n", encoding="utf-8"
        )

        with tarfile.open(archive, "w:gz") as tar:
            tar.add(bundle, arcname=name)

    archive_bytes = archive.read_bytes()
    record = dict(
        info,
        artifact=archive.name,
        bytes=len(archive_bytes),
        sha256=hashlib.sha256(archive_bytes).hexdigest(),
        bundle_version=version,
    )
    (ARTIFACTS / f"{name}.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return archive


def private_prefixes() -> list[str]:
    roots = [REPO_ROOT, Path.home(), *(Path(home) for home in cargo_homes())]
    results: list[str] = []
    for root in roots:
        results.append(str(root))
        try:
            results.append(str(root.resolve()))
        except OSError:
            pass
    return sorted({s for s in results if s})


def _smoke_env() -> dict[str, str]:
    env = os.environ.copy()
    env["GDK_DEBUG"] = ",".join(filter(None, [env.get("GDK_DEBUG"), "no-portals"]))
    env["GSETTINGS_BACKEND"] = "memory"
    return env


def _run_smoke(executable: Path, cwd: Path, flag: str, marker: str) -> None:
    env = _smoke_env()
    with tempfile.TemporaryDirectory(prefix="markdown-relocated-smoke-") as scratch:
        for name in ("DATA", "CONFIG", "CACHE"):
            directory = Path(scratch) / name.lower()
            directory.mkdir()
            env[f"XDG_{name}_HOME"] = str(directory)
        try:
            result = subprocess.run(
                [str(executable), flag],
                cwd=cwd,
                env=env,
                text=True,
                capture_output=True,
                timeout=30,
            )
        except subprocess.TimeoutExpired as error:
            for output in (error.stdout, error.stderr):
                if output:
                    print(
                        output.decode(errors="replace") if isinstance(output, bytes) else output,
                        end="",
                        flush=True,
                    )
            raise
    print(result.stdout, end="", flush=True)
    if result.stderr:
        print(result.stderr, end="", flush=True)
    result.check_returncode()
    if marker not in result.stdout or "shell-smoke: event loop exited" not in result.stdout:
        raise SystemExit(f"{executable} {flag}: exited without completing its smoke")


def _run_relaunch(executable: Path, cwd: Path) -> None:
    env = _smoke_env()
    with tempfile.TemporaryDirectory(prefix="markdown-relocated-relaunch-") as directory:
        relaunch_doc = Path(directory) / "live.md"
        sections = "\n\n".join(
            f"## Part {part}\n\nProse for part {part}, with a *marked* word."
            for part in range(120)
        )
        relaunch_doc.write_text(f"# Live\n\n{sections}\n", encoding="utf-8")
        env["MARKDOWN_RESTORATION_PATH"] = str(Path(directory) / "restoration.json")
        for phase, marker in (
            ("prepare", "relaunch-check: prepared"),
            ("verify", "relaunch-check: pass (restored window, document, top line and selection)"),
        ):
            result = subprocess.run(
                [str(executable), "--relaunch-check", phase, str(relaunch_doc)],
                cwd=cwd,
                env=env,
                text=True,
                capture_output=True,
                timeout=30,
            )
            print(result.stdout, end="", flush=True)
            if result.returncode != 0 or marker not in result.stdout:
                print(result.stderr, end="", flush=True)
                raise SystemExit(f"{executable.name}: relocated relaunch check {phase} failed")


def verify(archive: Path) -> None:
    """Extract the tarball outside the checkout and verify ELF, RPATH, licenses, and launches."""
    with tempfile.TemporaryDirectory(prefix="markdown-artifact-") as directory:
        root = Path(directory).resolve()
        extracted = root / "extracted"
        extracted.mkdir()
        with tarfile.open(archive, "r:gz") as tar:
            tar.extractall(extracted, filter="data")

        entries = list(extracted.iterdir())
        if len(entries) != 1 or not entries[0].is_dir():
            raise SystemExit("the artifact must hold exactly one top-level directory")
        bundle = entries[0]
        executable = bundle / "bin" / "markdown-linux"
        core_lib = bundle / "lib" / "libmarkdown_ffi.so"
        desktop_file = bundle / "share" / "applications" / f"{APP_ID}.desktop"

        for required in (executable, core_lib, desktop_file, bundle / "BuildInfo.json"):
            if not required.is_file():
                raise SystemExit(f"missing artifact file: {required.relative_to(bundle)}")
        if not os.access(executable, os.X_OK):
            raise SystemExit("the packaged executable lost its execute permission")

        # Verify ELF64 x86-64 architecture for both binary and shared library.
        for elf in (executable, core_lib):
            header = subprocess.check_output(["readelf", "-h", str(elf)], text=True)
            if "ELF64" not in header or "Advanced Micro Devices X86-64" not in header:
                raise SystemExit(f"unexpected ELF architecture for {elf.name}")

        # Verify exact 24 C ABI exports on libmarkdown_ffi.so.
        header_text = (REPO_ROOT / "core" / "include" / "markdown_core.h").read_text()
        expected_exports = set(re.findall(r"\b(md_\w+)\(", header_text))
        symbols = subprocess.check_output(
            ["nm", "-D", "--defined-only", str(core_lib)], text=True
        )
        exported = {line.split()[-1] for line in symbols.splitlines() if line.strip()}
        if exported != expected_exports:
            raise SystemExit(
                f"C ABI symbol mismatch: missing={sorted(expected_exports - exported)}, "
                f"extra={sorted(exported - expected_exports)}"
            )

        # Verify RUNPATH uses only $ORIGIN-relative entries and ldd resolves the bundled .so.
        dynamic = subprocess.check_output(["readelf", "-d", str(executable)], text=True)
        rpath_match = re.search(r"\((?:RUNPATH|RPATH)\)\s+Library runpath:\s+\[([^\]]+)\]", dynamic)
        if not rpath_match:
            raise SystemExit("executable is missing RUNPATH")
        for entry in rpath_match.group(1).split(":"):
            if not entry.startswith("$ORIGIN"):
                raise SystemExit(f"non-relative RUNPATH entry: {entry}")

        ldd_out = subprocess.check_output(["ldd", str(executable)], text=True)
        resolved_core = core_lib.resolve()
        ldd_match = re.search(r"libmarkdown_ffi\.so\s+=>\s+(\S+)", ldd_out)
        if not ldd_match or Path(ldd_match.group(1)).resolve() != resolved_core:
            raise SystemExit(
                f"ldd did not resolve libmarkdown_ffi.so to bundled {resolved_core}:\n{ldd_out}"
            )
        if str(REPO_ROOT) in ldd_out:
            raise SystemExit(f"ldd resolved a dependency inside the checkout:\n{ldd_out}")

        # No build-machine paths anywhere in the packaged bundle.
        prefixes = [prefix.encode() for prefix in private_prefixes()]
        for path in bundle.rglob("*"):
            if path.is_file():
                data = path.read_bytes()
                for prefix in prefixes:
                    if prefix in data:
                        raise SystemExit(
                            f"{path.relative_to(bundle)} names a build-machine path ({prefix.decode(errors='replace')})"
                        )

        info = json.loads((bundle / "BuildInfo.json").read_text(encoding="utf-8"))
        if info["configuration"] != "release" or info["signed"] is not False:
            raise SystemExit("unexpected BuildInfo.json metadata")
        if info["core_library_sha256"] != hashlib.sha256(core_lib.read_bytes()).hexdigest():
            raise SystemExit("BuildInfo.json core_library_sha256 mismatch")

        if shutil.which("desktop-file-validate"):
            subprocess.run(["desktop-file-validate", str(desktop_file)], check=True)

        # Verify license inventory and notices.
        licenses_dir = bundle / "Licenses"
        if (licenses_dir / "LICENSE").read_bytes() != (REPO_ROOT / "LICENSE").read_bytes():
            raise SystemExit("the bundled LICENSE differs from the repository's")
        inventory = json.loads((licenses_dir / "inventory.json").read_text(encoding="utf-8"))
        notices = (licenses_dir / "THIRD-PARTY-NOTICES.txt").read_text(encoding="utf-8")
        names = {pkg["name"] for pkg in inventory["packages"]}
        if not {"markdown-core", "markdown-ffi", "pulldown-cmark", "markdown-linux", "gtk4"} <= names:
            raise SystemExit("the inventory is missing expected core or app packages")
        for pkg in inventory["packages"]:
            heading = (
                f"\n{pkg['name']} {pkg['version']}\n"
                f"License: {pkg['license']}\n"
                f"Components: {', '.join(pkg['components'])}\n"
            )
            if heading not in notices:
                raise SystemExit(f"no notice for {pkg['name']} {pkg['version']}")
            if pkg["source"] != "workspace" and not pkg["files"]:
                raise SystemExit(f"no license text for {pkg['name']}")
            for filename in pkg["files"]:
                if f"----- {filename} -----" not in notices:
                    raise SystemExit(f"{pkg['name']}: {filename} missing from THIRD-PARTY-NOTICES.txt")

        # Launch from the extracted directory outside the checkout.
        for flag, marker in SMOKES.items():
            _run_smoke(executable, root, flag, marker)
        subprocess.run(
            [sys.executable, str(APP_ROOT / "tools" / "check_association.py"), str(executable)],
            cwd=root,
            check=True,
        )
        _run_relaunch(executable, root)
        print(
            f"artifact: {archive.name} verified relocated ({len(inventory['packages'])} "
            "linked packages inventoried; smoke, association, and relaunch passed)",
            flush=True,
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--no-verify",
        action="store_true",
        help="package without the relocated launch checks",
    )
    arguments = parser.parse_args()
    prepare_core("release")
    archive = package(build_release(prepared=True))
    if not arguments.no_verify:
        verify(archive)
    print(archive)
