#!/usr/bin/env python3
"""Inspect the production reader with generated public text, never user documents.

Runs on the current display in a private D-Bus session with a temporary AT-SPI
registry. Does not change accessibility settings or launch Orca. A successful
inspection is evidence, not the owner-confirmed LNX-001D1 screen-reader gate.
Exit 0: inspection completed (inspect gate_status); exit 2: inspection unavailable.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
HEADINGS = 1000


def fixture() -> str:
    return "\n\n".join(
        f"# Gate heading {i:04d}\n\n"
        f"Gate paragraph {i:04d}. Read this paragraph in document order."
        for i in range(HEADINGS)) + "\n"


def walk(node, depth=0):
    if depth > 16:
        raise RuntimeError("unexpected accessible tree depth")
    yield node
    for index in range(node.get_child_count()):
        child = node.get_child_at_index(index)
        if child is not None:
            yield from walk(child, depth + 1)


def inspect(binary: Path) -> dict:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, Gio, GLib

    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    address = bus.call_sync(
        "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus", "GetAddress",
        None, None, Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
    env = dict(os.environ, AT_SPI_BUS_ADDRESS=address, GTK_A11Y="atspi")
    # The child's libatspi and GTK must discover the same private registry.
    os.environ["AT_SPI_BUS_ADDRESS"] = address
    registry = subprocess.Popen(
        ["/usr/lib/at-spi2-registryd"], env=env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    app = None
    try:
        with tempfile.TemporaryDirectory(prefix="markdown-a11y-") as directory:
            document = Path(directory) / "gate.md"
            document.write_text(fixture(), encoding="utf-8")
            app = subprocess.Popen(
                [str(binary), str(document)], env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            deadline = time.monotonic() + 15
            nodes = []
            while time.monotonic() < deadline:
                if app.poll() is not None:
                    raise RuntimeError("reader exited before inspection")
                desktop = Atspi.get_desktop(0)
                for index in range(desktop.get_child_count()):
                    candidate = desktop.get_child_at_index(index)
                    if candidate and candidate.get_process_id() == app.pid:
                        nodes = list(walk(candidate))
                if any(n.get_role() == Atspi.Role.PARAGRAPH for n in nodes):
                    break
                time.sleep(0.2)
            else:
                raise RuntimeError("reader text not available through AT-SPI")

            headings = [n for n in nodes if n.get_role() == Atspi.Role.HEADING]
            paragraphs = [n for n in nodes if n.get_role() == Atspi.Role.PARAGRAPH]
            document_roles = {getattr(Atspi.Role, name) for name in dir(Atspi.Role)
                              if name.startswith("DOCUMENT_")}
            documents = [n for n in nodes if n.get_role() in document_roles]
            content = [n for n in nodes if n.get_role() in
                       (Atspi.Role.HEADING, Atspi.Role.PARAGRAPH)]
            expected = [text for i in range(HEADINGS) for text in (
                f"Gate heading {i:04d}",
                f"Gate paragraph {i:04d}. Read this paragraph in document order.")]
            texts = [Atspi.Text.get_text(n, 0, -1) for n in content]
            first = expected.index(texts[0]) if texts and texts[0] in expected else -1
            ordered = first >= 0 and texts == expected[first:first + len(texts)]
            levels = all(n.get_attributes().get("level") == "1" for n in headings)
            paragraph = paragraphs[0]
            focus_ok = False
            focus_rejected = False
            try:
                focused = Atspi.Component.grab_focus(paragraph)
                # Observe focus, not merely a successful method return.
                # Only the disposable test app is touched.
                time.sleep(0.1)
                focus_ok = focused and paragraph.get_state_set().contains(Atspi.StateType.FOCUSED)
            except GLib.Error:
                focus_rejected = True
            selection_ok = False
            selection_rejected = False
            try:
                selected = Atspi.Text.add_selection(paragraph, 0, 4)
                if selected:
                    selection = Atspi.Text.get_selection(paragraph, 0)
                    selection_ok = (selection.start_offset == 0 and selection.end_offset == 4)
                    Atspi.Text.remove_selection(paragraph, 0)
            except GLib.Error:
                selection_rejected = True
            return {
                "inspection": "completed",
                "session": "private D-Bus, temporary registry, current display",
                "generated_headings": HEADINGS,
                "generated_paragraphs": HEADINGS,
                "document_roles": [n.get_role_name() for n in documents],
                "lists": [{"name": n.get_name(), "children": n.get_child_count()}
                          for n in nodes if n.get_role() == Atspi.Role.LIST],
                "exposed_headings": len(headings),
                "exposed_paragraphs": len(paragraphs),
                "distant_heading_exposed": any(
                    n.get_name() == f"Gate heading {HEADINGS - 1:04d}" for n in headings),
                "exposed_text_in_model_order": ordered,
                "heading_levels_correct": bool(headings) and levels,
                "basic_selection": selection_ok,
                "selection_operation_rejected": selection_rejected,
                "paragraph_focus": bool(focus_ok),
                "focus_operation_rejected": focus_rejected,
                "gate_status": "not proven; owner Orca confirmation required",
                "structural_failure": not documents,
                "orca_heading_navigation": "not run",
                "orca_continuous_reading": "not run",
            }
    finally:
        for process in (app, registry):
            if process is not None:
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configuration", choices=("debug", "release"), default="release")
    parser.add_argument("--private-child", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--prepare-only", action="store_true",
                        help="write the public owner-test document under build/linux/accessibility-gate")
    args = parser.parse_args()
    if args.prepare_only:
        destination = ROOT / "build/linux/accessibility-gate/fixture.md"
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists() and destination.read_text(encoding="utf-8") != fixture():
            raise SystemExit("refusing to overwrite a different test document")
        destination.write_text(fixture(), encoding="utf-8")
        print(destination.relative_to(ROOT))
        return 0
    if not args.private_child:
        # A fresh session avoids collisions with the user's running application
        # and works even when the desktop's accessibility bus has gone stale.
        process = subprocess.Popen(
            ["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()),
             "--private-child", "--configuration", args.configuration],
            stderr=subprocess.DEVNULL, start_new_session=True)
        try:
            return process.wait(timeout=40)
        except subprocess.TimeoutExpired:
            print(json.dumps({"inspection": "unavailable", "reason": "probe timed out"}))
            return 2
        finally:
            # Include activated helpers belonging to this disposable session.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    try:
        binary = ROOT / "build/linux/target" / args.configuration / "markdown-linux"
        print(json.dumps(inspect(binary), indent=2))
        return 0
    except Exception:
        # Errors may contain private bus addresses or paths; do not print them.
        print(json.dumps({"inspection": "unavailable",
                          "reason": "AT-SPI, reader, display or registry unavailable"}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
