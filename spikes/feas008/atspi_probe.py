#!/usr/bin/env python3
"""Inspect the FEAS-008 native renderer through the live AT-SPI bus.

The app runs in control mode. The probe walks its accessible tree, checks that
the exposed rows are the model fragments in reading order, records roles,
heading levels and image names, then jumps to the middle and the end and
checks again. It cannot judge what a screen reader announces; Orca is checked
by the owner.
"""

import json
import subprocess
import sys
import time
from pathlib import Path

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
NATIVE = ROOT / "spikes/feas008/target/release/feas008-native"


class App:
    def __init__(self, document: Path):
        self.proc = subprocess.Popen([str(NATIVE), str(document), "--control"], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.wait("ready")

    def send(self, line: str, until: str) -> dict:
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()
        return self.wait(until)

    def wait(self, name: str) -> dict:
        for line in self.proc.stdout:
            if line.startswith("EVENT "):
                event = json.loads(line[6:])
                if event.get("event") == name:
                    return event
        raise RuntimeError(f"app exited before {name}")

    def close(self):
        self.proc.stdin.write("quit\n")
        self.proc.stdin.flush()
        self.proc.communicate(timeout=30)


def find_app(pid: int, timeout: float = 10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        desktop = Atspi.get_desktop(0)
        for i in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(i)
            if app is not None and app.get_process_id() == pid:
                return app
        time.sleep(0.2)
    raise RuntimeError("application not on the AT-SPI bus")


def walk(node, visit, depth=0):
    visit(node, depth)
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child is not None:
            walk(child, visit, depth + 1)


def text_of(node) -> str | None:
    iface = node.get_text_iface()
    if iface is None:
        return None
    return Atspi.Text.get_text(node, 0, Atspi.Text.get_character_count(node))


def document_list(app):
    found = []
    walk(app, lambda n, d: found.append(n) if n.get_role() == Atspi.Role.LIST and n.get_name().startswith("Document") else None)
    if not found:
        raise RuntimeError("document list not exposed")
    return found[0]


def rows(listing) -> list[dict]:
    out = []
    for i in range(listing.get_child_count()):
        item = listing.get_child_at_index(i)
        content = []
        walk(item, lambda n, d: content.append(n) if d > 0 else None)
        shown = [c for c in content if c.get_state_set().contains(Atspi.StateType.SHOWING)
                 or c.get_role() in (Atspi.Role.HEADING, Atspi.Role.PARAGRAPH, Atspi.Role.IMAGE)]
        roles = [c.get_role_name() for c in shown]
        attrs = {}
        for c in shown:
            try:
                attrs.update(c.get_attributes() or {})
            except Exception:
                pass
        texts = [t for t in (text_of(c) for c in shown) if t]
        out.append({
            "role": item.get_role_name(),
            "name": item.get_name(),
            "description": item.get_description(),
            "child_roles": roles,
            "level": attrs.get("level"),
            "image_names": [c.get_name() for c in shown if c.get_role() == Atspi.Role.IMAGE],
            "text": texts[0] if texts else None,
            "showing": item.get_state_set().contains(Atspi.StateType.SHOWING),
        })
    return out


def check_order(app: App, exposed: list[dict], near: int) -> dict:
    """Exposed row names must be a contiguous, ordered run of model fragments
    beginning within 400 items of `near` (generated inputs repeat text)."""
    names = [r["name"] for r in exposed]
    lo = max(0, near - 400)
    model = app.send(f"texts {lo} {near + 400 + len(names)}", "texts")["texts"]
    starts = [lo + i for i in range(len(model)) if model[i:i + len(names)] == names]
    ordered = bool(starts) and bool(names)
    start = min(starts, key=lambda s: abs(s - near)) if starts else None
    label_text_matches = all(r["text"] == r["name"] for r in exposed if r["text"] is not None)
    return {"contiguous_model_order": ordered, "first_model_item": start, "exposed_rows": len(names),
            "label_text_equals_row_name": label_text_matches}


def probe(document: Path, jumps: list[str]) -> dict:
    app = App(document)
    try:
        accessible = find_app(app.proc.pid)
        result = {"document": str(document.relative_to(ROOT)), "toolkit": accessible.get_toolkit_name(),
                  "toolkit_version": accessible.get_toolkit_version(), "views": []}
        for jump in ["top", *jumps]:
            near = 0
            if jump != "top":
                near = app.send(jump, "presented")["item"]
                time.sleep(0.6)
            listing = document_list(accessible)
            exposed = rows(listing)
            view = {"after": jump, "target_item": near, **check_order(app, exposed, near)}
            roles = {}
            for r in exposed:
                key = r["description"] or r["role"]
                roles[key] = roles.get(key, 0) + 1
            view["row_descriptions"] = roles
            view["headings"] = [{"name": r["name"], "level": r["level"], "roles": r["child_roles"]}
                                for r in exposed if "heading" in r["child_roles"]][:4]
            view["images"] = [{"name": r["name"], "image_names": r["image_names"]}
                              for r in exposed if r["image_names"]][:4]
            view["list_name"] = listing.get_name()
            view["list_child_count"] = listing.get_child_count()
            result["views"].append(view)
        return result
    finally:
        app.close()


def main() -> int:
    cases = [
        (ROOT / "fixtures/markdown/foundation/gfm-tour.md", []),
        (ROOT / "build/feas008/media/media-links.md", []),
        (ROOT / "build/fixtures/performance/prose-5mib.md", ["item 60000", "item 121361"]),
    ]
    out = [probe(doc, jumps) for doc, jumps in cases]
    print(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
