#!/usr/bin/env python3
"""Validate the independent native Markdown fixture corpus."""

from __future__ import annotations

import hashlib
import tempfile
from pathlib import Path
from typing import Any

import yaml
from jsonschema import Draft202012Validator, FormatChecker

from generate_performance_fixtures import generate_profile
from generate_path_fixtures import materialize_paths


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "fixtures"
KNOWN_REQUIREMENTS = {
    "R1",
    "R1.1",
    "R1.2",
    "R1.3",
    "R1.4",
    "R1.5",
    "R2",
    "R3",
    "R4",
    "R5",
}


def load_yaml(path: Path) -> Any:
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def inside_fixture(path_text: str) -> Path:
    path = (FIXTURES / path_text).resolve()
    if not path.is_relative_to(FIXTURES.resolve()):
        raise ValueError(f"path escapes fixtures: {path_text}")
    if not path.is_file():
        raise ValueError(f"missing fixture file: {path_text}")
    return path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validator(schema_name: str) -> Draft202012Validator:
    schema = load_yaml(FIXTURES / "schema" / schema_name)
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema, format_checker=FormatChecker())


def main() -> int:
    manifest = load_yaml(FIXTURES / "manifest.yaml")
    validator("manifest.schema.yaml").validate(manifest)
    semantic_validator = validator("semantic-case.schema.yaml")
    performance_validator = validator("performance-profiles.schema.yaml")

    source = manifest["source_snapshot"]
    license_path = inside_fixture(source["license_file"])
    if sha256(license_path) != source["license_sha256"]:
        raise ValueError("copied source license hash does not match the manifest")

    case_ids: set[str] = set()
    listed_inputs: set[Path] = set()
    listed_expected: set[Path] = set()
    for case in manifest["cases"]:
        case_id = case["id"]
        if case_id in case_ids:
            raise ValueError(f"duplicate case id: {case_id}")
        case_ids.add(case_id)
        if set(case["requirements"]) - KNOWN_REQUIREMENTS:
            raise ValueError(f"{case_id}: unknown requirement reference")
        if case["origin"]["commit"] != source["commit"]:
            raise ValueError(f"{case_id}: origin commit differs from source snapshot")
        origin = Path(case["origin"]["path"])
        if origin.is_absolute() or ".." in origin.parts:
            raise ValueError(f"{case_id}: origin path must be repository-relative")

        input_path = inside_fixture(case["input"])
        expected_path = inside_fixture(case["expected"])
        listed_inputs.add(input_path)
        listed_expected.add(expected_path)
        if sha256(input_path) != case["sha256"]:
            raise ValueError(f"{case_id}: copied input hash mismatch")

        expected = load_yaml(expected_path)
        semantic_validator.validate(expected)
        if expected["case_id"] != case_id:
            raise ValueError(f"{case_id}: expected file names a different case")

    actual_inputs = {path.resolve() for path in (FIXTURES / "markdown").rglob("*.md")}
    actual_expected = {path.resolve() for path in (FIXTURES / "expected").glob("*.yaml")}
    if listed_inputs != actual_inputs:
        raise ValueError("manifest/input set differs from committed Markdown corpus")
    if listed_expected != actual_expected:
        raise ValueError("manifest/expected set differs from semantic expectation corpus")

    if any(not path.relative_to(FIXTURES).as_posix().isascii() for path in actual_inputs):
        raise ValueError("committed Markdown fixture paths must be ASCII; generate Unicode paths")
    output = ROOT / "build/fixtures/paths"
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="validate-", dir=output) as temporary:
        path_records = materialize_paths(FIXTURES, Path(temporary))

    profiles_path = inside_fixture(manifest["performance_profiles"])
    profiles = load_yaml(profiles_path)
    performance_validator.validate(profiles)
    profile_ids: set[str] = set()
    for profile in profiles["profiles"]:
        if profile["id"] in profile_ids:
            raise ValueError(f"duplicate performance profile id: {profile['id']}")
        profile_ids.add(profile["id"])
        content = generate_profile(profile)
        digest = hashlib.sha256(content).hexdigest()
        if len(content) != profile["target_bytes"] or digest != profile["sha256"]:
            raise ValueError(f"{profile['id']}: generated content is not deterministic")

    print(
        f"validated {len(case_ids)} semantic cases and "
        f"{len(profile_ids)} performance profiles and {len(path_records)} generated paths"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
