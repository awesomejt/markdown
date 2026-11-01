#!/usr/bin/env python3
"""Stage a model-v2 core package (shared library, header, manifest) for the Linux host.

The Rust host links the core's cdylib, not its staticlib: a Rust static archive
carries a private copy of std, which collides with the host's own std when the
release build uses LTO. The cdylib exports only the C ABI's md_ functions.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tomllib

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]
PACKAGES = REPO_ROOT / "build" / "linux" / "core-package"
TARGET = "x86_64-unknown-linux-gnu"
CONFIGURATIONS = ("debug", "release")


def cargo_homes() -> list[str]:
    """CARGO_HOME as configured and resolved; Cargo passes rustc the former."""
    configured = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo").absolute()
    return list(dict.fromkeys((str(configured), str(configured.resolve()))))


def build_environment() -> dict[str, str]:
    env = dict(os.environ)
    # Cargo prefers the encoded form; it also keeps paths with spaces intact.
    if "CARGO_ENCODED_RUSTFLAGS" in env:
        encoded = env["CARGO_ENCODED_RUSTFLAGS"]
        arguments = encoded.split("\x1f") if encoded else []
    else:
        arguments = env.get("RUSTFLAGS", "").split()
    env.pop("RUSTFLAGS", None)
    # Panic locations name source files; keep the build machine's home, registry
    # and checkout paths out of the archive. The last matching prefix wins, and
    # CARGO_HOME may lie inside the checkout. The C ABI needs panic=unwind.
    remaps = [f"--remap-path-prefix={REPO_ROOT}=."] + [
        f"--remap-path-prefix={home}=/cargo" for home in cargo_homes()]
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(arguments + ["-C", "panic=unwind"] + remaps)
    return env


def prepare_core(configuration: str = "debug") -> Path:
    if configuration not in CONFIGURATIONS:
        raise ValueError("configuration")
    if platform.machine() != "x86_64":
        raise SystemExit("This development package is validated only on x86-64 Linux")
    env = build_environment()
    compiler = subprocess.check_output(["rustc", "--version"], env=env, text=True).strip()
    pinned = tomllib.loads((REPO_ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    if compiler.split()[1] != pinned:
        raise SystemExit(f"Rust compiler must match repository pin {pinned}")
    target_dir = REPO_ROOT / "build" / "linux" / "rust"
    command = ["cargo", "build", "--locked", "-p", "markdown-ffi", "--features", "model-v2",
               "--target", TARGET, "--target-dir", str(target_dir)]
    if configuration == "release":
        command.append("--release")
    subprocess.run(command, cwd=REPO_ROOT, env=env, check=True)

    package = PACKAGES / configuration
    staging = package.with_name(f".{configuration}.staging")
    shutil.rmtree(staging, ignore_errors=True)
    (staging / "lib").mkdir(parents=True)
    (staging / "include").mkdir()
    header = REPO_ROOT / "core" / "include" / "markdown_core.h"
    shutil.copy2(header, staging / "include" / "markdown_core.h")
    library = staging / "lib" / "libmarkdown_ffi.so"
    shutil.copy2(target_dir / TARGET / configuration / "libmarkdown_ffi.so", library)
    shutil.copy2(REPO_ROOT / "LICENSE", staging / "LICENSE")

    # The library must export exactly the header's functions and nothing else.
    expected = set(re.findall(r"\b(md_\w+)\(", header.read_text()))
    symbols = subprocess.check_output(["nm", "-D", "--defined-only", str(library)], text=True)
    exported = {line.split()[-1] for line in symbols.splitlines() if line.strip()}
    if exported != expected:
        raise SystemExit(f"C ABI symbol mismatch: missing={sorted(expected - exported)}, "
                         f"extra={sorted(exported - expected)}")
    manifest = {
        "target": TARGET, "models": [1, 2], "features": ["model-v2"], "panic": "unwind",
        "configuration": configuration,
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True).strip(),
        "rustc": compiler,
        "header_sha256": hashlib.sha256(header.read_bytes()).hexdigest(),
        "cargo_lock_sha256": hashlib.sha256((REPO_ROOT / "Cargo.lock").read_bytes()).hexdigest(),
        "library": {"path": "lib/libmarkdown_ffi.so",
                    "sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
                    "bytes": library.stat().st_size, "exports": sorted(exported)},
    }
    (staging / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    # build.rs reads these two values without a JSON parser.
    (staging / "identity").write_text(
        f"configuration={configuration}\nsha256={manifest['library']['sha256']}\n")
    # Replace the whole package at once, so a failed run never leaves a mixed one.
    shutil.rmtree(package, ignore_errors=True)
    staging.rename(package)
    return package


def verify(configuration: str) -> dict:
    """The staged archive must still be the one its manifest describes."""
    package = PACKAGES / configuration
    manifest = json.loads((package / "manifest.json").read_text())
    library = package / manifest["library"]["path"]
    if (manifest["configuration"] != configuration
            or hashlib.sha256(library.read_bytes()).hexdigest() != manifest["library"]["sha256"]):
        raise SystemExit(f"{package} does not match its manifest; re-run package_core.py")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", choices=CONFIGURATIONS, default="debug")
    print(prepare_core(parser.parse_args().configuration))
