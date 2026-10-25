#!/usr/bin/env python3
"""Serial macOS UI measurements with process-group timeouts and durable checkpoints."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import signal
import statistics
import subprocess
import sys
import time

import yaml

ROOT = Path(__file__).resolve().parents[2]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--smoke", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin" or args.trials < 1 or args.timeout <= 0:
        raise SystemExit("Requires macOS and positive trials/timeout.")
    out = ROOT / "build/feas004/measurements"
    out.mkdir(parents=True, exist_ok=True)
    manifest = yaml.safe_load((ROOT / "fixtures/manifest.yaml").read_text())
    profiles = yaml.safe_load((ROOT / "fixtures/performance/profiles.yaml").read_text())
    cases = [(c["id"], "fixtures/" + c["input"], 1) for c in manifest["cases"]]
    if not args.smoke:
        cases += [(c["id"], "build/fixtures/performance/" + c["filename"], args.trials)
                  for c in profiles["profiles"]]
    else:
        cases = cases[:1]
    rows = []
    for case, path, trials in cases:
        for mode, swiftui in [("tk1", False), ("tk2", False), ("blocks", False), ("tk2", True)]:
            for trial in range(trials):
                name = f"{case}-{mode}-{'swiftui' if swiftui else 'appkit'}-{trial + 1}"
                checkpoint = out / f"{name}.json"
                checkpoint.unlink(missing_ok=True)
                command = ["/usr/bin/time", "-l", "spikes/feas004/.build/release/MacRenderingProbe", mode, path]
                if swiftui:
                    command.append("--swiftui")
                command += ["--automated", str(checkpoint)]
                start = time.monotonic()
                process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, start_new_session=True)
                timeout = False
                try:
                    stdout, stderr = process.communicate(timeout=args.timeout)
                except subprocess.TimeoutExpired:
                    timeout = True
                    os.killpg(process.pid, signal.SIGKILL)
                    stdout, stderr = process.communicate()
                elapsed = (time.monotonic() - start) * 1000
                (out / f"{name}.stderr").write_bytes(stderr)
                metrics = json.loads(checkpoint.read_text()) if checkpoint.exists() else {}
                intervals = metrics.pop("timer_intervals_ms", [])
                if intervals:
                    metrics["timer_median_ms"] = statistics.median(intervals)
                    metrics["timer_p95_ms"] = sorted(intervals)[int(0.95 * (len(intervals) - 1))]
                    metrics["timer_max_ms"] = max(intervals)
                rss = re.search(rb"(\d+)\s+maximum resident set size", stderr)
                row = {"case": case, "mode": mode, "shell": "swiftui" if swiftui else "appkit",
                       "trial": trial + 1, "timeout": timeout, "returncode": process.returncode,
                       "external_wall_ms": elapsed, "metrics": metrics,
                       "peak_rss_bytes": int(rss[1]) if rss else None}
                rows.append(row)
                (out / "results.yaml").write_text(yaml.safe_dump(rows, sort_keys=False))
                print(name, "TIMEOUT" if timeout else process.returncode,
                      "first draw", round(metrics.get("entry_to_first_draw_ms", -1), 2), flush=True)
    print(f"Recorded {len(rows)} trials in {out.relative_to(ROOT)}/results.yaml")


if __name__ == "__main__":
    main()
