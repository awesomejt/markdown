#!/usr/bin/env python3
"""Compare FEAS-010 extraction from both engines with the frozen expectations.

Prints one line per case and engine, then a JSON summary on request. Checks
that need an engine or a renderer (diagram validity, HTML policy, filesystem
semantics) are listed as not evaluated rather than passed.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures"
PROBE = ROOT / "spikes/feas010/target/release/feas010-extensions"
ENGINES = ("pulldown", "comrak")


def extract(engine: str, path: Path) -> dict:
    out = subprocess.run([str(PROBE), "extract", engine, str(path)], check=True, capture_output=True, text=True)
    return json.loads(out.stdout)


def compare(case: dict, expected: dict, got: dict, source: bytes) -> tuple[list[str], list[str]]:
    fails: list[str] = []
    skipped: list[str] = []

    def expect(name: str, want, have) -> None:
        if want != have:
            fails.append(f"{name}: expected {want!r}, got {have!r}")

    doc = expected["document"]
    fm = doc["front_matter"]
    expect("front_matter.present", fm["present"], got["front_matter"]["present"])
    if fm["present"]:
        expect("front_matter.properties", fm["properties"], got["front_matter"]["properties"])
    expect("outline", doc["outline"], [{k: h[k] for k in ("level", "text", "slug")} for h in got["outline"]])
    for key, want in doc["feature_counts"].items():
        expect(f"feature_counts.{key}", want, got["feature_counts"].get(key))

    exp = expected["expectations"]
    if "code_fences" in exp:
        expect("code_fences", [{k: f[k] for k in ("language", "semantic")} for f in exp["code_fences"]],
               [{k: f[k] for k in ("language", "semantic")} for f in got["code_fences"]])
        if any("valid" in f for f in exp["code_fences"]):
            skipped.append("code_fences.valid needs a diagram engine (FEAS-009)")
    if "links" in exp:
        expect("links", exp["links"], [{k: l[k] for k in ("target", "kind")} for l in got["links"]])
    if "security" in exp:
        skipped.append("security policy and invariants belong to the renderer")

    ext = exp.get("extensions", {})
    if "alerts" in ext:
        expect("extensions.alerts", ext["alerts"], got["alerts"])
    if "non_alert_marker" in ext:
        marker = ext["non_alert_marker"].encode()
        start = source.find(marker)
        expect("non_alert_marker exists", True, start >= 0)
        # The look-alike must not be counted: alerts equal the valid markers only.
    if "math" in ext:
        counts = got["feature_counts"]
        expect("extensions.math", ext["math"], {
            "inline": counts["math_inline"], "display": counts["math_display"], "fences": counts["math_fences"]})
    if "diagram_engines" in ext:
        expect("extensions.diagram_engines", sorted(ext["diagram_engines"]),
               sorted({f["language"] for f in got["code_fences"] if f["semantic"] == "diagram"}))
    if "path_semantics" in ext:
        skipped.append("path_semantics belong to platform filesystem tests")

    voice = got.get("voice")
    if ext.get("mode") == "script":
        if voice is None:
            fails.append("voice: script mode not detected")
        else:
            fails.extend(check_voice(ext, voice, got, source))
    elif voice is not None:
        fails.append("voice: syntax is active without the script opt-in")
    fails.extend(f"span check: {f}" for f in got["span_checks"]["failures"])
    return fails, skipped


def check_voice(ext: dict, v: dict, got: dict, source: bytes) -> list[str]:
    fails = []
    if ext["speakers"] != v["speakers"]:
        fails.append(f"voice.speakers: expected {ext['speakers']}, got {v['speakers']}")
    text = lambda line: source[line["span"][0]:line["span"][1]].decode()
    lines = v["lines"]
    if ext.get("speaker_carry_over"):
        carried = [l for l in lines if not l["labelled"] and l["speaker"]]
        if not any(text(l).startswith("A second paragraph") and l["speaker"] == "ANNA" for l in carried):
            fails.append("voice.speaker_carry_over: unlabelled follow-on paragraph lost its speaker")
    if ext.get("heading_resets_speaker"):
        for h in got["outline"][1:]:
            after = [l for l in lines if l["span"][0] >= h["span"][1]]
            if after and after[0]["speaker"] is not None and not after[0]["labelled"]:
                fails.append(f"voice.heading_resets_speaker: speaker carried past heading {h['text']!r}")
    forms = sorted({d["form"] for d in v["directions"]})
    if sorted(ext.get("direction_forms", [])) != forms:
        fails.append(f"voice.direction_forms: expected {ext.get('direction_forms')}, got {forms}")
    if ext.get("note_form") and not all(n["form"] == ext["note_form"] for n in v["notes"]) or not v["notes"]:
        fails.append("voice.note_form")
    if ext.get("pronunciation_form") == "brace-pipe" and not v["pronunciations"]:
        fails.append("voice.pronunciation_form")
    if ext.get("av_table") and v["av_tables"] < 1:
        fails.append("voice.av_table not detected")
    if ext.get("source_line_numbers") == "required":
        for l in lines:
            if l["line"] != source[: l["span"][0]].count(b"\n") + 1:
                fails.append(f"voice.source_line_numbers wrong for span {l['span']}")
    # Visual A/V cells and directions are never spoken text.
    for l in lines:
        if text(l).startswith(("Wide shot", "Close-up")):
            fails.append("voice: a Visual A/V cell was treated as spoken")
    return fails


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", type=Path, help="write a summary here")
    args = parser.parse_args()
    manifest = yaml.safe_load((FIXTURES / "manifest.yaml").read_text(encoding="utf-8"))
    summary = {}
    failed = False
    for case in manifest["cases"]:
        path = FIXTURES / case["input"]
        expected = yaml.safe_load((FIXTURES / case["expected"]).read_text(encoding="utf-8"))
        source = path.read_bytes()
        for engine in ENGINES:
            got = extract(engine, path)
            fails, skipped = compare(case, expected, got, source)
            failed |= bool(fails)
            summary.setdefault(case["id"], {})[engine] = {
                "pass": not fails, "failures": fails, "not_evaluated": skipped,
                "span_checks": got["span_checks"]["checked"]}
            status = "PASS" if not fails else "FAIL"
            print(f"{case['id']:18} {engine:9} {status}  spans={got['span_checks']['checked']}")
            for f in fails:
                print(f"    - {f}")
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
