#!/usr/bin/env python3
"""Five fresh-process compiler screens. Not D-012 or cold-cache benchmarks."""
from __future__ import annotations

import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "spikes/feas009"
BUILD = ROOT / "build/feas009"


def main() -> None:
    if sys.platform != "darwin":
        raise SystemExit("Native macOS required; adapt the harness for other platforms")
    BUILD.mkdir(parents=True, exist_ok=True)
    commands = {
        "katex": [str(BUILD / "EngineProbe"), "jsc", str(SOURCE / "node_modules/katex/dist/katex.min.js"), str(SOURCE / "katex-probe.js")],
        "mermaid": [str(BUILD / "EngineProbe"), "webkit", str(SOURCE / "node_modules/mermaid/dist/mermaid.min.js"), str(SOURCE / "mermaid-probe.js")],
        "d2": [str(BUILD / "D2Probe")],
    }
    result = {
        "schema_version": 1, "task": "FEAS-009", "platform_part": "macos",
        "method": "Five fresh processes with unpurged caches; six operations per case, first discarded. Compile only, no presented-content or visual-fidelity claim.",
        "environment": {"platform": platform.platform(), "machine": platform.machine(),
                        "os": subprocess.check_output(["sw_vers"], text=True).strip()},
        "engines": {},
    }
    for name, command in commands.items():
        runs = []
        for index in range(5):
            start = time.perf_counter()
            process = subprocess.run(["/usr/bin/time", "-l", *command], cwd=SOURCE,
                                     capture_output=True, text=True, timeout=45)
            elapsed = (time.perf_counter() - start) * 1000
            if process.returncode:
                raise RuntimeError(f"{name}: {process.stdout}\n{process.stderr}")
            output = json.loads(process.stdout)
            rss = re.search(r"(\d+)\s+maximum resident set size", process.stderr)
            runs.append({"process_to_result_ms": elapsed,
                         "host_peak_rss_bytes": int(rss[1]) if rss else None,
                         "output": output})
            print(f"{name}: {index + 1}/5 complete", file=sys.stderr, flush=True)
        cases = {}
        for case in runs[0]["output"]["results"]:
            key = case["name"]
            samples = [value for run in runs for item in run["output"]["results"]
                       if item["name"] == key for value in item["samples_ms"]]
            cases[key] = {"median_ms": statistics.median(samples), "min_ms": min(samples),
                          "max_ms": max(samples), "sample_count": len(samples),
                          "outcome": {k: v for k, v in case.items() if k not in ("name", "samples_ms")}}
        result["engines"][name] = {"runs": runs, "cases": cases,
            "process_to_result_median_ms": statistics.median(r["process_to_result_ms"] for r in runs),
            "host_peak_rss_median_bytes": statistics.median(r["host_peak_rss_bytes"] for r in runs)}
    assets = [SOURCE / "node_modules/mermaid/dist/mermaid.min.js",
              SOURCE / "node_modules/katex/dist/katex.min.js", BUILD / "EngineProbe", BUILD / "D2Probe"]
    result["artifact_bytes"] = {p.name: p.stat().st_size for p in assets}
    result["memory_limit"] = "time -l peak RSS is host-only. WKWebView child processes are excluded; never compare it as total WebKit memory."
    (BUILD / "measurements-macos.json").write_text(json.dumps(result, indent=2) + "\n")
    print(BUILD / "measurements-macos.json")


if __name__ == "__main__":
    main()
