#!/usr/bin/env python3
"""Build Linux Rust core variants and the gtk-rs runtime C-ABI host."""
from __future__ import annotations

import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006/linux"
CORE_MANIFEST = ROOT / "spikes/feas006/rust/Cargo.toml"
HOST_MANIFEST = ROOT / "spikes/feas006/linux/Cargo.toml"


def run(*command: str, capture: bool = False) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout if capture else ""


def main() -> None:
    if not sys.platform.startswith("linux"):
        raise SystemExit("Native Linux required")
    for kind, features in (("probe", ("--features", "probe-hooks")), ("standard", ())):
        target = BUILD / f"rust-{kind}"
        run(
            "cargo", "build", "--manifest-path", str(CORE_MANIFEST), "--locked",
            "--release", "--target-dir", str(target), *features,
        )
        destination = BUILD / kind
        destination.mkdir(parents=True, exist_ok=True)
        shutil.copy2(target / "release/libfeas006_core.so", destination)

    host_target = BUILD / "host-target"
    run(
        "cargo", "build", "--manifest-path", str(HOST_MANIFEST), "--locked",
        "--release", "--target-dir", str(host_target),
    )
    for kind in ("probe", "standard"):
        shutil.copy2(host_target / "release/feas006-linux-host", BUILD / kind)

    debug_core = BUILD / "rust-debug"
    debug_host = BUILD / "host-debug"
    run(
        "cargo", "build", "--manifest-path", str(CORE_MANIFEST), "--locked",
        "--features", "probe-hooks", "--target-dir", str(debug_core),
    )
    run(
        "cargo", "build", "--manifest-path", str(HOST_MANIFEST), "--locked",
        "--target-dir", str(debug_host),
    )
    debug = BUILD / "debug"
    debug.mkdir(parents=True, exist_ok=True)
    shutil.copy2(debug_core / "debug/libfeas006_core.so", debug)
    shutil.copy2(debug_host / "debug/feas006-linux-host", debug)

    layout_source = BUILD / "header-layout.c"
    layout_binary = BUILD / "header-layout"
    layout_source.write_text(
        '#include <stdio.h>\n#include "md_core.h"\n'
        'int main(void) { printf("%zu %zu %zu\\n", sizeof(md_options), '
        'sizeof(md_block), sizeof(md_snapshot)); return 0; }\n',
        encoding="utf-8",
    )
    run(
        "cc", "-std=c11", "-Wall", "-Wextra", "-Werror",
        "-I", str(ROOT / "spikes/feas006/include"), str(layout_source),
        "-o", str(layout_binary),
    )
    c_sizes = [int(value) for value in run(str(layout_binary), capture=True).split()]
    abi = json.loads(run(str(BUILD / "probe/feas006-linux-host"), "--abi", capture=True))
    host_sizes = [abi["options_bytes"], abi["block_bytes"], abi["snapshot_bytes"]]
    if c_sizes != host_sizes:
        raise RuntimeError(f"C/header layout mismatch: {c_sizes} != {host_sizes}")
    details = {
        "platform": platform.platform(),
        "architecture": platform.machine(),
        "c_header_sizes": c_sizes,
        "host_and_core_abi": abi,
    }
    (BUILD / "layout-check.json").write_text(json.dumps(details, indent=2), encoding="utf-8")
    print(json.dumps(details))


if __name__ == "__main__":
    main()

