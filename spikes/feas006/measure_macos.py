#!/usr/bin/env python3
"""Native Mac boundary and AppKit checkpoint; preserve all failures and timeouts."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import time

import yaml

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006/macos"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=BUILD / "measurements")
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if sys.platform != "darwin":
        raise SystemExit("Native macOS required")
    args.output.mkdir(parents=True, exist_ok=False)
    manifest = yaml.safe_load((ROOT / "fixtures/manifest.yaml").read_text())
    profiles = yaml.safe_load((ROOT / "fixtures/performance/profiles.yaml").read_text())
    semantic = [(c["id"], ROOT / "fixtures" / c["input"]) for c in manifest["cases"]]
    performance = [(p["id"], ROOT / "build/fixtures/performance" / p["filename"]) for p in profiles["profiles"]]
    console = BUILD / "probe/SwiftCoreProbe"
    app = BUILD / "probe/MacCoreProbe"
    results = {"platform": platform.platform(), "core_sha256": hashlib.sha256((BUILD / "probe/libfeas006_core.dylib").read_bytes()).hexdigest(),
               "console_sha256": hashlib.sha256(console.read_bytes()).hexdigest(),
               "appkit_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
               "swift_console": [], "semantic_contracts": [], "appkit": []}

    def save() -> None:
        (args.output / "results.yaml").write_text(yaml.safe_dump(results, sort_keys=False, allow_unicode=True))

    def cli(arguments: list[str], name: str) -> dict:
        with (args.output / (name + ".stderr")).open("wb") as stderr:
            return json.loads(subprocess.check_output([str(console), *arguments], cwd=ROOT, stderr=stderr, timeout=60))

    results["self_test"] = cli(["--self-test", str(performance[1][1])], "self-test")
    save()
    for case, path in semantic:
        contract = cli(["--contract", str(path)], "contract-" + case)
        record = next(c for c in manifest["cases"] if c["id"] == case)
        expected = yaml.safe_load((ROOT / "fixtures" / record["expected"]).read_text())
        contract["matches_expected_outline"] = contract["headings"] == [
            {"level": h["level"], "text": h["text"]} for h in expected["document"]["outline"]]
        contract["case"] = case
        results["semantic_contracts"].append(contract)
    for case, path in semantic + performance:
        row = cli([str(path), "5"], "swift-" + case)
        row["case"] = case
        results["swift_console"].append(row)
        save()
        print("Swift ABI/copy", case, "passed", flush=True)
    jobs = [(mode, case, path, 1, []) for mode in ["tk1", "tk2", "blocks"] for case, path in semantic]
    jobs += [(mode, case, path, trial, []) for mode in ["tk2", "blocks"] for case, path in performance for trial in range(1, 4)]
    jobs += [("tk1", case, path, 1, []) for case, path in performance[:2]]
    jobs += [("tk2", "pre-cancelled", performance[1][1], 1, ["--cancel-before-parse"]),
             ("tk2", "in-flight-cancelled", performance[1][1], 1, ["--cancel-on-entry"]),
             ("tk2", "swiftui-small", semantic[0][1], 1, ["--swiftui"]),
             ("tk2", "swiftui-large", performance[1][1], 1, ["--swiftui"])]
    for mode, case, path, trial, flags in jobs:
        name = f"{mode}-{case}-{trial}"
        checkpoint = (args.output / (name + ".json")).resolve()
        command = ["/usr/bin/time", "-l", str(app), mode, str(path), "--automated", str(checkpoint), *flags]
        start = time.monotonic()
        process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        timed_out = False
        try:
            _, stderr = process.communicate(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            _, stderr = process.communicate()
        (args.output / (name + ".stderr")).write_bytes(stderr)
        metrics = json.loads(checkpoint.read_text()) if checkpoint.exists() else {}
        intervals = metrics.pop("timer_intervals_ms", [])
        if intervals:
            metrics["timer_p95_ms"] = sorted(intervals)[int(0.95 * (len(intervals) - 1))]
            metrics["timer_max_ms"] = max(intervals)
        rss = re.search(rb"(\d+)\s+maximum resident set size", stderr)
        row = {"mode": mode, "case": case, "trial": trial, "flags": flags, "timed_out": timed_out,
               "exit_code": process.returncode, "external_wall_ms": (time.monotonic() - start) * 1000,
               "peak_rss_bytes": int(rss[1]) if rss else None, "metrics": metrics}
        results["appkit"].append(row)
        save()
        print(name, "TIMEOUT" if timed_out else process.returncode, "core status", metrics.get("core_status"),
              "first draw", round(metrics.get("entry_to_first_draw_ms", -1), 2), flush=True)


if __name__ == "__main__":
    main()
