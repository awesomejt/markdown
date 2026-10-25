#!/usr/bin/env python3
"""Run the four release probes serially on macOS; preserve trials and RSS bytes."""
from __future__ import annotations

import csv
import io
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys

import yaml

ROOT = Path(__file__).resolve().parents[2]
ENGINES = {
    "comrak": ["spikes/feas002/target/release/markdown-parser-probe", "comrak"],
    "pulldown": ["spikes/feas002/target/release/markdown-parser-probe", "pulldown"],
    "cmark-gfm": ["build/feas002/cmark-probe"],
    "swift-markdown": ["spikes/feas002/swift/.build/release/swift-markdown-probe"],
}


def run(engine: str, path: str, iterations: int) -> dict:
    command = ["/usr/bin/time", "-l", *ENGINES[engine], path, str(iterations)]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True,
                            timeout=120, check=True)
    rows = list(csv.DictReader(io.StringIO(result.stdout)))
    if len(rows) != 1 or rows[0]["engine"] != engine:
        raise RuntimeError(f"unexpected output: {result.stdout}")
    row = rows[0]
    for key in ("bytes", "iterations", "elapsed_ns", "ns_per_iteration", "units", "positioned_units"):
        row[key] = int(row[key])
    if row["bytes"] != (ROOT / path).stat().st_size or row["units"] <= 0:
        raise RuntimeError(f"invalid counts: {row}")
    match = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
    if not match:
        raise RuntimeError(f"missing Darwin RSS: {result.stderr}")
    row["peak_rss_bytes"] = int(match[1])
    return row


def main() -> None:
    if sys.platform != "darwin":
        raise SystemExit("This runner interprets /usr/bin/time -l RSS as Darwin bytes.")
    output = ROOT / "build/feas002-macos"
    output.mkdir(parents=True, exist_ok=True)
    profiles = yaml.safe_load((ROOT / "fixtures/performance/profiles.yaml").read_text())["profiles"]
    rows = []
    summary = {"schema_version": 1, "platform": platform.platform(),
               "method": {"trials": 5, "warmups_per_process": 1,
                          "iterations_per_trial": "10 (1 MiB), 3 (5 MiB)",
                          "timing": "probe monotonic parse-and-walk interval; file read excluded",
                          "rss": "Darwin /usr/bin/time -l maximum RSS in bytes; whole process"},
               "performance": {}, "memory_single_iteration": [], "semantic_smoke": []}
    for profile in profiles:
        case = profile["id"]
        path = f"build/fixtures/performance/{profile['filename']}"
        iterations = 3 if profile["target_bytes"] > 1048576 else 10
        results = {engine: [] for engine in ENGINES}
        # Rotate execution order across trials to reduce fixed-order drift.
        names = list(ENGINES)
        for trial in range(5):
            for engine in names[trial % 4:] + names[:trial % 4]:
                row = run(engine, path, iterations)
                rows.append({"kind": "performance", "trial": trial + 1, **row})
                results[engine].append(row)
        summary["performance"][case] = {}
        for engine, values in results.items():
            times = [v["ns_per_iteration"] / 1e6 for v in values]
            summary["performance"][case][engine] = {
                "median_ms": statistics.median(times), "min_ms": min(times), "max_ms": max(times),
                "median_peak_rss_bytes": statistics.median(v["peak_rss_bytes"] for v in values),
                "max_peak_rss_bytes": max(v["peak_rss_bytes"] for v in values),
                "units": values[0]["units"], "positioned_units": values[0]["positioned_units"],
            }
        print(case, json.dumps(summary["performance"][case]), flush=True)
    for case in ("prose-5mib", "delimiter-storm-1mib"):
        for engine in ENGINES:
            row = run(engine, f"build/fixtures/performance/{case}.md", 1)
            rows.append({"kind": "memory-check", "trial": 1, **row})
            summary["memory_single_iteration"].append(row)
    manifest = yaml.safe_load((ROOT / "fixtures/manifest.yaml").read_text())
    for case in manifest["cases"]:
        for engine in ENGINES:
            row = run(engine, f"fixtures/{case['input']}", 1)
            rows.append({"kind": "semantic-smoke", "trial": 1, **row})
            summary["semantic_smoke"].append({"case": case["id"], "engine": engine,
                                             "units": row["units"],
                                             "positioned_units": row["positioned_units"]})
    with (output / "trials.csv").open("w") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    (output / "summary.yaml").write_text(yaml.safe_dump(summary, sort_keys=False))
    print(f"Saved {len(rows)} rows; semantic runs are smoke checks, not golden conformance.")


if __name__ == "__main__":
    main()
