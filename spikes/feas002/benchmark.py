#!/usr/bin/env python3
"""Run repeatable parse-and-walk trials and emit one CSV row per trial."""

from __future__ import annotations

import argparse
import csv
import subprocess
import sys
from pathlib import Path


ENGINES = ("comrak", "pulldown", "cmark-gfm")
HEADER = (
    "trial",
    "engine",
    "file",
    "bytes",
    "iterations",
    "elapsed_ns",
    "ns_per_iteration",
    "units",
    "positioned_units",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--one-mib-iterations", type=int, default=10)
    parser.add_argument("--five-mib-iterations", type=int, default=3)
    return parser.parse_args()


def run(command: list[str], cwd: Path) -> list[str]:
    result = subprocess.run(
        command, check=True, text=True, capture_output=True, cwd=cwd
    )
    rows = list(csv.reader(result.stdout.splitlines()))
    if len(rows) != 2 or tuple(rows[0]) != HEADER[1:]:
        raise RuntimeError(f"unexpected probe output from {command[0]}")
    return rows[1]


def main() -> int:
    args = parse_args()
    if min(args.trials, args.one_mib_iterations, args.five_mib_iterations) < 1:
        raise SystemExit("trial and iteration counts must be positive")

    root = Path(__file__).resolve().parents[2]
    rust_probe = root / "spikes/feas002/target/release/markdown-parser-probe"
    cmark_probe = root / "build/feas002/cmark-probe"
    corpus = root / "build/fixtures/performance"
    missing = [path for path in (rust_probe, cmark_probe, corpus) if not path.exists()]
    if missing:
        raise SystemExit("missing build inputs; run spikes/feas002/build.sh and "
                         "tools/generate_performance_fixtures.py")

    writer = csv.writer(sys.stdout, lineterminator="\n")
    writer.writerow(HEADER)
    for path in sorted(corpus.glob("*.md")):
        iterations = (
            args.five_mib_iterations if "5mib" in path.name
            else args.one_mib_iterations
        )
        relative = path.relative_to(root)
        for trial in range(1, args.trials + 1):
            for engine in ENGINES:
                if engine == "cmark-gfm":
                    command = [str(cmark_probe), str(relative), str(iterations)]
                else:
                    command = [
                        str(rust_probe), engine, str(relative), str(iterations)
                    ]
                writer.writerow([trial, *run(command, root)])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

