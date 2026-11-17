#!/usr/bin/env python3
"""Package the release Windows app as a local, unsigned development zip and verify it relocated."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
import zipfile

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT / "apps/windows/tools"))
from check_association import check_association  # noqa: E402
from licenses import BUILD_ONLY_NUGET, write_notices  # noqa: E402
from package_core import cargo_homes, header_exports, pe_exports, rustc_sysroots  # noqa: E402

ARTIFACTS = REPO_ROOT / "build/windows/artifacts"


def run_app(command: list[str], *, cwd: Path, env: dict[str, str], timeout: int) -> str:
    print("+ " + " ".join(command), flush=True)
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as error:
        if error.stdout:
            output = error.stdout.decode(errors="replace") if isinstance(error.stdout, bytes) else error.stdout
            print(output, end="", flush=True)
        raise
    if result.stdout:
        print(result.stdout, end="", flush=True)
    result.check_returncode()
    return result.stdout


def app_version() -> str:
    """Read the package version from the single source of truth in Markdown.Windows.csproj."""
    csproj = REPO_ROOT / "apps/windows/Markdown.Windows.csproj"
    match = re.search(r"<Version>([^<]+)</Version>", csproj.read_text(encoding="utf-8"))
    if not match:
        raise SystemExit(f"missing <Version> in {csproj}")
    return match.group(1).strip()


def build_info(configuration: str = "release") -> dict:
    """What built this Windows package: versions and hashes only, never a path."""
    core_dll = REPO_ROOT / f"build/windows/{configuration.capitalize()}/markdown_ffi.dll"
    if not core_dll.is_file():
        core_dll = REPO_ROOT / f"build/windows/core-package/{configuration}/bin/markdown_ffi.dll"

    def output(*cmd: str) -> str:
        return subprocess.check_output(cmd, cwd=REPO_ROOT, text=True, stderr=subprocess.DEVNULL).strip()

    commit = output("git", "rev-parse", "HEAD")
    dirty = bool(output("git", "status", "--porcelain"))
    rustc = output("rustc", "--version")
    dotnet = output("dotnet", "--version")

    core_sha = ""
    if core_dll.is_file():
        core_sha = hashlib.sha256(core_dll.read_bytes()).hexdigest()

    return {
        "configuration": configuration,
        "source_commit": commit,
        "source_dirty": dirty,
        "target": "x86_64-pc-windows-msvc",
        "minimum_windows": "10.0.22000.0",
        "rustc": rustc,
        "dotnet": dotnet,
        "windows_app_sdk": "2.5.1",
        "core_library_sha256": core_sha,
        "signed": False,
    }


def prepare_release_metadata(release_dir: Path) -> dict:
    """Ensure Licenses and BuildInfo.json are staged in release_dir."""
    write_notices(release_dir)
    info = build_info("release")
    (release_dir / "BuildInfo.json").write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    return info


def package(release_dir: Path) -> Path:
    """Zip the release directory and record its hash record beside it."""
    info_file = release_dir / "BuildInfo.json"
    if not info_file.is_file():
        info = prepare_release_metadata(release_dir)
    else:
        info = json.loads(info_file.read_text(encoding="utf-8"))

    if info["configuration"] != "release":
        raise SystemExit("the development artifact must be the release build")

    for script in ("install.ps1", "uninstall.ps1"):
        (release_dir / script).write_bytes((REPO_ROOT / "apps/windows/resources" / script).read_bytes())
    version = app_version()
    name = (
        f"Markdown-{version}-{info['source_commit'][:12]}"
        f"{'-dirty' if info['source_dirty'] else ''}-windows-x64"
    )
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    archive = ARTIFACTS / f"{name}.zip"
    archive.unlink(missing_ok=True)

    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for root, _dirs, files in os.walk(release_dir):
            for file in files:
                source_path = Path(root) / file
                # Do not package PDB symbol files in release artifacts
                if source_path.suffix.lower() == ".pdb":
                    continue
                rel_path = source_path.relative_to(release_dir)
                archive_name = f"Markdown/{rel_path.as_posix()}"
                zf.write(source_path, archive_name)

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
    roots = [REPO_ROOT, Path.home(), *cargo_homes(), *rustc_sysroots()]
    if user_prof := os.environ.get("USERPROFILE"):
        roots.append(Path(user_prof))
    if local_app := os.environ.get("LOCALAPPDATA"):
        roots.append(Path(local_app))
    results: list[str] = []
    for r in roots:
        results.append(str(r))
        results.append(r.as_posix())
        try:
            resolved = r.resolve()
            results.append(str(resolved))
            results.append(resolved.as_posix())
        except (OSError, RuntimeError):
            pass
    # Unique non-empty strings
    return sorted(set(s for s in results if s))


def verify_pe_architecture(binary: Path) -> None:
    data = binary.read_bytes()
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise SystemExit(f"{binary} is not a valid PE binary (missing MZ header)")
    (pe_offset,) = struct.unpack_from("<I", data, 0x3C)
    if pe_offset + 24 > len(data) or data[pe_offset : pe_offset + 4] != b"PE\x00\x00":
        raise SystemExit(f"{binary} is not a valid PE binary (missing PE signature)")

    coff_offset = pe_offset + 4
    machine, _num_sections, _time, _sym_ptr, _num_syms, opt_size, _chars = struct.unpack_from(
        "<HHIIIHH", data, coff_offset
    )
    if machine != 0x8664:  # IMAGE_FILE_MACHINE_AMD64
        raise SystemExit(f"{binary} unexpected PE machine type: 0x{machine:x} (expected x64 0x8664)")

    opt_offset = coff_offset + 20
    if opt_size < 120 or opt_offset + opt_size > len(data):
        raise SystemExit(f"{binary} has truncated PE optional header")
    (magic,) = struct.unpack_from("<H", data, opt_offset)
    if magic != 0x20B:  # PE32+ (64-bit)
        raise SystemExit(f"{binary} is not a PE32+ (64-bit) image: magic=0x{magic:x}")

    # Check that development artifact is unsigned (no certificate in security directory)
    (num_rva_and_sizes,) = struct.unpack_from("<I", data, opt_offset + 108)
    if num_rva_and_sizes >= 5:  # Directory 4 is IMAGE_DIRECTORY_ENTRY_SECURITY
        cert_rva, cert_size = struct.unpack_from("<II", data, opt_offset + 112 + 4 * 8)
        if cert_rva != 0 or cert_size != 0:
            raise SystemExit(f"{binary} has embedded security certificate (must be unsigned)")


def verify(archive: Path, *, env: dict[str, str]) -> None:
    """Extract the zip outside the checkout and check what a copy to another folder needs."""
    with tempfile.TemporaryDirectory(prefix="markdown-windows-artifact-") as directory:
        root = Path(directory).resolve()
        extracted = root / "extracted"
        with zipfile.ZipFile(archive, "r") as zf:
            zf.extractall(extracted)

        children = [p.name for p in extracted.iterdir()]
        if children != ["Markdown"]:
            raise SystemExit(f"the artifact must hold exactly Markdown, found: {children}")

        package_dir = extracted / "Markdown"
        # Reproduce a parent shell's incompatible/missing module search path.
        # Install scripts must load the utility module belonging to their own PSHOME.
        script_env = dict(env)
        script_env["PSModulePath"] = str(root / "restricted-modules")
        installed = root / "installed app"
        install = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                   str(package_dir / "install.ps1"), "-Destination", str(installed) + os.sep]
        run_app(install, cwd=root, env=script_env, timeout=60)
        # Existing destinations are never overwritten, even for an apparent upgrade.
        retry = subprocess.run(install, capture_output=True, text=True, timeout=60, env=script_env)
        if retry.returncode == 0:
            raise SystemExit("installer overwrote existing installation")
        # PowerShell Set-Location must determine relative installation destinations.
        relative_parent = root / "relative location"
        relative_parent.mkdir()
        relative_check = root / "relative-check.ps1"
        relative_check.write_text(
            "param([string]$Installer,[string]$Work)\n"
            "$ErrorActionPreference='Stop'\nSet-Location -LiteralPath $Work\n"
            "& $Installer -Destination 'relative app\\'\n", encoding="utf-8")
        subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                        str(relative_check), "-Installer", str(package_dir / "install.ps1"), "-Work", str(relative_parent)],
                       cwd=root, check=True, capture_output=False, text=True, timeout=60, env=script_env)
        relative_install = relative_parent / "relative app"
        if not (relative_install / "Markdown.Windows.exe").is_file():
            raise SystemExit("relative destination ignored PowerShell location")
        subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                        str(relative_install / "uninstall.ps1")], check=True, capture_output=False, text=True, timeout=60, env=script_env)
        if relative_install.exists():
            raise SystemExit("trailing-separator relative installation could not be uninstalled")

        failure_check = root / "failure-check.ps1"
        failure_check.write_text(
            "param([string]$Installer,[string]$Target)\n"
            "$ErrorActionPreference='Stop'\n$script:copies=0\n"
            "function Copy-Item { param([string]$LiteralPath,[string]$Destination)\n"
            " $script:copies++; if ($script:copies -eq 2) { throw 'synthetic copy failure' }\n"
            " Microsoft.PowerShell.Management\\Copy-Item -LiteralPath $LiteralPath -Destination $Destination\n}\n"
            "try { & $Installer -Destination $Target; exit 2 } catch { if ($_.Exception.Message -ne 'synthetic copy failure') { throw } }\n",
            encoding="utf-8")
        failed_target = root / "failed install"
        subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                        str(failure_check), "-Installer", str(package_dir / "install.ps1"), "-Target", str(failed_target)],
                       check=True, capture_output=False, text=True, timeout=60, env=script_env)
        if failed_target.exists() or list(root.glob(".markdown-install-*")):
            raise SystemExit("failed installation left its target or staging directory")
        app_dir = installed
        exe = app_dir / "Markdown.Windows.exe"
        core_dll = app_dir / "markdown_ffi.dll"
        if not exe.is_file():
            raise SystemExit("artifact missing Markdown.Windows.exe")
        if not core_dll.is_file():
            raise SystemExit("artifact missing markdown_ffi.dll")

        # Verify PE headers, architecture (x64) and unsigned status
        verify_pe_architecture(exe)
        verify_pe_architecture(core_dll)

        # Verify core DLL exports exact 24 symbols
        expected_exports = header_exports()
        actual_exports = pe_exports(core_dll)
        if actual_exports != expected_exports:
            raise SystemExit(f"unexpected exports in artifact core DLL: {actual_exports}")

        # Verify BuildInfo.json
        build_info_file = app_dir / "BuildInfo.json"
        if not build_info_file.is_file():
            raise SystemExit("artifact missing BuildInfo.json")
        info = json.loads(build_info_file.read_text(encoding="utf-8"))
        if info.get("configuration") != "release":
            raise SystemExit("artifact BuildInfo.json configuration != release")
        if info.get("signed") is not False:
            raise SystemExit("artifact BuildInfo.json signed must be false")
        if info.get("target") != "x86_64-pc-windows-msvc":
            raise SystemExit(f"artifact BuildInfo.json target unexpected: {info.get('target')}")

        # Verify licenses and notices
        licenses_dir = app_dir / "Licenses"
        if not (licenses_dir / "LICENSE").is_file():
            raise SystemExit("artifact missing Licenses/LICENSE")
        if (licenses_dir / "LICENSE").read_bytes() != (REPO_ROOT / "LICENSE").read_bytes():
            raise SystemExit("artifact LICENSE differs from repository LICENSE")

        inventory_file = licenses_dir / "inventory.json"
        if not inventory_file.is_file():
            raise SystemExit("artifact missing Licenses/inventory.json")
        inventory = json.loads(inventory_file.read_text(encoding="utf-8"))
        notices = (licenses_dir / "THIRD-PARTY-NOTICES.txt").read_text(encoding="utf-8")

        rust_names = {pkg["name"] for pkg in inventory["rust_packages"]}
        if not {"markdown-core", "markdown-ffi", "pulldown-cmark"} <= rust_names:
            raise SystemExit("the inventory is missing core Rust packages")
        for pkg in inventory["rust_packages"]:
            heading = f"\n{pkg['name']} {pkg['version']}\nLicense: {pkg['license']}\n"
            if heading not in notices:
                raise SystemExit(f"no notice for Rust package {pkg['name']} {pkg['version']}")
            if pkg["source"] != "workspace":
                if not pkg["files"]:
                    raise SystemExit(f"no license text recorded for Rust package {pkg['name']}")
                for file_name in pkg["files"]:
                    if f"----- {file_name} -----" not in notices:
                        raise SystemExit(f"{pkg['name']}: {file_name} missing from notices")

        nuget_names = {pkg["name"] for pkg in inventory["nuget_packages"]}
        if not {"Microsoft.WindowsAppSDK", "Microsoft.Web.WebView2"} <= nuget_names:
            raise SystemExit("the inventory is missing core NuGet packages")
        for pkg in inventory["nuget_packages"]:
            heading = f"\n{pkg['name']} {pkg['version']}\nLicense: {pkg['license']}\n"
            if heading not in notices:
                raise SystemExit(f"no notice for NuGet package {pkg['name']} {pkg['version']}")
            if pkg["name"] not in BUILD_ONLY_NUGET:
                if not pkg["files"]:
                    raise SystemExit(f"no license text recorded for NuGet package {pkg['name']}")
                for file_name in pkg["files"]:
                    if f"----- {file_name} -----" not in notices:
                        raise SystemExit(f"{pkg['name']}: {file_name} missing from notices")

        # Verify no build-machine paths leaked into any file in the artifact
        prefixes = private_prefixes()
        needles = []
        for p in prefixes:
            needles.append(p.lower().encode("utf-8"))
            needles.append(p.lower().encode("utf-16-le"))

        for file_path in app_dir.rglob("*"):
            if file_path.is_file():
                content = file_path.read_bytes().lower()
                for needle in needles:
                    if needle in content:
                        raise SystemExit(f"{file_path.relative_to(extracted)} names a build-machine path")

        version = run_app([str(exe), "--version"], cwd=app_dir, env=env, timeout=30)
        if "Semantic model v2" not in version or "ABI v1" not in version:
            raise SystemExit("installed diagnostics missing semantic/ABI versions")

        # Launch relocated checks from installed directory
        run_app([str(exe), "--document-tests"], cwd=app_dir, env=env, timeout=120)
        run_app([str(exe), "--render-tests"], cwd=app_dir, env=env, timeout=120)
        run_app([str(exe), "--smoke"], cwd=app_dir, env=env, timeout=60)
        check_association(exe)

        # Relocated relaunch test
        relaunch_doc = root / "live.md"
        sections = "\n\n".join(
            f"## Part {part}\n\nProse for part {part}, with a *marked* word." for part in range(120)
        )
        relaunch_doc.write_text(f"# Live\n\n{sections}\n", encoding="utf-8")
        preferences_store_path = root / "preferences.json"
        preferences_store_path.write_text(
            json.dumps({"theme": 2, "textScale": 1.32}, indent=2) + "\n",
            encoding="utf-8",
        )
        relaunch_env = dict(env)
        relaunch_env["MARKDOWN_RESTORATION_PATH"] = str(root / "restoration.json")
        relaunch_env["MARKDOWN_PREFERENCES_PATH"] = str(preferences_store_path)
        for phase, marker in (
            ("prepare", "relaunch-check: prepared"),
            ("verify", "relaunch-check: pass (restored window, document, top line and selection)"),
        ):
            out = run_app(
                [str(exe), "--relaunch-check", phase, str(relaunch_doc)],
                cwd=root,
                env=relaunch_env,
                timeout=30,
            )
            if marker not in out:
                raise SystemExit(f"artifact relaunch check {phase} failed to report marker")

        run_app([str(exe), "--core-probe", "single"], cwd=app_dir, env=env, timeout=60)

        uninstall = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                     str(installed / "uninstall.ps1")]
        # No removals occur when an owned file was modified.
        original_info = build_info_file.read_bytes()
        build_info_file.write_bytes(original_info + b"\n")
        changed = subprocess.run(uninstall, capture_output=True, text=True, timeout=60, env=script_env)
        if changed.returncode == 0 or not exe.is_file():
            raise SystemExit("uninstall removed files after a hash mismatch")
        build_info_file.write_bytes(original_info)
        # A tampered marker must never reach an absolute target outside the install root.
        marker = installed / ".markdown-dev-install.json"
        original_marker = marker.read_bytes()
        record = json.loads(original_marker.decode("utf-8-sig"))
        outside = root / "outside.txt"
        outside.write_text("preserve", encoding="utf-8")
        record["files"].insert(0, {"path": "../outside.txt", "sha256": hashlib.sha256(outside.read_bytes()).hexdigest()})
        marker.write_text(json.dumps(record), encoding="utf-8")
        escaped = subprocess.run(uninstall, capture_output=True, text=True, timeout=60, env=script_env)
        if escaped.returncode == 0 or not exe.is_file() or outside.read_text(encoding="utf-8") != "preserve":
            raise SystemExit("uninstall accepted an escaping manifest path")
        marker.write_bytes(original_marker)
        sentinel = installed / "keep-user-file.txt"
        sentinel.write_text("keep", encoding="utf-8")
        run_app(uninstall, cwd=root, env=script_env, timeout=60)
        if exe.exists() or sentinel.read_text(encoding="utf-8") != "keep":
            raise SystemExit("uninstall did not remove app while preserving unowned file")
        if (installed / ".markdown-dev-install.json").exists():
            raise SystemExit("uninstall retained install marker")

        print(
            f"artifact: {archive.name} verified relocated ({len(inventory['rust_packages'])} Rust packages, "
            f"{len(inventory['nuget_packages'])} NuGet packages inventoried, Native AOT relocated tests, "
            f"smoke, association, relaunch, and probe passed)",
            flush=True,
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--no-verify", action="store_true", help="package without the relocated launch checks"
    )
    args = parser.parse_args()

    release_dir = REPO_ROOT / "build/windows/Release"
    if not (release_dir / "Markdown.Windows.exe").is_file():
        raise SystemExit(f"Release build not found in {release_dir}; run check.py first")

    prepare_release_metadata(release_dir)
    archive = package(release_dir)
    env = dict(os.environ)
    installer = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
    env["PATH"] = str(installer / "Microsoft Visual Studio/Installer") + os.pathsep + env.get("PATH", "")

    if not args.no_verify:
        verify(archive, env=env)
    print(archive)


if __name__ == "__main__":
    main()
