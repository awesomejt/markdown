#!/usr/bin/env python3
"""Reconcile fixture expectations into the parser conformance contract.

Every expectation in fixtures/expected/*.yaml is either mapped to a named check
that core/crates/markdown-core/tests/conformance.rs runs, or deferred with a
reason. An unclassified key or invariant fails, so new fixture expectations
force an explicit decision. `--check` verifies the committed JSON is current.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "fixtures"
OUTPUT = ROOT / "specs" / "parsing" / "conformance" / "corpus.json"

# feature_counts keys the Rust test counts (see `count` in conformance.rs).
COUNTED = {
    "headings", "tables", "task_items", "alerts", "blockquotes", "code_fences",
    "footnote_definitions", "math_inline", "math_display", "math_fences",
    "diagram_fences", "raw_html_constructs", "links",
}
VOICE = "Voice-over interpretation is not implemented; script opt-in is checked as an info diagnostic and the syntax reads as ordinary Markdown (CORE-003B3)."
DIAGRAM = "Diagram engines are advanced work (ADV); diagram fences are inert source fallbacks with a diagnostic."
PLATFORM = "Filesystem lookup, normalization and display names are platform-adapter behavior, outside the parser."
DEFERRED_COUNTS = {"notes": VOICE, "pronunciation_guides": VOICE}
# Extension keys: a check token, or a deferral reason.
EXTENSIONS: dict[str, tuple[str, str]] = {
    "alerts": ("check", "alerts"),
    "non_alert_marker": ("check", "non_alert_marker"),
    "math": ("check", "math"),
    "mode": ("check", "voice_mode"),
    "diagram_engines": ("defer", DIAGRAM),
    "path_semantics": ("defer", PLATFORM),
    **{k: ("defer", VOICE) for k in (
        "speakers", "speaker_carry_over", "heading_resets_speaker", "direction_forms",
        "note_form", "pronunciation_form", "av_table", "source_line_numbers")},
}
STRICTER_HTML = "The core keeps every raw HTML construct as inert source text, which is stricter than this policy; no safe subset or sanitizer output exists."
# Free-text invariants: a check token, or a deferral reason.
INVARIANTS: dict[str, tuple[str, str]] = {
    "Invalid math produces a bounded diagnostic and preserves readable source.": ("check", "fallback_diagnostics"),
    "Collapsible HTML cannot execute scripts or event handlers.": ("check", "html_inert"),
    "Unknown languages remain readable plain code.": ("check", "code_fences"),
    "Highlighting failure never hides or changes source text.": ("defer", "Syntax highlighting is lazy native presentation work; the core stores literal code text only."),
    "Diagram compilation is offline and cannot access arbitrary files or URLs.": ("defer", DIAGRAM),
    "Generated SVG cannot execute scripts or load external resources.": ("defer", DIAGRAM),
    "A diagram failure preserves source and a bounded diagnostic.": ("check", "fallback_diagnostics"),
    "Normalization fallback cannot escape the approved workspace.": ("defer", PLATFORM),
    "The displayed filename round-trips without replacement characters.": ("defer", PLATFORM),
    "Script and iframe content cannot execute.": ("check", "html_inert"),
    "Event-handler attributes are removed or never represented.": ("check", "html_inert"),
    "A blocked link cannot be activated.": ("check", "blocked_links"),
    "Safe visible text remains accessible where the selected policy permits it.": ("check", "html_inert"),
    "Voice-over syntax is inert unless front matter opts into script mode.": ("check", "voice_mode"),
    "Notes and directions never execute or resolve as links.": ("check", "no_links"),
}


def load(path: Path) -> Any:
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def reconcile(case_id: str, input_path: str, case: dict[str, Any]) -> dict[str, Any]:
    doc, exp = case["document"], case["expectations"]
    checks: set[str] = {"outline", "front_matter", "counts", "html_inert", "blocked_links"}
    deferred: list[dict[str, str]] = []

    def defer(path: str, reason: str) -> None:
        deferred.append({"path": path, "reason": reason})

    counts = {}
    for key, value in doc["feature_counts"].items():
        if key in COUNTED:
            counts[key] = value
        elif key in DEFERRED_COUNTS:
            defer(f"document.feature_counts.{key}", DEFERRED_COUNTS[key])
        else:
            raise SystemExit(f"{case_id}: unclassified feature count {key!r}")
    out: dict[str, Any] = {
        "id": case_id,
        "input": f"fixtures/{input_path}",
        "front_matter": doc["front_matter"],
        "outline": doc["outline"],
        "counts": counts,
    }
    fences = exp.get("code_fences")
    if fences is not None:
        checks.add("code_fences")
        out["code_fences"] = [{"language": f["language"], "semantic": f["semantic"]} for f in fences]
        if any("valid" in f for f in fences):
            defer("expectations.code_fences[].valid", DIAGRAM)
    if "links" in exp:
        checks.add("links")
        out["links"] = exp["links"]
    security = exp.get("security", {})
    out["blocked_url_schemes"] = security.get("blocked_url_schemes", [])
    policy = security.get("raw_html_policy", "text")
    if policy != "text":
        defer("expectations.security.raw_html_policy", f"{policy}: {STRICTER_HTML}")
    for text in security.get("invariants", []):
        if text not in INVARIANTS:
            raise SystemExit(f"{case_id}: unclassified invariant {text!r}")
        kind, value = INVARIANTS[text]
        if kind == "check":
            checks.add(value)
        else:
            defer(f"expectations.security.invariants: {text}", value)
    extensions = exp.get("extensions", {})
    for key, value in extensions.items():
        if key not in EXTENSIONS:
            raise SystemExit(f"{case_id}: unclassified extension {key!r}")
        kind, token = EXTENSIONS[key]
        if kind == "defer":
            defer(f"expectations.extensions.{key}", token)
            continue
        checks.add(token)
        out[token] = value
    out["checks"] = sorted(checks)
    out["deferred"] = deferred
    return out


def build() -> dict[str, Any]:
    manifest = load(FIXTURES / "manifest.yaml")
    cases = [
        reconcile(c["id"], c["input"], load(FIXTURES / c["expected"]))
        for c in manifest["cases"]
    ]
    profiles = [
        {k: p[k] for k in ("id", "generator", "target_bytes", "sha256")}
        | {"line_bytes": p.get("line_bytes", 0)}
        for p in load(FIXTURES / "performance" / "profiles.yaml")["profiles"]
    ]
    return {
        "version": 1,
        "generated_by": "tools/generate_conformance.py",
        "sources": ["fixtures/manifest.yaml", "fixtures/expected/*.yaml", "fixtures/performance/profiles.yaml"],
        "cases": cases,
        "profiles": profiles,
    }


def render() -> str:
    return json.dumps(build(), indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if the committed JSON is stale")
    args = parser.parse_args()
    text = render()
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != text:
            print(f"{OUTPUT.relative_to(ROOT)} is stale; run tools/generate_conformance.py", file=sys.stderr)
            return 1
        print(f"conformance contract current: {len(build()['cases'])} cases")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
