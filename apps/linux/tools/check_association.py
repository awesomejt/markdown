#!/usr/bin/env python3
"""Exercise a development desktop entry on a private bus and temporary XDG data.

No installation, MIME default update or persistent desktop registration occurs.
"""
import argparse
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time

APP_ROOT = Path(__file__).resolve().parents[1]


def quoted_exec(path: Path) -> str:
    # Desktop Entry Exec escaping (not shell quoting). Percent is a field code
    # even inside quotes. Backslashes need a second escape through the entry's
    # general string unescaping before Exec argument parsing.
    value = str(path).replace("%", "%%")
    value = value.replace("\\", "\\\\").replace('"', '\\"').replace("`", "\\`").replace("$", "\\$")
    return '"' + value.replace("\\", "\\\\") + '"'


def session(binary: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="markdown-association-") as scratch:
        root = Path(scratch)
        environment = os.environ.copy()
        for name in ("DATA", "CONFIG", "CACHE"):
            directory = root / name.lower()
            directory.mkdir()
            environment[f"XDG_{name}_HOME"] = str(directory)
        applications = root / "data" / "applications"
        applications.mkdir()
        # Exercise Exec paths that require desktop-entry escaping, without a shell.
        executable = root / 'reader space $ ` " link'
        executable.symlink_to(binary)
        template = (APP_ROOT / "resources" / "io.github.awesomejt.markdown.desktop.in").read_text()
        entry = applications / "io.github.awesomejt.markdown.desktop"
        entry.write_text(template.replace("@EXEC@", quoted_exec(executable)))
        first = root / "startup 文 space.md"
        one = root / "desktop one 文.any"
        # Keep a non-UTF-8 Linux filename all the way through Exec %F and Gio.
        two = root / os.fsdecode(b"desktop two \xff.md")
        first.write_bytes(b"# First delivery\n")
        one.write_bytes(b"# Desktop one\n")
        two.write_bytes(b"# Desktop two\n")
        process = subprocess.Popen([str(binary), "--association-test", str(first)],
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   env=environment, start_new_session=True)
        output = bytearray()
        try:
            selector = selectors.DefaultSelector()
            selector.register(process.stdout, selectors.EVENT_READ)
            deadline = time.monotonic() + 15
            while b"association-smoke: ready" not in output:
                if time.monotonic() >= deadline or process.poll() is not None:
                    raise RuntimeError("primary did not start association smoke")
                for key, _ in selector.select(timeout=0.2):
                    output.extend(os.read(key.fileobj.fileno(), 65536))
            # gio launch interprets the desktop entry and starts a SECOND process;
            # GApplication forwards its two files to the unique primary over D-Bus.
            subprocess.run(["gio", "launch", str(entry), str(one), str(two)],
                           env=environment, check=True, timeout=10,
                           stdout=subprocess.PIPE, stderr=None)
            tail, _ = process.communicate(timeout=25)
            output.extend(tail)
            if process.returncode or b"association-smoke: pass" not in output:
                raise RuntimeError("association smoke did not complete")
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                tail, _ = process.communicate()
                output.extend(tail)
            print(output.decode(errors="replace"), end="", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--session", action="store_true", help=argparse.SUPPRESS)
    arguments = parser.parse_args()
    binary = arguments.binary.resolve(strict=True)
    if arguments.session:
        session(binary)
    else:
        subprocess.run(["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()),
                        str(binary), "--session"], check=True, timeout=60)


if __name__ == "__main__":
    main()
