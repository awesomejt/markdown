#!/usr/bin/env python3
"""Check bounded probe outcomes, including expected unsafe raw-engine output."""
import json
from pathlib import Path
import sys

path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("build/feas009/measurements-macos.json")
data = json.loads(path.read_text())
count = 0
for name, engine in data["engines"].items():
    assert len(engine["runs"]) == 5
    for run in engine["runs"]:
        cases = {case["name"]: case for case in run["output"]["results"]}
        for case in cases.values():
            assert len(case["samples_ms"]) == 5
            assert all(0 <= sample < 30000 for sample in case["samples_ms"])
            count += 1
        assert cases["invalid"]["error"]
        if name == "katex":
            for key in ("inline", "display", "integral"):
                assert not cases[key]["error"] and cases[key]["has_mathml"]
            assert cases["macro-loop"]["error"]
            assert not cases["unsafe-link"]["has_javascript_href"]
            assert not cases["remote-image"]["has_remote_image"]
        elif name == "mermaid":
            for key in ("flowchart", "sequence"):
                assert not cases[key]["error"] and cases[key]["svg_valid"]
            assert all(not case["scripts"] for case in cases.values())
            assert all(not case["foreign_objects"] for case in cases.values())
            assert all(not case["unsafe_attributes"] for case in cases.values())
            # These sampled attribute checks are not a complete sanitizer.
        elif name == "d2":
            assert cases["flowchart"]["svg_present"] and not cases["flowchart"]["error"]
            assert cases["sequence"]["svg_present"] and not cases["sequence"]["error"]
            assert cases["import-denied"]["error"]
            # These intentionally document why a separate output policy is required.
            assert cases["remote-icon"]["has_remote_reference"]
            assert cases["unsafe-link"]["has_javascript_reference"]
print(f"Checked {count} case outcomes in 15 processes; expected raw-output hazards retained")
