#!/usr/bin/env python3
"""Measure the macOS app on the generated profiles (MAC-001G, MAC-001H).

D-012 launch to presented content, repeated-load memory, interaction
timings and external refreshes, from the app's own `--measure` modes. Rows hold counts, times and
memory only. Build the app first (build.py) and generate the profiles
(tools/generate_performance_fixtures.py, which check.py also runs).
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile

from build import REPO_ROOT

PROFILES = ["prose-1mib", "prose-5mib", "long-lines-1mib"]
# D-016 initial targets, in milliseconds.
WARM_LAUNCH, COLD_LAUNCH, JUMP = 400, 700, 150


def app(configuration):
    path = REPO_ROOT / "build" / "macos" / configuration / "Markdown.app"
    if not path.exists():
        raise SystemExit(f"{path} is missing; run build.py --configuration {configuration}")
    return path


def profile(name):
    path = REPO_ROOT / "build" / "fixtures" / "performance" / f"{name}.md"
    if not path.exists():
        raise SystemExit(f"{path} is missing; run tools/generate_performance_fixtures.py")
    return path


def launch_once(bundle, document, cold):
    """One Launch Services launch, as Finder opens a document."""
    if cold:
        # Needs an administrator; the agent shell has none.
        subprocess.run(["sudo", "purge"], check=True)
    with tempfile.TemporaryDirectory(prefix="markdown-measure-") as directory:
        output = Path(directory) / "out.json"
        subprocess.run(["open", "-n", "-W", "-a", str(bundle), "--stdout", str(output),
                        "--stderr", "/dev/null", str(document), "--args", "--measure", "launch"],
                       check=True, timeout=60)
        lines = output.read_text().splitlines() if output.exists() else []
    if not lines:
        raise SystemExit("the app reported no measurement")
    return json.loads(lines[-1])


def direct(bundle, mode, document, timeout):
    executable = bundle / "Contents" / "MacOS" / "Markdown"
    result = subprocess.run([str(executable), "--measure", mode, str(document)],
                            capture_output=True, text=True, timeout=timeout, check=True)
    return [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]


def spread(values):
    return {"min": min(values), "median": statistics.median(values), "max": max(values)}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("mode", choices=["launch", "repeated", "interaction", "refresh"])
    parser.add_argument("--configuration", choices=["debug", "release"], default="release")
    parser.add_argument("--profile", action="append", choices=PROFILES)
    parser.add_argument("--runs", type=int, default=5, help="launches per profile")
    parser.add_argument("--count", type=int, default=20, help="repeated loads or refreshes")
    parser.add_argument("--cold", action="store_true", help="purge caches before each launch")
    args = parser.parse_args()
    bundle = app(args.configuration)
    failed = False
    for name in args.profile or PROFILES:
        document = profile(name)
        if args.mode == "launch":
            if not args.cold:
                launch_once(bundle, document, False)  # warm the caches; not recorded
            rows = [launch_once(bundle, document, args.cold) for _ in range(args.runs)]
            presented = [row["ms"]["presented"] for row in rows]
            completed = [row["ms"]["completed"] for row in rows]
            target = COLD_LAUNCH if args.cold else WARM_LAUNCH
            ok = max(presented) <= target
            failed |= not ok
            print(json.dumps({"profile": name, "cache": "cold" if args.cold else "warm",
                              "presented_ms": spread(presented), "completed_ms": spread(completed),
                              "target_ms": target, "meets_target": ok,
                              "runs": [row["ms"] for row in rows]}))
        elif args.mode == "repeated":
            rows = direct(bundle, f"repeated={args.count}", document, 60 + 10 * args.count)
            retained = [row["retained_footprint_mib"] for row in rows]
            print(json.dumps({"profile": name, "loads": len(rows),
                              "baseline_footprint_mib": rows[0]["baseline_footprint_mib"],
                              "retained_footprint_mib": spread(retained),
                              "retained_by_load": [round(value, 1) for value in retained],
                              "presented_ms": spread([row["presented_ms"] for row in rows])}))
        elif args.mode == "refresh":
            # The app overwrites the document it measures, so give it a copy.
            with tempfile.TemporaryDirectory(prefix="markdown-refresh-") as directory:
                copy = Path(directory) / document.name
                copy.write_bytes(document.read_bytes())
                rows = direct(bundle, f"refresh={args.count}", copy, 60 + 10 * args.count)
            footprints = [row["footprint_mib"] for row in rows]
            print(json.dumps({"profile": name, "refreshes": len(rows),
                              "refresh_ms": spread([row["refresh_ms"] for row in rows]),
                              "kept_place": all(row["kept_place"] for row in rows),
                              "baseline_footprint_mib": rows[0]["baseline_footprint_mib"],
                              "footprint_mib": spread(footprints),
                              "footprint_by_refresh": [round(value, 1) for value in footprints]}))
        else:
            row = direct(bundle, "interaction", document, 120)[-1]
            jumps = [row["ms"][key] for key in ("jump_first_ms", "jump_last_ms", "jump_middle_ms")]
            ok = max(jumps) <= JUMP
            failed |= not ok
            print(json.dumps({"profile": name, **row, "jump_target_ms": JUMP, "meets_target": ok}))
        sys.stdout.flush()
    # A missed target is evidence to record, not a broken tool.
    if failed:
        print("measure: at least one D-016 target was missed", file=sys.stderr)


if __name__ == "__main__":
    main()
