#!/usr/bin/env python3
"""Run serial native Windows probes; checkpoint every trial and bound stalls."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=45)
    parser.add_argument("--output", type=Path, default=ROOT / "build/feas003/measurements")
    parser.add_argument("--surfaces", nargs="+", default=["richtext", "richedit", "blocks"])
    args = parser.parse_args()
    if sys.platform != "win32":
        raise SystemExit("Run on native Windows Python; cross-builds are not rendering evidence")
    exe = next((ROOT / "build/feas003/csharp/bin/x64/Release").rglob("WindowsRenderingProbe.exe"))
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = yaml.safe_load((ROOT / "fixtures/manifest.yaml").read_text(encoding="utf-8"))
    profiles = yaml.safe_load((ROOT / "fixtures/performance/profiles.yaml").read_text(encoding="utf-8"))
    cases = [(c["id"], ROOT / "fixtures" / c["input"], 1) for c in manifest["cases"]]
    cases += [(p["id"], ROOT / "build/fixtures/performance" / p["filename"], args.trials) for p in profiles["profiles"]]
    results = {"platform": platform.platform(), "method": "fresh process per trial, serial",
               "probe_assembly_sha256": hashlib.sha256(exe.with_suffix(".dll").read_bytes()).hexdigest(),
               "timeout_seconds": args.timeout, "trials": []}
    for surface in args.surfaces:
        for case_id, input_path, trials in cases:
            for trial in range(1, trials + 1):
                key = f"{surface}-{case_id}-{trial}"
                output = args.output / f"{key}.json"
                if output.exists():
                    raise SystemExit(f"Refusing to overwrite earlier measurement: {output}")
                started = time.perf_counter()
                with (args.output / f"{key}.stderr").open("wb") as stderr:
                    process = subprocess.Popen([str(exe), surface, str(input_path), str(output.resolve())],
                                               cwd=ROOT, stderr=stderr, stdout=subprocess.DEVNULL)
                    first_checkpoint = None
                    timed_out = False
                    try:
                        while process.poll() is None:
                            elapsed = time.perf_counter() - started
                            # Do not hold a Windows file handle while the producer
                            # atomically replaces its JSON checkpoint.
                            if first_checkpoint is None and Path(str(output) + ".ready").exists():
                                first_checkpoint = elapsed * 1000
                            if elapsed >= args.timeout:
                                timed_out = True
                                break
                            time.sleep(0.01)
                    finally:
                        if process.poll() is None:
                            process.kill()
                        process.wait()
                row = {"surface": surface, "case": case_id, "trial": trial,
                       "exit_code": process.returncode, "timed_out": timed_out,
                       "launch_to_first_checkpoint_ms": first_checkpoint,
                       "launch_to_exit_ms": (time.perf_counter() - started) * 1000}
                if output.exists():
                    data = json.loads(output.read_text(encoding="utf-8"))
                    if "error" in data:
                        data["error"] = data["error"].replace(str(ROOT), "<repo>")
                    row["metrics"] = data
                results["trials"].append(row)
                (args.output / "results.yaml").write_text(yaml.safe_dump(results, sort_keys=False), encoding="utf-8")
                metrics = row.get("metrics", {})
                status = "TIMEOUT" if timed_out else "ERROR" if process.returncode or metrics.get("error") else "complete"
                print(key, status,
                      "render_ms=", metrics.get("entry_to_render_callback_ms"),
                      "end=", metrics.get("end_reached"), "selection=", metrics.get("selection_matches"), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
