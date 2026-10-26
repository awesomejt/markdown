#!/usr/bin/env python3
"""Validate the platform capability matrix document mechanically.

Checks structure and evidence rules only; technical truth remains review work.
Standard library only, so it runs on every host without the fixture venv.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DOCUMENT = ROOT / "docs" / "Platform-Capability-Evaluation.md"
DEFAULT_PROJECT = ROOT / "project.yaml"

PLATFORMS = ("Windows", "macOS", "Linux", "iOS/iPadOS")
STATUSES = {"Supported", "Alternative", "Deferred", "Unavailable"}
EVIDENCE = {"Measured", "Authoritative", "Unverified"}
SOURCE_KINDS = {"M": "Measured", "A": "Authoritative", "U": "Unverified"}
REQUIRED_SECTIONS = (
    "Classification rules",
    "Overview",
    "iOS/iPadOS synthesis",
    "Retained measured failures and deviations",
    "Implications for GATE-001",
    "Sources",
)
OVERVIEW_HEADER = ["ID", "Capability", "Requirement", *PLATFORMS]
DETAIL_HEADER = [
    "Platform",
    "Status",
    "Evidence",
    "Basis and user impact",
    "Fallback",
    "Next check",
    "Sources",
]
SOURCE_HEADER = ["ID", "Kind", "Reference", "Scope"]
REQUIREMENTS = {"R1.1", "R1.2", "R1.3", "R1.4", "R1.5", "R2", "R3", "R4", "R5"}

CAP_ID = re.compile(r"^CAP-\d{2}$")
CAP_HEADING = re.compile(r"^### (CAP-\d{2}) — (.+)$")
SOURCE_ID = re.compile(r"^[MAU]\d{2}$")
LINK = re.compile(r"^\[[^\]]+\]\(([^)\s]+)\)$")
OWNER = re.compile(r"^([A-Z]+-\d{3}): \S")
PLACEHOLDER = re.compile(r"\b(TODO|TBD|FIXME|XXX)\b|\?\?\?|^\.\.\.$")


def split_row(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def read_table(lines: list[str], start: int) -> tuple[list[str], list[list[str]], int]:
    """Return the header, body rows and next index for a table at or after start."""
    index = start
    while index < len(lines) and not lines[index].startswith("|"):
        if lines[index].startswith("#"):
            raise ValueError(f"line {start + 1}: expected a table before the next heading")
        index += 1
    if index + 1 >= len(lines):
        raise ValueError(f"line {start + 1}: table not found")
    header = split_row(lines[index])
    if not re.fullmatch(r"\|( --- \|)+", lines[index + 1].strip()):
        raise ValueError(f"line {index + 2}: malformed table separator")
    rows = []
    index += 2
    while index < len(lines) and lines[index].startswith("|"):
        rows.append(split_row(lines[index]))
        index += 1
    return header, rows, index


def section_index(lines: list[str], title: str) -> int | None:
    for index, line in enumerate(lines):
        if line == f"## {title}":
            return index
    return None


def load_task_keys(project: Path) -> set[str]:
    text = project.read_text(encoding="utf-8")
    return set(re.findall(r"^\s*task_key:\s*(\S+)\s*$", text, re.MULTILINE))


def validate(text: str, document: Path, task_keys: set[str]) -> list[str]:
    errors: list[str] = []
    lines = text.splitlines()

    for title in REQUIRED_SECTIONS:
        if section_index(lines, title) is None:
            errors.append(f"missing section: ## {title}")
    if errors:
        return errors

    # Source catalog.
    sources: dict[str, str] = {}
    try:
        header, rows, catalog_end = read_table(lines, section_index(lines, "Sources") + 1)
    except ValueError as error:
        return [f"sources: {error}"]
    if header != SOURCE_HEADER:
        errors.append(f"sources: header must be {SOURCE_HEADER}")
    for row in rows:
        if len(row) != len(SOURCE_HEADER):
            errors.append(f"sources: wrong column count in {row[:1]}")
            continue
        source_id, kind, reference, scope = row
        if not SOURCE_ID.match(source_id):
            errors.append(f"sources: invalid ID {source_id!r}")
            continue
        if source_id in sources:
            errors.append(f"sources: duplicate ID {source_id}")
        sources[source_id] = kind
        if SOURCE_KINDS[source_id[0]] != kind:
            errors.append(f"sources: {source_id} kind must be {SOURCE_KINDS[source_id[0]]}")
        if not scope or PLACEHOLDER.search(scope):
            errors.append(f"sources: {source_id} needs a scope")
        link = LINK.match(reference)
        if source_id[0] == "M":
            if not link or "://" in link.group(1):
                errors.append(f"sources: {source_id} must link a local path")
            elif not (document.parent / link.group(1)).resolve().exists():
                errors.append(f"sources: {source_id} path does not exist: {link.group(1)}")
        elif source_id[0] == "A":
            if not link or not link.group(1).startswith("https://"):
                errors.append(f"sources: {source_id} must link an HTTPS reference")
        elif "](" in reference or "://" in reference or not reference:
            errors.append(f"sources: {source_id} must describe a gap without a link")

    # Overview.
    try:
        header, rows, _ = read_table(lines, section_index(lines, "Overview") + 1)
    except ValueError as error:
        return errors + [f"overview: {error}"]
    if header != OVERVIEW_HEADER:
        errors.append(f"overview: header must be {OVERVIEW_HEADER}")
    overview: dict[str, tuple[str, list[str]]] = {}
    for position, row in enumerate(rows, start=1):
        if len(row) != len(OVERVIEW_HEADER):
            errors.append(f"overview: wrong column count in {row[:1]}")
            continue
        cap_id, title, requirement, *statuses = row
        if cap_id != f"CAP-{position:02d}":
            errors.append(f"overview: expected CAP-{position:02d}, found {cap_id}")
        if cap_id in overview:
            errors.append(f"overview: duplicate {cap_id}")
        for item in requirement.split(", "):
            if item not in REQUIREMENTS:
                errors.append(f"overview: {cap_id} unknown requirement {item!r}")
        for platform, status in zip(PLATFORMS, statuses):
            if status not in STATUSES:
                errors.append(f"overview: {cap_id} {platform} invalid status {status!r}")
        overview[cap_id] = (title, statuses)
    if not overview:
        errors.append("overview: no capabilities")

    # Detail sections.
    details: dict[str, list[str]] = {}
    cited: set[str] = set()
    for index, line in enumerate(lines):
        match = CAP_HEADING.match(line)
        if not match:
            if line.startswith("### CAP"):
                errors.append(f"line {index + 1}: malformed capability heading")
            continue
        cap_id, title = match.groups()
        if cap_id in details:
            errors.append(f"{cap_id}: duplicate detail section")
            continue
        if cap_id not in overview:
            errors.append(f"{cap_id}: detail section missing from overview")
        elif overview[cap_id][0] != title:
            errors.append(f"{cap_id}: title differs from overview")
        try:
            header, rows, _ = read_table(lines, index + 1)
        except ValueError as error:
            errors.append(f"{cap_id}: {error}")
            continue
        if header != DETAIL_HEADER:
            errors.append(f"{cap_id}: header must be {DETAIL_HEADER}")
        if [row[0] for row in rows] != list(PLATFORMS):
            errors.append(f"{cap_id}: rows must be exactly {list(PLATFORMS)} in order")
        statuses = []
        for row in rows:
            if len(row) != len(DETAIL_HEADER):
                errors.append(f"{cap_id}: wrong column count for {row[0]}")
                continue
            platform, status, evidence, basis, fallback, next_check, cell = row
            where = f"{cap_id} {platform}"
            statuses.append(status)
            for name, value in zip(DETAIL_HEADER, row):
                if not value or PLACEHOLDER.search(value):
                    errors.append(f"{where}: {name} is empty or a placeholder")
            if status not in STATUSES:
                errors.append(f"{where}: invalid status {status!r}")
            if evidence not in EVIDENCE:
                errors.append(f"{where}: invalid evidence {evidence!r}")
            if evidence == "Unverified" and status != "Deferred":
                errors.append(f"{where}: only Deferred may rest on Unverified evidence")
            owner = OWNER.match(next_check)
            if not owner:
                errors.append(f"{where}: next check must start with an owner task key")
            elif owner.group(1) not in task_keys:
                errors.append(f"{where}: next-check owner {owner.group(1)} is not a project task")
            ids = [item.strip() for item in cell.split(",")]
            for source_id in ids:
                if source_id not in sources:
                    errors.append(f"{where}: unknown source {source_id!r}")
            cited.update(ids)
            prefixes = {source_id[:1] for source_id in ids if source_id in sources}
            required = {"Measured": "M", "Authoritative": "A", "Unverified": "U"}.get(evidence)
            if required and required not in prefixes:
                errors.append(f"{where}: {evidence} evidence needs a {required} source")
        details[cap_id] = statuses
        if cap_id in overview and statuses != overview[cap_id][1]:
            errors.append(f"{cap_id}: overview statuses differ from detail rows")
    for cap_id in overview:
        if cap_id not in details:
            errors.append(f"{cap_id}: missing detail section")

    # Every catalog entry is cited by a cell or by prose outside the catalog.
    prose = "\n".join(lines[: section_index(lines, "Sources")] + lines[catalog_end:])
    for source_id in sources:
        if source_id not in cited and not re.search(rf"\b{source_id}\b", prose):
            errors.append(f"sources: {source_id} is never cited")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("document", nargs="?", type=Path, default=DEFAULT_DOCUMENT)
    parser.add_argument("--project", type=Path, default=DEFAULT_PROJECT)
    args = parser.parse_args()
    document = args.document.resolve()
    errors = validate(
        document.read_text(encoding="utf-8"), document, load_task_keys(args.project)
    )
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        return 1
    print(f"capability matrix valid: {document.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
