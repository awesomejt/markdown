#!/usr/bin/env python3
"""Measure the Windows Native AOT app on generated performance profiles (WIN-001G).

D-012 launch to presented content, repeated-load memory, and interaction timings
from the app's own `--measure` modes. Rows hold counts, times, and memory only.
Build the app first (apps/windows/tools/check.py) and generate the performance fixtures
(tools/generate_performance_fixtures.py).
"""
import argparse
import ctypes
import uuid
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile

REPO_ROOT = Path(__file__).resolve().parents[3]
PROFILES = ["prose-1mib", "prose-5mib", "long-lines-1mib"]

# D-016 initial targets, in milliseconds.
WARM_LAUNCH, COLD_LAUNCH, JUMP = 400, 700, 150


def app_binary(configuration: str) -> Path:
    path = REPO_ROOT / "build" / "windows" / configuration / "Markdown.Windows.exe"
    if not path.exists():
        raise SystemExit(f"{path} is missing; publish the app first")
    return path


def profile_path(name: str) -> Path:
    path = REPO_ROOT / "build" / "fixtures" / "performance" / f"{name}.md"
    if not path.exists():
        raise SystemExit(f"{path} is missing; run tools/generate_performance_fixtures.py")
    return path


def run_measure(binary: Path, mode: str, doc_path: Path) -> dict:
    cmd = [str(binary), "--measure", mode, str(doc_path)]
    with tempfile.TemporaryDirectory(prefix="markdown-window-measure-") as directory:
        env = dict(os.environ)
        for key in ("PREFERENCES", "RESTORATION", "RECENTS"):
            env[f"MARKDOWN_{key}_PATH"] = str(Path(directory) / f"{key.lower()}.json")
        env["MARKDOWN_INSTANCE_KEY"] = f"measure-{uuid.uuid4().hex}"
        before = ctypes.c_longlong()
        ctypes.windll.kernel32.QueryPerformanceCounter(ctypes.byref(before))
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, timeout=120, env=env)
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        raise SystemExit(f"No output from: {' '.join(cmd)}")
    text = "\n".join(lines)
    row = json.loads(text)
    if "presented_qpc" in row:
        row["launch_to_frame_ms"] = (row["presented_qpc"] - before.value) * 1000 / row["qpc_frequency"]
    return row


def spread(values: list[float]) -> dict:
    return {
        "min": round(min(values), 2),
        "median": round(statistics.median(values), 2),
        "max": round(max(values), 2),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("mode", choices=["launch", "repeated", "interaction", "refresh", "window-launch", "jump-frame"])
    parser.add_argument("--configuration", choices=["Debug", "Release"], default="Release")
    parser.add_argument("--profile", action="append", choices=PROFILES)
    parser.add_argument("--runs", type=int, default=5, help="launches per profile")
    parser.add_argument("--count", type=int, default=20, help="repeated loads or refreshes")
    args = parser.parse_args()

    binary = app_binary(args.configuration)
    profiles = args.profile or PROFILES

    for name in profiles:
        doc = profile_path(name)
        if args.mode in ("window-launch", "jump-frame"):
            _ = run_measure(binary, args.mode, doc)
            rows = [run_measure(binary, args.mode, doc) for _ in range(args.runs)]
            out = {"profile": name, "mode": args.mode, "runs": args.runs,
                   "launch_to_frame_ms": spread([r["launch_to_frame_ms"] for r in rows]),
                   "peak_rss_mib": spread([r["peak_rss_mib"] for r in rows]),
                   "realized_rows": [r["realized_rows"] for r in rows], "proxy": rows[0]["proxy"],
                   "cache": "one warmup; warm filesystem caches; no cold-cache claim"}
            if args.mode == "jump-frame":
                out["jump_ms"] = spread([v for r in rows for v in r["jump_ms"]])
            print(json.dumps(out, indent=2))
        elif args.mode == "launch":
            # Warm up cache once
            _ = run_measure(binary, "launch", doc)
            rows = [run_measure(binary, "launch", doc) for _ in range(args.runs)]
            pipeline = [r["pipeline_ms"] for r in rows]
            parse = [r["parse_ms"] for r in rows]
            plan = [r["plan_ms"] for r in rows]
            target = WARM_LAUNCH
            meets_target = (max(pipeline) <= WARM_LAUNCH) if target is not None else None
            target_note = "In-process pipeline comparison only; not process launch or presented content"
            out = {
                "profile": name,
                "configuration": args.configuration,
                "runs": args.runs,
                "pipeline_ms": spread(pipeline),
                "parse_ms": spread(parse),
                "plan_ms": spread(plan),
                "target_ms": target,
                "meets_target": meets_target,
                "target_note": target_note,
                "peak_rss_kib": spread([float(r["peak_rss_kib"]) for r in rows]),
            }
            print(json.dumps(out, indent=2))
        elif args.mode == "repeated":
            res = run_measure(binary, f"repeated={args.count}", doc)
            loads = res.get("loads_data", [])
            times = [l["elapsed_ms"] for l in loads]
            rss = [l["retained_rss_mib"] for l in loads]
            out = {
                "profile": name,
                "configuration": args.configuration,
                "loads": res["loads"],
                "baseline_rss_mib": res["baseline_rss_mib"],
                "final_rss_mib": res["final_rss_mib"],
                "elapsed_ms": spread(times),
                "retained_rss_mib": spread(rss),
            }
            print(json.dumps(out, indent=2))
        elif args.mode == "interaction":
            res = run_measure(binary, "interaction", doc)
            out = {
                "profile": name,
                "configuration": args.configuration,
                "target_resolution_ms": res["target_resolution_ms"],
                "meets_target_resolution_150ms": res["meets_target_resolution_150ms"],
                "copy_slice_ms": res["copy_slice_ms"],
                "clipboard_write_ms": res["clipboard_write_ms"],
                "plain_chars": res["plain_chars"],
                "markdown_bytes": res["markdown_bytes"],
                "markdown_equals_source": res["markdown_equals_source"],
                "readback_plain_equals_model": res["readback_plain_equals_model"],
                "readback_markdown_equals_source": res["readback_markdown_equals_source"],
            }
            print(json.dumps(out, indent=2))
        elif args.mode == "refresh":
            with tempfile.TemporaryDirectory(prefix="markdown-refresh-") as directory:
                copy = Path(directory) / doc.name
                copy.write_bytes(doc.read_bytes())
                res = run_measure(binary, f"refresh={args.count}", copy)
            refreshes = res.get("refreshes_data", [])
            times = [r["refresh_ms"] for r in refreshes]
            rss = [r["footprint_mib"] for r in refreshes]
            out = {
                "profile": name,
                "configuration": args.configuration,
                "refreshes": len(refreshes),
                "refresh_ms": spread(times),
                "kept_place": all(r["kept_place"] for r in refreshes),
                "baseline_footprint_mib": res["baseline_footprint_mib"],
                "footprint_mib": spread(rss),
                "footprint_by_refresh": [round(v, 1) for v in rss],
            }
            print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
