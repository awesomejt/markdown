#!/usr/bin/env python3
"""Verify the Windows development file-association template and multi-process activation."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
REG_TEMPLATE = ROOT / "apps/windows/resources/io.github.awesomejt.markdown.reg.in"
EXPECTED_EXTENSIONS = (".md", ".markdown", ".mdown", ".mkd", ".txt")


def log_cmd(cmd: list[str]) -> None:
    line = "+ " + " ".join(cmd)
    try:
        print(line, flush=True)
    except UnicodeEncodeError:
        print(line.encode("ascii", "backslashreplace").decode("ascii"), flush=True)


def check_association(exe_path: Path) -> None:
    if not exe_path.is_file():
        raise SystemExit(f"Missing Windows executable: {exe_path}")
    if not REG_TEMPLATE.is_file():
        raise SystemExit(f"Missing association template: {REG_TEMPLATE}")

    template_text = REG_TEMPLATE.read_text(encoding="utf-8")
    if "[HKEY_CURRENT_USER\\Software\\Classes\\Io.Github.Awesomejt.Markdown\\shell\\open\\command]" not in template_text:
        raise SystemExit("Association template is missing shell\\open\\command section")
    if '@="\\"@EXE@\\" \\"%1\\""' not in template_text:
        raise SystemExit("Association template is missing '\"@EXE@\" \"%1\"' command specification")

    for ext in EXPECTED_EXTENSIONS:
        section = f"[HKEY_CURRENT_USER\\Software\\Classes\\{ext}\\OpenWithProgids]"
        if section not in template_text:
            raise SystemExit(f"Association template is missing {section}")

    with tempfile.TemporaryDirectory(prefix="markdown-win-assoc-") as tmp:
        tmp_dir = Path(tmp)
        escaped_exe = str(exe_path).replace("\\", "\\\\")
        rendered_reg = tmp_dir / "io.github.awesomejt.markdown.reg"
        rendered_reg.write_text(template_text.replace("@EXE@", escaped_exe), encoding="utf-8")

        alpha = tmp_dir / "alpha.md"
        beta = tmp_dir / "beta notes.markdown"
        gamma = tmp_dir / "γ-דוק-日本語.txt"

        alpha.write_text("# Alpha Startup\n\nFirst window.\n", encoding="utf-8")
        beta.write_text("# Beta Second Process\n\nSpaces in path.\n", encoding="utf-8")
        gamma.write_text("# Gamma Unicode Process\n\nUnicode filename.\n", encoding="utf-8")

        pipe_name = f"markdown-assoc-{uuid.uuid4().hex}"
        report_path = tmp_dir / "report.json"

        # Launch primary process with startup file argument (alpha.md).
        primary_cmd = [
            str(exe_path),
            "--association-smoke",
            pipe_name,
            str(report_path),
            "4",
            str(alpha),
        ]
        log_cmd(primary_cmd)
        primary = subprocess.Popen(
            primary_cmd,
            cwd=exe_path.parent,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )

        try:
            # Give the primary process a brief moment to acquire the primary mutex and start
            # its named-pipe listener, then invoke secondary processes for beta, gamma, and
            # a duplicate alpha (verifying existing-window reuse).
            time.sleep(0.25)
            for secondary_file in (beta, gamma, alpha):
                secondary_cmd = [
                    str(exe_path),
                    "--association-smoke",
                    pipe_name,
                    str(report_path),
                    "4",
                    str(secondary_file),
                ]
                log_cmd(secondary_cmd)
                res = subprocess.run(
                    secondary_cmd,
                    cwd=exe_path.parent,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    encoding="utf-8",
                    errors="replace",
                    timeout=15,
                )
                if res.stdout:
                    print(res.stdout, end="", flush=True)
                res.check_returncode()

            stdout, _ = primary.communicate(timeout=20)
            if stdout:
                print(stdout, end="", flush=True)
            if primary.returncode != 0:
                raise SystemExit(f"Primary association smoke process exited with {primary.returncode}")
        finally:
            if primary.poll() is None:
                primary.kill()

        if not report_path.is_file():
            raise SystemExit("Association smoke did not produce report.json")

        report = json.loads(report_path.read_text(encoding="utf-8"))
        if report.get("deliveries") != 4:
            raise SystemExit(f"Expected 4 deliveries, got {report}")
        if report.get("window_count") != 3:
            raise SystemExit(f"Expected 3 windows (duplicate alpha reused), got {report}")

        windows = report.get("windows", [])
        expected = [
            ("alpha.md — Markdown", "alpha.md", 1, "Alpha Startup", ""),
            ("beta notes.markdown — Markdown", "beta notes.markdown", 1, "Beta Second Process", ""),
            ("γ-דוק-日本語.txt — Markdown", "γ-דוק-日本語.txt", 1, "Gamma Unicode Process", ""),
        ]
        actual = [
            (
                w.get("title"),
                w.get("display_name"),
                w.get("generation"),
                w.get("heading"),
                w.get("error"),
            )
            for w in windows
        ]
        if actual != expected:
            raise SystemExit(f"Association window state mismatch:\n  expected: {expected}\n  actual:   {actual}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("Usage: check_association.py <path-to-Markdown.Windows.exe>")
    check_association(Path(sys.argv[1]).resolve())


if __name__ == "__main__":
    main()
