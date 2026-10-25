#!/usr/bin/env python3
"""Windows integration evidence, with serial probes and preserved timeouts."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build/feas006"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=BUILD / "measurements")
    args = parser.parse_args()
    if sys.platform != "win32": raise SystemExit("Native Windows required for named-host evidence")
    args.output.mkdir(parents=True, exist_ok=False)
    console = next((BUILD / "console/Release").rglob("ConsoleProbe.exe"))
    window = next((BUILD / "winui/x64/Release").rglob("Feas006WindowsProbe.exe"))
    native = BUILD / "native/FfiNativeClient.exe"
    core = BUILD / "rust/release/feas006_core.dll"
    manifest = yaml.safe_load((ROOT / "fixtures/manifest.yaml").read_text(encoding="utf-8"))
    profiles = yaml.safe_load((ROOT / "fixtures/performance/profiles.yaml").read_text(encoding="utf-8"))
    semantic = [(c["id"], ROOT / "fixtures" / c["input"]) for c in manifest["cases"]]
    performance = [(p["id"], ROOT / "build/fixtures/performance" / p["filename"]) for p in profiles["profiles"]]
    results = {"platform": platform.platform(), "dll_sha256": hashlib.sha256(core.read_bytes()).hexdigest(),
               "managed_console": [], "native_console": [], "semantic_contracts": [], "winui": []}
    def save() -> None:
        (args.output / "results.yaml").write_text(yaml.safe_dump(results, sort_keys=False, allow_unicode=True), encoding="utf-8")
    def cli(command: list[str], key: str) -> dict:
        with (args.output / (key + ".stderr")).open("wb") as stderr:
            value = subprocess.check_output(command, cwd=ROOT, stderr=stderr, timeout=60)
        return json.loads(value)
    results["self_test"] = cli([str(console), "--self-test", str(performance[1][1])], "self-test")
    save()
    for case_id, path in semantic:
        contract = cli([str(console), "--contract", str(path)], "contract-" + case_id)
        case = next(c for c in manifest["cases"] if c["id"] == case_id)
        expected = yaml.safe_load((ROOT / "fixtures" / case["expected"]).read_text(encoding="utf-8"))
        outline = [{"level": h["level"], "text": h["text"]} for h in expected["document"]["outline"]]
        contract["matches_expected_outline"] = contract["headings"] == outline
        contract["case"] = case_id; results["semantic_contracts"].append(contract)
    for case_id, path in semantic + performance:
        managed = cli([str(console), str(path), "5"], "managed-" + case_id)
        managed["case"] = case_id;results["managed_console"].append(managed)
        cpp = cli([str(native), str(core), str(path), "5"], "cpp-" + case_id)
        cpp["case"] = case_id;results["native_console"].append(cpp)
        save();print("ABI/copy", case_id, "passed", flush=True)
    jobs = [(surface, case, path, 1, []) for surface in ["richtext", "richedit", "blocks"] for case, path in semantic]
    jobs += [("blocks", case, path, trial, []) for case, path in performance for trial in range(1, 4)]
    jobs += [(surface, case, path, 1, []) for surface in ["richtext", "richedit"] for case, path in performance[:2]]
    jobs += [("blocks", "pre-cancelled", performance[1][1], 1, ["--cancel-before-parse"])]
    for surface, case, path, trial, flags in jobs:
        output = (args.output / f"{surface}-{case}-{trial}.json").resolve()
        started = time.perf_counter(); timeout = False
        process = subprocess.Popen([str(window), surface, str(path), str(output), *flags], cwd=ROOT)
        try: process.wait(timeout=45)
        except subprocess.TimeoutExpired: timeout = True
        finally:
            if process.poll() is None: process.kill()
            process.wait()
        row = {"surface": surface, "case": case, "trial": trial, "exit_code": process.returncode,
               "timed_out": timeout, "launch_to_exit_ms": (time.perf_counter() - started) * 1000}
        if output.exists():
            row["metrics"] = json.loads(output.read_text(encoding="utf-8"))
            if "error" in row["metrics"]: row["metrics"]["error"] = row["metrics"]["error"].replace(str(ROOT), "<repo>")
        results["winui"].append(row); save()
        metrics = row.get("metrics", {})
        print(surface, case, trial, "TIMEOUT" if timeout else "ERROR" if metrics.get("error") else "complete",
              "core_ms", metrics.get("core_parse_model_ms"), "render_ms", metrics.get("entry_to_render_callback_ms"), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
