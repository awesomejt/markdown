#!/usr/bin/env python3
"""Median parse versus extension cost for both engines on the performance corpus.

Each trial is a fresh process with one untimed warmup, matching FEAS-002.
Prints a CSV of every trial, then a median table on stderr.
"""

from __future__ import annotations

import argparse
import csv
import statistics
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "spikes/feas010/target/release/feas010-extensions"
CORPUS = ROOT / "build/fixtures/performance"
ENGINES = ("pulldown", "comrak")
MODES = ("parse", "full", "script")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trials", type=int, default=5)
    args = parser.parse_args()
    if not PROBE.exists() or not CORPUS.exists():
        raise SystemExit("build the probe and run tools/generate_performance_fixtures.py first")
    writer = csv.writer(sys.stdout, lineterminator="\n")
    writer.writerow(["trial", "engine", "file", "mode", "ns_per_iteration"])
    results = defaultdict(list)
    for path in sorted(CORPUS.glob("*.md")):
        iterations = "3" if "5mib" in path.name else "10"
        for trial in range(1, args.trials + 1):
            # Rotate order so no engine or mode always runs first.
            order = [(e, m) for e in ENGINES for m in MODES]
            shift = trial % len(order)
            for engine, mode in order[shift:] + order[:shift]:
                subprocess.run([str(PROBE), "bench", engine, str(path), "1", mode], check=True, capture_output=True)
                out = subprocess.run([str(PROBE), "bench", engine, str(path), iterations, mode],
                                     check=True, capture_output=True, text=True).stdout.splitlines()[1]
                ns = int(out.split(",")[-1])
                writer.writerow([trial, engine, path.name, mode, ns])
                results[(path.name, engine, mode)].append(ns)
    print("\nmedian ms", file=sys.stderr)
    for (name, engine, mode), values in sorted(results.items()):
        print(f"{name:24} {engine:9} {mode:7} {statistics.median(values) / 1e6:9.2f}"
              f"  (min {min(values) / 1e6:.2f}, max {max(values) / 1e6:.2f})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
