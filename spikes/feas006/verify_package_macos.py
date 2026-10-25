#!/usr/bin/env python3
"""Relocate the owned development ZIP, then test its normal ABI and app binaries."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("package_info", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "darwin":
        raise SystemExit("Native macOS required")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    info = json.loads(args.package_info.read_text())
    with zipfile.ZipFile(info["zip"]) as archive:
        if archive.testzip() is not None or not (archive.getinfo("SwiftCoreProbe").external_attr >> 16) & 0o111:
            raise RuntimeError("Package integrity/executable mode failed")
    relocated = args.output / "relocated"
    relocated.mkdir()
    # Unlike Python ZipFile.extractall, ditto restores recorded executable bits.
    subprocess.run(["ditto", "-x", "-k", info["zip"], str(relocated)], check=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith("DYLD_")}
    normal = json.loads(subprocess.check_output([str(relocated / "SwiftCoreProbe"), "--normal-check"], cwd="/private/tmp", env=env))
    if normal["probe_hooks"] != 0 or not normal["normal_check"]["passed"]:
        raise RuntimeError("Normal ABI tests failed")
    core_hash = hashlib.sha256((relocated / "libfeas006_core.dylib").read_bytes()).hexdigest()
    results = {"normal": normal, "zip_integrity": True, "executable_permissions": True,
               "extractor": "ditto -x -k", "launch_cwd": "outside repository",
               "core_sha256": core_hash, "apps": []}
    for mode, case, flags in [("tk1", "small", []), ("tk2", "small", []), ("blocks", "small", []),
                              ("tk2", "large", []), ("tk2", "cancelled", ["--cancel-before-parse"])]:
        contents = relocated / f"MarkdownCore-{mode}.app/Contents"
        if hashlib.sha256((contents / "Frameworks/libfeas006_core.dylib").read_bytes()).hexdigest() != core_hash:
            raise RuntimeError("App/core fingerprint mismatch")
        fixture = ROOT / "build/fixtures/performance/prose-5mib.md" if case == "large" else relocated / "fixtures/markdown/foundation/gfm-tour.md"
        checkpoint = args.output / f"{mode}-{case}.json"
        process = subprocess.Popen([str(contents / "MacOS/MacCoreProbe"), mode, str(fixture),
                                    "--automated", str(checkpoint), *flags], cwd="/private/tmp", env=env,
                                   start_new_session=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            _, stderr = process.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate()
            raise
        (args.output / f"{mode}-{case}.stderr").write_bytes(stderr)
        metrics = json.loads(checkpoint.read_text())
        if process.returncode != 0 or not metrics.get("completed"):
            raise RuntimeError("Packaged app did not complete")
        if case == "cancelled":
            if metrics.get("core_status") != 5:
                raise RuntimeError("Packaged cancellation failed")
        elif metrics.get("probe_hooks") != 0 or not metrics.get("first_draw_observed") or (mode != "blocks" and not metrics.get("selection_copy_matches")):
            raise RuntimeError("Packaged presentation/copy failed")
        metrics.pop("timer_intervals_ms", None)
        results["apps"].append({"mode": mode, "case": case, "metrics": metrics})
        (args.output / "results.json").write_text(json.dumps(results, indent=2))
        print("Relocated normal package", mode, case, "passed", flush=True)


if __name__ == "__main__":
    main()
