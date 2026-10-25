#!/usr/bin/env python3
"""Materialize Unicode filenames independently of Git's checkout behavior."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import tempfile
import unicodedata
from pathlib import Path
from typing import Any

import yaml


ROOT = Path(__file__).resolve().parents[1]


def safe_component(name: str) -> str:
    """Reject separators and Windows-special names even on POSIX hosts."""
    if (
        not name
        or name in {".", ".."}
        or any(char in name for char in '/\\:<>"|?*')
        or any(ord(char) < 32 for char in name)
        or name.endswith((".", " "))
        or name.split(".")[0].upper()
        in {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)),
            *(f"LPT{i}" for i in range(1, 10))}
    ):
        raise ValueError(f"unsafe filename component: {name!r}")
    return name


def materialize_paths(fixtures: Path, destination: Path) -> list[dict[str, Any]]:
    """Write into a fresh directory; never overwrite an existing fixture."""
    fixtures = fixtures.resolve()
    manifest = yaml.safe_load((fixtures / "manifest.yaml").read_text(encoding="utf-8"))
    records = []
    for case in manifest["cases"]:
        spec = case.get("runtime_path")
        if spec is None:
            continue
        source = (fixtures / case["input"]).resolve()
        if not source.is_relative_to(fixtures):
            raise ValueError("path fixture source escapes fixtures")
        content = source.read_bytes()
        digest = hashlib.sha256(content).hexdigest()
        if digest != case["sha256"]:
            raise ValueError(f"{case['id']}: path fixture source hash mismatch")
        case_id = safe_component(case["id"])
        for form in spec["normalizations"]:
            if form not in {"NFC", "NFD"}:
                raise ValueError(f"unsupported normalization: {form!r}")
            filename = safe_component(unicodedata.normalize(form, spec["filename"]))
            # Separate directories avoid assuming NFC/NFD names can coexist.
            folder = destination / case_id / form
            folder.mkdir(parents=True, exist_ok=False)
            requested = folder / filename
            with requested.open("xb") as handle:
                handle.write(content)
            stored_files = list(folder.iterdir())
            if len(stored_files) != 1 or requested.read_bytes() != content:
                raise ValueError(f"{case_id}: generated path did not round-trip")
            stored = stored_files[0]
            if stored.read_bytes() != content:
                raise ValueError(f"{case_id}: directory entry bytes differ")
            alternate_form = "NFD" if form == "NFC" else "NFC"
            alternate = folder / unicodedata.normalize(alternate_form, filename)
            alternate_exists = alternate.exists()
            if alternate_exists and not alternate.samefile(requested):
                raise ValueError(f"{case_id}: canonical lookup resolved another file")
            records.append({
                "case": case_id,
                "normalization": form,
                "path": str(requested.resolve()),
                "requested_name": filename,
                "stored_name": stored.name,
                "stored_codepoints": [f"U+{ord(char):04X}" for char in stored.name],
                "canonical_equivalent_lookup": alternate_exists,
                "sha256": digest,
            })
    return records


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "build/fixtures/paths")
    parser.add_argument("--check", action="store_true", help="Remove generated files after checking")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.check:
        with tempfile.TemporaryDirectory(prefix="check-", dir=args.output) as temporary:
            records = materialize_paths(ROOT / "fixtures", Path(temporary))
    else:
        run_directory = Path(tempfile.mkdtemp(prefix="run-", dir=args.output))
        records = materialize_paths(ROOT / "fixtures", run_directory)
        report = {"host": platform.platform(), "cases": records}
        (run_directory / "paths.json").write_text(
            json.dumps(report, ensure_ascii=True, indent=2) + "\n", encoding="utf-8"
        )
    print(json.dumps({"host": platform.platform(), "temporary": args.check, "cases": records},
                     ensure_ascii=True, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
