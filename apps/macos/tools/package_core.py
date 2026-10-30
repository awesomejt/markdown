"""Stage matching model-v2 static libraries and the public header for SwiftPM."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]
PACKAGE = APP_ROOT / "CorePackage"


def prepare_core(configuration="debug"):
    if configuration not in ("debug", "release"):
        raise ValueError("configuration")
    if os.uname().machine != "arm64":
        raise SystemExit("This development package is validated only on Apple Silicon")
    flags = (os.environ.get("RUSTFLAGS", "") + " -C panic=unwind").strip()
    env = dict(os.environ, MACOSX_DEPLOYMENT_TARGET="27.0", RUSTFLAGS=flags)
    if "CARGO_ENCODED_RUSTFLAGS" in env:
        encoded = env["CARGO_ENCODED_RUSTFLAGS"]
        arguments = encoded.split("\x1f") if encoded else []
        env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(arguments + ["-C", "panic=unwind"])
    compiler = subprocess.check_output(["rustc", "--version"], env=env, text=True).strip()
    pinned = tomllib.loads((REPO_ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    if compiler.split()[1] != pinned:
        raise SystemExit(f"Rust compiler must match repository pin {pinned}")
    target_dir = REPO_ROOT / "build/macos/rust"
    target = "aarch64-apple-darwin"
    (PACKAGE / "lib").mkdir(parents=True, exist_ok=True)
    (PACKAGE / "include").mkdir(exist_ok=True)
    header = REPO_ROOT / "core/include/markdown_core.h"
    shutil.copy2(header, PACKAGE / "include/markdown_core.h")
    (PACKAGE / "module.modulemap").write_text(
        'module CMarkdown {\n  header "include/markdown_core.h"\n'
        '  header "include/package_identity.h"\n  export *\n}\n')
    expected = set(re.findall(r"\b(md_\w+)\(", header.read_text()))
    manifest = {"target": target, "minimum_macos": "27.0", "models": [1, 2],
                "features": ["model-v2"], "panic": "unwind", "configuration": configuration,
                "source_commit": subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True).strip(),
                "rustc": compiler,
                "header_sha256": hashlib.sha256(header.read_bytes()).hexdigest(), "libraries": {}}
    command = ["cargo", "build", "--locked", "-p", "markdown-ffi", "--features", "model-v2",
               "--target", target, "--target-dir", str(target_dir)]
    if configuration == "release":
        command.append("--release")
    subprocess.run(command, cwd=REPO_ROOT, env=env, check=True)
    library = PACKAGE / "lib/libmarkdown_ffi.a"
    shutil.copy2(target_dir / target / configuration / "libmarkdown_ffi.a", library)
    # Audit native Mach-O exports; Apple's older LLVM cannot decode optional
    # embedded Rust bitcode. Linking/consumer tests still validate the archive.
    symbols = subprocess.check_output(["nm", "--no-llvm-bc", "-gU", str(library)],
                                      text=True, stderr=subprocess.PIPE)
    exported = set(re.findall(r"\b_(md_\w+)$", symbols, re.MULTILINE))
    if exported != expected:
        raise SystemExit(f"C ABI symbol mismatch: missing={expected-exported}, extra={exported-expected}")
    manifest["libraries"][configuration] = {
        "sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
        "bytes": library.stat().st_size, "exports": sorted(exported)}
    # A changed archive must invalidate SwiftPM's imported module/link graph,
    # even when the public ABI header itself has not changed.
    fingerprint = manifest["libraries"][configuration]["sha256"]
    identity = PACKAGE / "include/package_identity.h"
    content = (f'#define MD_PACKAGE_FINGERPRINT "{fingerprint}"\n'
               f'#define MD_PACKAGE_CONFIGURATION "{configuration}"\n')
    if not identity.exists() or identity.read_text() != content:
        identity.write_text(content)
    manifest["cargo_lock_sha256"] = hashlib.sha256((REPO_ROOT / "Cargo.lock").read_bytes()).hexdigest()
    for stale in (PACKAGE / "lib").glob("libmarkdown_ffi_*.a"):
        stale.unlink()
    shutil.copy2(REPO_ROOT / "LICENSE", PACKAGE / "LICENSE")
    (PACKAGE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return PACKAGE


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", choices=("debug", "release"), default="debug")
    print(prepare_core(parser.parse_args().configuration))
