#!/usr/bin/env python3
"""Generate deterministic Markdown performance fixtures from YAML profiles."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
from typing import Any

import yaml


def fill_to_size(seed: bytes, target_bytes: int) -> bytes:
    if target_bytes <= 0:
        raise ValueError("target_bytes must be positive")
    repeats, remainder = divmod(target_bytes, len(seed))
    return seed * repeats + seed[:remainder]


def generate_profile(profile: dict[str, Any]) -> bytes:
    generator = profile["generator"]
    target_bytes = int(profile["target_bytes"])
    if generator == "prose-sections-v1":
        seed = (
            b"## Generated section\n\n"
            b"A representative paragraph has links, **emphasis**, `code`, Unicode cafe\xcc\x81, "
            b"and enough ordinary prose to exercise parsing and native text layout.\n\n"
            b"- first item\n- second item\n- [ ] pending item\n\n"
        )
    elif generator == "delimiter-storm-v1":
        seed = (
            b"# Hostile delimiters\n\n"
            b"[unclosed **emphasis _and `code <tag attr='value'> ((({{{[[[\\\\\n"
        )
    elif generator == "long-lines-v1":
        prefix = b"# Long lines\n\n"
        line_bytes = int(profile.get("line_bytes", 131072))
        line = b"x" * max(1, line_bytes - 1) + b"\n"
        return (prefix + fill_to_size(line, target_bytes - len(prefix)))[:target_bytes]
    else:
        raise ValueError(f"unknown generator: {generator}")
    return fill_to_size(seed, target_bytes)


def load_profiles(path: Path) -> list[dict[str, Any]]:
    payload = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict) or payload.get("version") != 1:
        raise ValueError("performance profile file must have version: 1")
    profiles = payload.get("profiles")
    if not isinstance(profiles, list):
        raise ValueError("performance profile file must contain profiles")
    return profiles


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profiles", type=Path, default=Path("fixtures/performance/profiles.yaml")
    )
    parser.add_argument("--output", type=Path, default=Path("build/fixtures/performance"))
    parser.add_argument("--check", action="store_true", help="Verify metadata without writing")
    args = parser.parse_args()

    for profile in load_profiles(args.profiles):
        content = generate_profile(profile)
        digest = hashlib.sha256(content).hexdigest()
        if len(content) != profile["target_bytes"]:
            raise SystemExit(f"{profile['id']}: generated size mismatch")
        expected = profile.get("sha256")
        if expected and digest != expected:
            raise SystemExit(f"{profile['id']}: sha256 mismatch: {digest}")
        if not args.check:
            args.output.mkdir(parents=True, exist_ok=True)
            (args.output / profile["filename"]).write_bytes(content)
        print(f"{profile['id']}: {len(content)} bytes sha256={digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
