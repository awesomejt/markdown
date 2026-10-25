#!/usr/bin/env python3
"""Build native Mac Rust dylibs and Swift hosts; no signing or installed-tool edits."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006/macos"


def main() -> None:
    if sys.platform != "darwin":
        raise SystemExit("Native macOS required")
    env = os.environ.copy()
    if not shutil.which("cargo"):
        isolated = ROOT / "build/feas002-macos/toolchains"
        env["RUSTUP_HOME"] = str(isolated / "rustup")
        env["CARGO_HOME"] = str(isolated / "cargo")
        env["PATH"] = str(isolated / "cargo/bin") + os.pathsep + env["PATH"]
    # Bake a relocatable dependency identity at link time; do not modify or
    # re-sign Mach-O outputs after compilation.
    env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C link-arg=-Wl,-install_name,@rpath/libfeas006_core.dylib"
    manifest = "spikes/feas006/rust/Cargo.toml"
    for kind in ("probe", "standard"):
        target = BUILD / ("rust-" + kind)
        command = ["cargo", "build", "--manifest-path", manifest, "--locked", "--release", "--target-dir", str(target)]
        if kind == "probe":
            command += ["--features", "probe-hooks"]
        subprocess.run(command, cwd=ROOT, env=env, check=True)
        out = BUILD / kind
        out.mkdir(parents=True, exist_ok=True)
        shutil.copy2(target / "release/libfeas006_core.dylib", out)
        for host in ("console", "appkit"):
            sources = ["spikes/feas006/macos/CoreInterop.swift"]
            sources += ["spikes/feas006/macos/Console.swift"] if host == "console" else ["spikes/feas004/Sources/MacRenderingProbe/main.swift"]
            flags = ["-parse-as-library"] if host == "console" else ["-D", "FEAS006"]
            command = ["xcrun", "swiftc", "-swift-version", "5", "-strict-concurrency=complete", "-warnings-as-errors", "-g", "-O", *flags,
                       "-import-objc-header", "spikes/feas006/include/md_core.h", *sources,
                       "-L", str(out), "-lfeas006_core", "-Xlinker", "-rpath", "-Xlinker", "@executable_path",
                       "-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks",
                       "-o", str(out / ("SwiftCoreProbe" if host == "console" else "MacCoreProbe"))]
            subprocess.run(command, cwd=ROOT, check=True)
            if kind == "probe":
                debug = BUILD / "debug"
                debug.mkdir(exist_ok=True)
                shutil.copy2(out / "libfeas006_core.dylib", debug)
                command[command.index("-O")] = "-Onone"
                command[-1] = str(debug / Path(command[-1]).name)
                subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run(["xcrun", "swiftc", "-swift-version", "5", "-g", "-O", "-parse-as-library",
                    "-import-objc-header", "spikes/feas006/include/md_core.h",
                    "spikes/feas006/macos/CoreInterop.swift", "spikes/feas006/macos/MemoryDiagnostics.swift",
                    "-L", str(BUILD / "probe"), "-lfeas006_core", "-Xlinker", "-rpath", "-Xlinker", "@executable_path",
                    "-o", str(BUILD / "probe/MemoryDiagnostics")], cwd=ROOT, check=True)
    print("Built native probe/normal dylibs, release Swift console/AppKit, debug hosts and memory diagnostic.")


if __name__ == "__main__":
    main()
