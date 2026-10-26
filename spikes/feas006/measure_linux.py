#!/usr/bin/env python3
"""Measure Linux C-ABI copying and GTK use while retaining failures/timeouts."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import threading
import time

import yaml

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006/linux"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=BUILD / "measurements")
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if not sys.platform.startswith("linux"):
        raise SystemExit("Native Linux required")
    args.output.mkdir(parents=True, exist_ok=False)
    manifest = yaml.safe_load((ROOT / "fixtures/manifest.yaml").read_text(encoding="utf-8"))
    profiles = yaml.safe_load((ROOT / "fixtures/performance/profiles.yaml").read_text(encoding="utf-8"))
    semantic = [(case["id"], ROOT / "fixtures" / case["input"]) for case in manifest["cases"]]
    performance = [(profile["id"], ROOT / "build/fixtures/performance" / profile["filename"]) for profile in profiles["profiles"]]
    host = BUILD / "probe/feas006-linux-host"
    core = BUILD / "probe/libfeas006_core.so"
    results = {
        "schema_version": 1,
        "platform": platform.platform(),
        "architecture": platform.machine(),
        "core_sha256": hashlib.sha256(core.read_bytes()).hexdigest(),
        "host_sha256": hashlib.sha256(host.read_bytes()).hexdigest(),
        "abi": json.loads(subprocess.check_output([host, "--abi"], text=True)),
        "console": [],
        "semantic_contracts": [],
        "gtk": [],
    }

    def save() -> None:
        (args.output / "results.yaml").write_text(
            yaml.safe_dump(results, sort_keys=False, allow_unicode=True), encoding="utf-8"
        )

    def cli(arguments: list[str], name: str, timeout: float = 120) -> dict:
        with (args.output / f"{name}.stderr").open("wb") as stderr:
            output = subprocess.check_output([host, *arguments], cwd=ROOT, stderr=stderr, timeout=timeout)
        return json.loads(output)

    results["self_test"] = cli(["--self-test", str(performance[1][1])], "self-test", 180)
    save()
    for case, path in semantic:
        contract = cli(["--contract", str(path)], f"contract-{case}")
        record = next(item for item in manifest["cases"] if item["id"] == case)
        expected = yaml.safe_load((ROOT / "fixtures" / record["expected"]).read_text(encoding="utf-8"))
        contract["case"] = case
        contract["matches_expected_outline"] = contract["headings"] == [
            {"level": heading["level"], "text": heading["text"]}
            for heading in expected["document"]["outline"]
        ]
        results["semantic_contracts"].append(contract)
    save()
    for case, path in semantic + performance:
        row = cli([str(path), "5"], f"console-{case}")
        row["case"] = case
        results["console"].append(row)
        save()

    jobs = [(case, path, []) for case, path in semantic]
    jobs += [(case, path, []) for case, path in performance]
    jobs += [
        ("pre-cancelled", performance[1][1], ["--cancel-before-parse"]),
        ("in-flight-cancelled", performance[1][1], ["--cancel-on-entry"]),
    ]
    for case, path, flags in jobs:
        checkpoint = (args.output / f"gtk-{case}.json").resolve()
        command = [
            str(host), "--gtk", str(path),
            "--automated", str(checkpoint), *flags,
        ]
        started = time.monotonic()
        process = subprocess.Popen(
            command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=True,
        )
        peak_rss_bytes = 0
        sampling_done = threading.Event()

        def sample_rss() -> None:
            nonlocal peak_rss_bytes
            status = Path(f"/proc/{process.pid}/status")
            while not sampling_done.wait(0.002):
                try:
                    for line in status.read_text(encoding="utf-8").splitlines():
                        if line.startswith("VmHWM:"):
                            peak_rss_bytes = max(peak_rss_bytes, int(line.split()[1]) * 1024)
                            break
                except (FileNotFoundError, ProcessLookupError):
                    break

        sampler = threading.Thread(target=sample_rss, daemon=True)
        sampler.start()
        timed_out = False
        try:
            _, stderr = process.communicate(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            _, stderr = process.communicate()
        sampling_done.set()
        sampler.join()
        (args.output / f"gtk-{case}.stderr").write_bytes(stderr)
        metrics = json.loads(checkpoint.read_text(encoding="utf-8")) if checkpoint.exists() else {}
        row = {
            "case": case,
            "flags": flags,
            "timed_out": timed_out,
            "exit_code": process.returncode,
            "external_wall_ms": (time.monotonic() - started) * 1000,
            "peak_rss_bytes": peak_rss_bytes or None,
            "metrics": metrics,
        }
        results["gtk"].append(row)
        save()
        print(case, "TIMEOUT" if timed_out else process.returncode, metrics.get("phase"), flush=True)
        if "--cancel-on-entry" in flags:
            accepted = (
                not timed_out
                and process.returncode == 0
                and metrics.get("phase") == "worker-complete"
                and metrics.get("core_status") == "Cancelled"
                and metrics.get("observed_cancel_entry") is True
                and metrics.get("native_owners_zero_before_gtk") is True
            )
            if not accepted:
                raise RuntimeError("In-flight cancellation did not observe entry and return Cancelled")
        if "--cancel-before-parse" in flags:
            accepted = (
                not timed_out
                and process.returncode == 0
                and metrics.get("phase") == "worker-complete"
                and metrics.get("core_status") == "Cancelled"
                and metrics.get("observed_cancel_entry") is False
                and metrics.get("native_owners_zero_before_gtk") is True
            )
            if not accepted:
                raise RuntimeError("Pre-cancellation did not return Cancelled cleanly")


if __name__ == "__main__":
    main()

