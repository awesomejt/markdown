"""Check common inline Markdown file links, excluding code and URI targets.

This is a bounded tooling checker, not a full CommonMark parser: reference-style
links, nested labels and heading-anchor validation remain outside its scope.
"""
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


def prose(text: str) -> str:
    lines = []
    fence = None
    for line in text.splitlines(keepends=True):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if fence:
            if marker and marker[1][0] == fence[0] and len(marker[1]) >= len(fence) and not marker[2].strip():
                fence = None
            lines.append("\n")
        elif marker:
            fence = marker[1]
            lines.append("\n")
        else:
            lines.append(line)
    text = "".join(lines)
    # Equal-length backtick runs delimit inline code. Longer runs are content.
    runs = list(re.finditer(r"`+", text))
    chars = list(text)
    next_same = {}
    following = {}
    for index in range(len(runs) - 1, -1, -1):
        following[index] = next_same.get(runs[index][0])
        next_same[runs[index][0]] = index
    i = 0
    while i < len(runs):
        end = following[i]
        if end is None:
            i += 1
        else:
            chars[runs[i].start():runs[end].end()] = " " * (runs[end].end() - runs[i].start())
            i = end + 1
    return "".join(chars)


def local_targets(text: str):
    text = prose(text)
    for match in re.finditer(r"!?\[[^\]\n]*\]\(\s*", text):
        i = match.end()
        target = []
        angle = i < len(text) and text[i] == "<"
        i += int(angle)
        depth = 0
        while i < len(text):
            char = text[i]
            if char == "\\" and i + 1 < len(text):
                target.append(text[i + 1]); i += 2; continue
            if angle:
                if char == ">":
                    i += 1
                    break
            elif char.isspace() or (char == ")" and depth == 0):
                break
            elif char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
            target.append(char)
            i += 1
        tail = text[i:]
        # Closing ')' or a conventional quoted/parenthesized optional title.
        if not re.match(r'''^\s*(?:(?:"[^"\n]*"|'[^'\n]*'|\([^\n)]*\))\s*)?\)''', tail):
            continue
        value = "".join(target)
        if not value or re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", value):
            continue
        parsed = urlsplit(value)
        if parsed.netloc or not parsed.path:
            continue
        yield unquote(parsed.path)


def check_paths(root: Path, names: list[str]) -> int:
    errors = []
    count = 0
    for name in sorted(set(names)):
        path = root / name
        if name.startswith("fixtures/markdown/") or not path.is_file():
            continue
        for target in local_targets(path.read_text(encoding="utf-8")):
            if not (path.parent / target).exists():
                errors.append(f"{name}: missing local target {target}")
            count += 1
    if errors:
        raise ValueError("\n".join(errors))
    return count
