#!/usr/bin/env python3
"""Build and verify the local Windows shared-core C ABI package."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

REPO_ROOT = Path(__file__).resolve().parents[3]
HEADER = REPO_ROOT / "core/include/markdown_core.h"
CARGO_LOCK = REPO_ROOT / "Cargo.lock"
TOOLCHAIN = REPO_ROOT / "rust-toolchain.toml"
LICENSE = REPO_ROOT / "LICENSE"
RUST_TARGET_DIR = REPO_ROOT / "build/windows/rust"
PACKAGE_ROOT = REPO_ROOT / "build/windows/core-package"
TARGET_TRIPLE = "x86_64-pc-windows-msvc"
CONFIGURATIONS = ("debug", "release")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_rustc() -> str:
    match = re.search(r'^channel\s*=\s*"([^"]+)"', TOOLCHAIN.read_text(encoding="utf-8"), re.M)
    if not match:
        raise SystemExit(f"missing toolchain channel in {TOOLCHAIN}")
    return match.group(1)


def check_toolchain() -> str:
    version = subprocess.check_output(["rustc", "--version"], cwd=REPO_ROOT, text=True).strip()
    expected = expected_rustc()
    if f"rustc {expected} " not in version:
        raise SystemExit(f"expected rustc {expected} from {TOOLCHAIN}, got {version}")
    return version


def header_exports() -> list[str]:
    text = re.sub(r"/\*.*?\*/", "", HEADER.read_text(encoding="utf-8"), flags=re.S)
    names = sorted(set(re.findall(r"\b(md_[a-z0-9_]+)\s*\(", text)))
    if not names:
        raise SystemExit(f"no md_* function declarations found in {HEADER}")
    return names


def pe_exports(library: Path) -> list[str]:
    data = library.read_bytes()
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise SystemExit(f"{library} is not a valid PE binary (missing MZ header)")
    (pe_offset,) = struct.unpack_from("<I", data, 0x3C)
    if pe_offset + 24 > len(data) or data[pe_offset : pe_offset + 4] != b"PE\x00\x00":
        raise SystemExit(f"{library} is not a valid PE binary (missing PE signature)")

    coff_offset = pe_offset + 4
    _machine, num_sections, _time, _sym_ptr, _num_syms, opt_size, _chars = struct.unpack_from(
        "<HHIIIHH", data, coff_offset
    )
    opt_offset = coff_offset + 20
    if opt_size < 120 or opt_offset + opt_size > len(data):
        raise SystemExit(f"{library} has truncated PE optional header")
    (magic,) = struct.unpack_from("<H", data, opt_offset)
    if magic != 0x20B:
        raise SystemExit(f"{library} is not a PE32+ (64-bit) image: magic=0x{magic:x}")
    (num_rva_and_sizes,) = struct.unpack_from("<I", data, opt_offset + 108)
    if num_rva_and_sizes < 1:
        return []
    export_rva, export_size = struct.unpack_from("<II", data, opt_offset + 112)
    if export_rva == 0 or export_size == 0:
        return []

    sec_table = opt_offset + opt_size
    sections: list[tuple[int, int, int, int]] = []
    for idx in range(num_sections):
        sec_offset = sec_table + idx * 40
        if sec_offset + 40 > len(data):
            raise SystemExit(f"{library} has truncated section table")
        vsize, va, rsize, raw_ptr = struct.unpack_from("<IIII", data, sec_offset + 8)
        sections.append((va, max(vsize, rsize), raw_ptr, rsize))

    def rva_to_offset(rva: int) -> int:
        for va, span, raw_ptr, rsize in sections:
            if va <= rva < va + span:
                delta = rva - va
                if delta >= rsize:
                    raise SystemExit(f"RVA 0x{rva:x} falls into uninitialized section data")
                return raw_ptr + delta
        raise SystemExit(f"unable to map RVA 0x{rva:x} to file offset in {library}")

    export_dir = rva_to_offset(export_rva)
    if export_dir + 40 > len(data):
        raise SystemExit(f"{library} has truncated export directory")
    num_names, _addr_funcs, addr_names, _addr_ordinals = struct.unpack_from(
        "<IIII", data, export_dir + 24
    )
    names_table = rva_to_offset(addr_names)
    exports: list[str] = []
    for idx in range(num_names):
        (name_rva,) = struct.unpack_from("<I", data, names_table + idx * 4)
        name_offset = rva_to_offset(name_rva)
        end = data.find(b"\x00", name_offset)
        if end < 0:
            raise SystemExit(f"unterminated export symbol name at offset {name_offset}")
        exports.append(data[name_offset:end].decode("ascii"))
    return sorted(set(exports))


def verify_exports(library: Path, expected: list[str]) -> list[str]:
    exports = pe_exports(library)
    if exports != expected:
        missing = sorted(set(expected) - set(exports))
        extra = sorted(set(exports) - set(expected))
        raise SystemExit(f"unexpected exports in {library}: missing={missing} extra={extra}")
    return exports


def cargo_homes() -> list[Path]:
    homes = [Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))]
    resolved = [home.resolve() for home in homes if home.exists()]
    return list(dict.fromkeys([*homes, *resolved]))


def rustc_sysroots() -> list[Path]:
    out = subprocess.check_output(["rustc", "--print", "sysroot"], cwd=REPO_ROOT, text=True).strip()
    if not out:
        return []
    sysroot = Path(out)
    paths = [sysroot]
    if sysroot.exists():
        paths.append(sysroot.resolve())
    return list(dict.fromkeys(paths))


def check_no_build_paths(library: Path) -> None:
    data = library.read_bytes()
    forbidden = [str(REPO_ROOT).encode()] + [str(home).encode() for home in cargo_homes()]
    for needle in forbidden:
        if needle and needle in data:
            raise SystemExit(f"build path {needle.decode(errors='replace')} leaked into {library}")


def git_commit() -> str:
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True).strip()
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def package_dir(configuration: str) -> Path:
    return PACKAGE_ROOT / configuration


def write_package_identity_cs(path: Path, configuration: str, digest: str) -> None:
    path.write_text(
        "// Generated by apps/windows/tools/package_core.py; do not edit.\n"
        "namespace Markdown.Windows.Core;\n\n"
        "internal static class CorePackageIdentity\n"
        "{\n"
        f'    internal const string Configuration = "{configuration}";\n'
        f'    internal const string Fingerprint = "{digest}";\n'
        "}\n",
        encoding="utf-8",
        newline="\n",
    )


def prepare_core(configuration: str) -> Path:
    if configuration not in CONFIGURATIONS:
        raise ValueError(f"unsupported configuration: {configuration}")
    rustc = check_toolchain()
    env = os.environ.copy()
    flags = [
        "-C",
        "panic=unwind",
        "-C",
        "link-arg=/PDBALTPATH:markdown_ffi.pdb",
        f"--remap-path-prefix={REPO_ROOT}=.",
    ]
    for home in cargo_homes():
        flags.append(f"--remap-path-prefix={home}=/cargo")
    for sysroot in rustc_sysroots():
        flags.append(f"--remap-path-prefix={sysroot}=/rust")
    if existing := env.get("CARGO_ENCODED_RUSTFLAGS"):
        flags = existing.split("\x1f") + flags
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)

    command = [
        "cargo",
        "build",
        "--package",
        "markdown-ffi",
        "--lib",
        "--locked",
        "--features",
        "model-v2",
        "--target",
        TARGET_TRIPLE,
        "--target-dir",
        str(RUST_TARGET_DIR),
    ]
    if configuration == "release":
        command.append("--release")
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=REPO_ROOT, env=env, check=True)

    built_dir = RUST_TARGET_DIR / TARGET_TRIPLE / configuration
    built_dll = built_dir / "markdown_ffi.dll"
    built_lib = built_dir / "markdown_ffi.dll.lib"
    if not built_dll.is_file():
        raise SystemExit(f"missing built library {built_dll}")
    if not built_lib.is_file():
        raise SystemExit(f"missing built import library {built_lib}")

    expected = header_exports()
    exports = verify_exports(built_dll, expected)
    check_no_build_paths(built_dll)
    digest = sha256(built_dll)

    out = package_dir(configuration)
    stage = PACKAGE_ROOT / f".staging-{configuration}-{os.getpid()}"
    shutil.rmtree(stage, ignore_errors=True)
    (stage / "bin").mkdir(parents=True)
    (stage / "lib").mkdir(parents=True)
    (stage / "include").mkdir(parents=True)
    shutil.copy2(built_dll, stage / "bin/markdown_ffi.dll")
    shutil.copy2(built_lib, stage / "lib/markdown_ffi.dll.lib")
    shutil.copy2(HEADER, stage / "include/markdown_core.h")
    shutil.copy2(LICENSE, stage / "LICENSE")
    (stage / "identity").write_text(
        f"configuration={configuration}\nsha256={digest}\n", encoding="utf-8", newline="\n"
    )
    write_package_identity_cs(stage / "PackageIdentity.g.cs", configuration, digest)

    manifest = {
        "name": "MarkdownCore",
        "abi_version": 1,
        "models": [1, 2],
        "features": ["model-v2"],
        "panic": "unwind",
        "configuration": configuration,
        "target": TARGET_TRIPLE,
        "source_commit": git_commit(),
        "rustc": rustc,
        "header": "include/markdown_core.h",
        "header_sha256": sha256(stage / "include/markdown_core.h"),
        "cargo_lock_sha256": sha256(CARGO_LOCK),
        "import_library": {
            "path": "lib/markdown_ffi.dll.lib",
            "sha256": sha256(stage / "lib/markdown_ffi.dll.lib"),
            "bytes": (stage / "lib/markdown_ffi.dll.lib").stat().st_size,
        },
        "library": {
            "path": "bin/markdown_ffi.dll",
            "sha256": digest,
            "bytes": (stage / "bin/markdown_ffi.dll").stat().st_size,
            "exports": exports,
        },
    }
    (stage / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    shutil.rmtree(out, ignore_errors=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    stage.rename(out)
    verify(configuration)
    return out


def verify(configuration: str) -> dict[str, object]:
    out = package_dir(configuration)
    manifest_path = out / "manifest.json"
    if not manifest_path.is_file():
        raise SystemExit(f"missing core package manifest {manifest_path}")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    header = out / str(manifest["header"])
    library_info = manifest["library"]
    assert isinstance(library_info, dict)
    import_info = manifest["import_library"]
    assert isinstance(import_info, dict)
    library = out / str(library_info["path"])
    import_lib = out / str(import_info["path"])
    if not header.is_file() or not library.is_file() or not import_lib.is_file():
        raise SystemExit(f"incomplete core package in {out}")
    if sha256(header) != manifest["header_sha256"] or sha256(header) != sha256(HEADER):
        raise SystemExit("packaged markdown_core.h does not match core/include/markdown_core.h")
    if sha256(CARGO_LOCK) != manifest["cargo_lock_sha256"]:
        raise SystemExit("packaged core is stale relative to Cargo.lock")
    if sha256(library) != library_info["sha256"]:
        raise SystemExit(f"checksum mismatch for {library}")
    if sha256(import_lib) != import_info["sha256"]:
        raise SystemExit(f"checksum mismatch for {import_lib}")
    expected_identity = f"configuration={configuration}\nsha256={library_info['sha256']}\n"
    if (out / "identity").read_text(encoding="utf-8") != expected_identity:
        raise SystemExit(f"identity mismatch for {out / 'identity'}")
    identity_cs = (out / "PackageIdentity.g.cs").read_text(encoding="utf-8")
    if (
        f'Configuration = "{configuration}"' not in identity_cs
        or f'Fingerprint = "{library_info["sha256"]}"' not in identity_cs
    ):
        raise SystemExit(f"PackageIdentity.g.cs mismatch in {out}")
    if manifest["models"] != [1, 2] or manifest["features"] != ["model-v2"]:
        raise SystemExit("packaged core does not declare model-v2")
    if manifest["panic"] != "unwind":
        raise SystemExit("packaged core does not declare panic=unwind")
    if manifest["target"] != TARGET_TRIPLE:
        raise SystemExit(f"packaged core target mismatch: {manifest['target']}")
    expected = header_exports()
    if verify_exports(library, expected) != library_info["exports"]:
        raise SystemExit("packaged export list does not match manifest")
    check_no_build_paths(library)
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", choices=CONFIGURATIONS, default="debug")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.verify_only:
        manifest = verify(args.configuration)
    else:
        prepare_core(args.configuration)
        manifest = verify(args.configuration)
    lib = manifest["library"]
    assert isinstance(lib, dict)
    print(f"MarkdownCore ({args.configuration}): {lib['bytes']} bytes, sha256 {lib['sha256']}")


if __name__ == "__main__":
    main()
