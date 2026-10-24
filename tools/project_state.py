#!/usr/bin/env python3
"""Run the project-state/v1 helper while preserving block-style YAML state."""

from __future__ import annotations

import importlib.util
import os
import sys
import tempfile
from pathlib import Path
from types import ModuleType
from typing import Any

try:
    import yaml
except ImportError as exc:  # pragma: no cover - environment failure
    raise SystemExit(
        "PyYAML is required for project state; install it with `python3 -m pip "
        "install PyYAML`."
    ) from exc


CONTRACT = "project-state/v1"


class BlockDumper(yaml.SafeDumper):
    """Indent block sequences under their mapping key."""

    def increase_indent(self, flow: bool = False, indentless: bool = False) -> None:
        return super().increase_indent(flow, False)


def backend_path() -> Path:
    override = os.environ.get("MARKDOWN_STATE_BACKEND")
    candidates = [
        Path(override).expanduser() if override else None,
        Path.home()
        / ".codex/skills/project-yaml-state-management/scripts/project_state.py",
    ]
    for candidate in candidates:
        if candidate is not None and candidate.is_file():
            return candidate
    raise SystemExit(
        "Cannot find the project-state/v1 backend. Set MARKDOWN_STATE_BACKEND "
        "to project_state.py from the project-yaml-state-management skill."
    )


def load_backend() -> ModuleType:
    path = backend_path()
    spec = importlib.util.spec_from_file_location("project_state_backend", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"Cannot load project-state backend: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def install_yaml_io(backend: ModuleType) -> None:
    def load_state(path: Path) -> dict[str, Any]:
        if not path.is_file():
            raise backend.StateError("not_found", f"State file does not exist: {path}")
        try:
            state = yaml.safe_load(path.read_text(encoding="utf-8"))
        except (OSError, yaml.YAMLError) as exc:
            raise backend.StateError(
                "validation_error", f"Cannot read valid YAML state: {exc}"
            ) from exc
        if not isinstance(state, dict) or state.get("contract_version") != CONTRACT:
            raise backend.StateError("validation_error", f"State must use {CONTRACT}")
        if not isinstance(state.get("revision"), int):
            raise backend.StateError(
                "validation_error", "State revision must be an integer"
            )
        return state

    def write_state(path: Path, state: dict[str, Any]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        payload = yaml.dump(
            state,
            Dumper=BlockDumper,
            allow_unicode=True,
            default_flow_style=False,
            sort_keys=False,
            width=100,
        )
        mode = path.stat().st_mode & 0o777 if path.exists() else 0o644
        descriptor, temporary_name = tempfile.mkstemp(
            dir=path.parent, prefix=f".{path.name}.", suffix=".tmp"
        )
        temporary = Path(temporary_name)
        try:
            os.fchmod(descriptor, mode)
            with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
                handle.write(payload)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, path)
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        finally:
            if temporary.exists():
                temporary.unlink()

    def read_request(path: str) -> dict[str, Any]:
        try:
            payload = sys.stdin.read() if path == "-" else Path(path).read_text("utf-8")
            request = yaml.safe_load(payload)
        except (OSError, yaml.YAMLError) as exc:
            raise backend.StateError(
                "validation_error", f"Cannot read valid YAML request: {exc}"
            ) from exc
        if not isinstance(request, dict):
            raise backend.StateError("validation_error", "Request must be an object")
        return request

    backend.load_state = load_state
    backend.write_state = write_state
    backend.read_request = read_request


def main() -> int:
    backend = load_backend()
    install_yaml_io(backend)
    return int(backend.main())


if __name__ == "__main__":
    raise SystemExit(main())
