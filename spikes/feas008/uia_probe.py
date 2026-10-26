#!/usr/bin/env python3
"""FEAS-008 UIA3 inspection of a running Windows host, the Windows analogue
of atspi_probe.py. Launches the host in control mode, jumps to the top,
middle and end of the document, and records the rows UI Automation exposes
there with UiaProbe.exe. Exposed rows are checked against the model's own
fragment order. Usage: uia_probe.py [ENGINE] [DOCUMENT] [--no-item-walk]"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "build/feas008/windows"
HOSTS = {"csharp-aot": OUT / "csharp-aot/Feas008WinUIAot.exe", "csharp-jit": OUT / "csharp-jit/Feas008WinUI.exe",
         "cpp": OUT / "cpp/Feas008Cpp.exe"}
PROBE = OUT / "uia/UiaProbe.exe"


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    engine = args[0] if args else "csharp-aot"
    document = Path(args[1]) if len(args) > 1 else ROOT / "build/fixtures/performance/prose-5mib.md"
    proc = subprocess.Popen([str(HOSTS[engine]), str(document), "--control"], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, text=True, encoding="utf-8")

    def command(line: str, until: str) -> dict:
        if line:
            proc.stdin.write(line + "\n")
            proc.stdin.flush()
        for raw in proc.stdout:
            if raw.startswith("EVENT "):
                event = json.loads(raw[6:])
                if event["event"] in (until, "error"):
                    return event
        raise RuntimeError("host exited")

    result = {"engine": engine, "document": document.name, "positions": []}
    try:
        command("", "ready")
        items = command("texts 0 1", "texts")["items"]
        for label, item in (("top", 0), ("middle", items // 2), ("end", items - 1)):
            command(f"focus {item}", "presented")
            # Walking every item through ItemContainer is bounded: on 121k items
            # each FindItemByProperty step realizes a peer in the host.
            limit = "0" if "--no-item-walk" in sys.argv else "2000"
            probe = subprocess.run([str(PROBE), str(proc.pid), "80", limit], capture_output=True,
                                   text=True, encoding="utf-8", timeout=240)
            data = json.loads(probe.stdout)
            names = [r["name"] for r in data.get("rows", []) if r["type"] == "list item"]
            # Match the exposed rows to a contiguous run of model fragments.
            window = command(f"texts {max(0, item - 120)} {min(items, item + 120)}", "texts")
            fragments = [t[:60] for t in window["texts"]]
            start = next((i for i in range(len(fragments)) if fragments[i:i + len(names)] == names), None)
            data["contiguous_model_order"] = start is not None and len(names) > 0
            data["first_exposed_item"] = None if start is None else window["from"] + start
            data["exposed_list_items"] = len(names)
            data["headings"] = [r for r in data.get("rows", []) if r["child_heading_level"] > 0][:3]
            data["rows"] = data.get("rows", [])[:4]
            result["positions"].append({"label": label, "item": item, **data})
    finally:
        proc.stdin.write("quit\n")
        proc.stdin.flush()
        try:
            proc.wait(20)
        except subprocess.TimeoutExpired:
            proc.kill()
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
