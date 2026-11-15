#!/usr/bin/env python3
"""Build and run coverage-guided libFuzzer targets for markdown-core and markdown-ffi."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FUZZ_DIR = ROOT / "core" / "fuzz"
BUILD_DIR = ROOT / "build" / "fuzz"

TARGETS = {
    "fuzz_parser": "FUZZ_TARGET_PARSER",
    "fuzz_model_decoder": "FUZZ_TARGET_MODEL_DECODER",
    "fuzz_ffi_session": "FUZZ_TARGET_FFI_SESSION",
    "fuzz_ffi_workspace": "FUZZ_TARGET_FFI_WORKSPACE",
}


def host_triple() -> str:
    out = subprocess.check_output(["rustc", "-vV"], text=True)
    for line in out.splitlines():
        if line.startswith("host: "):
            return line.split(": ", 1)[1].strip()
    raise SystemExit("Could not determine rustc host triple")


def build_staticlib(asan: bool) -> Path:
    triple = host_triple()
    rustflags = [
        "-C", "passes=sancov-module",
        "-C", "llvm-args=-sanitizer-coverage-level=4",
        "-C", "llvm-args=-sanitizer-coverage-inline-8bit-counters",
        "-C", "llvm-args=-sanitizer-coverage-pc-table",
        "-C", "llvm-args=-sanitizer-coverage-trace-compares",
    ]
    env = dict(os.environ)
    if asan:
        rustflags.extend(["-Z", "sanitizer=address"])
        env["RUSTC_BOOTSTRAP"] = "1"
    env["RUSTFLAGS"] = " ".join(rustflags)

    target_dir = BUILD_DIR / "cargo-target"
    cmd = [
        "cargo", "build",
        "--manifest-path", str(FUZZ_DIR / "Cargo.toml"),
        "--release",
        "--target", triple,
        "--target-dir", str(target_dir),
    ]
    print("+ " + " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=ROOT, env=env, check=True)
    lib = target_dir / triple / "release" / "libmarkdown_fuzz.a"
    if not lib.exists():
        raise SystemExit(f"Missing fuzz staticlib: {lib}")
    return lib


def link_target(name: str, define: str, lib: Path, asan: bool) -> Path:
    out = BUILD_DIR / "bin" / name
    out.parent.mkdir(parents=True, exist_ok=True)
    san = "fuzzer,address,undefined" if asan else "fuzzer,undefined"
    cmd = [
        "clang",
        f"-fsanitize={san}",
        "-O2",
        "-g",
        f"-D{define}",
        str(FUZZ_DIR / "harness.c"),
        str(lib),
        "-lpthread",
        "-ldl",
        "-lm",
        "-o", str(out),
    ]
    print("+ " + " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=ROOT, check=True)
    return out


def seed_corpus(name: str) -> Path:
    corpus = BUILD_DIR / "corpus" / name
    corpus.mkdir(parents=True, exist_ok=True)
    if name in ("fuzz_parser", "fuzz_ffi_session"):
        for md in sorted((ROOT / "fixtures" / "markdown").glob("*.md")):
            dst = corpus / md.name
            if not dst.exists():
                shutil.copy2(md, dst)
        # Seed nested wikilink regression input (RUSTSEC-2026-0433 check).
        (corpus / "nested_wikilinks.md").write_text("![[" * 128 + "x" + "]]" * 128)
    elif name == "fuzz_model_decoder":
        (corpus / "empty.json").write_bytes(b"{}")
    elif name == "fuzz_ffi_workspace":
        (corpus / "sample.bin").write_bytes(b"notes\x00notes/plan.md\x00docs/readme.md\x00")
    return corpus


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=int, default=10,
                        help="Max total seconds per target (default: 10)")
    parser.add_argument("--targets", nargs="*", choices=sorted(TARGETS),
                        default=list(TARGETS.keys()),
                        help="Targets to build and run")
    parser.add_argument("--no-asan", action="store_true",
                        help="Disable AddressSanitizer on Rust staticlib")
    parser.add_argument("--build-only", action="store_true",
                        help="Build targets without running")
    args = parser.parse_args()

    asan = not args.no_asan
    lib = build_staticlib(asan=asan)
    binaries = {name: link_target(name, TARGETS[name], lib, asan=asan) for name in args.targets}
    if args.build_only:
        return 0

    artifacts = BUILD_DIR / "artifacts"
    artifacts.mkdir(parents=True, exist_ok=True)
    for name, binary in binaries.items():
        corpus = seed_corpus(name)
        cmd = [
            str(binary),
            str(corpus),
            f"-max_total_time={args.seconds}",
            f"-artifact_prefix={artifacts}/{name}-",
            "-max_len=16384",
            "-print_final_stats=1",
        ]
        print(f"=== Running {name} for {args.seconds}s ===", flush=True)
        subprocess.run(cmd, cwd=ROOT, check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
