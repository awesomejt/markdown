"""Validate versioned wire goldens independently of the Rust type decoder."""
from pathlib import Path
import hashlib
import json

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/semantic-model/v1"


def main() -> None:
    schema = json.loads((SPEC / "document.schema.json").read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)
    count = 0
    kinds = set()
    for path in sorted((SPEC / "fixtures").glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        source = path.with_suffix(".source.txt").read_bytes()
        source.decode("utf-8")
        validator.validate(data)
        if data["source"]["bytes"] != len(source):
            raise ValueError(f"{path.name}: source length mismatch")
        if data["source"]["sha256"] != hashlib.sha256(source).hexdigest():
            raise ValueError(f"{path.name}: source hash mismatch")
        kinds.update(node["content"]["kind"] for node in data["nodes"])
        for changed in (dict(data, model_version=2), dict(data, unknown=True)):
            if validator.is_valid(changed):
                raise ValueError(f"{path.name}: invalid version/unknown field accepted")
        count += 1
    if count != 5 or len(kinds) != 24:
        raise ValueError(f"unexpected coverage: {count} fixtures, {len(kinds)} kinds")
    spec2 = ROOT / "specs/semantic-model/v2"
    schema2 = json.loads((spec2 / "document.schema.json").read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema2)
    validator2 = Draft202012Validator(schema2)
    count2 = 0
    for path in sorted((spec2 / "fixtures").glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        source = path.with_suffix(".source.txt").read_bytes()
        source.decode("utf-8")
        validator2.validate(data)
        if data["source"] != {"bytes": len(source), "sha256": hashlib.sha256(source).hexdigest()}:
            raise ValueError(f"{path.name}: v2 source mismatch")
        for changed in (dict(data, model_version=1), dict(data, model_version=3), dict(data, unknown=True)):
            if validator2.is_valid(changed):
                raise ValueError(f"{path.name}: invalid v2 version/unknown field accepted")
        count2 += 1
    if count2 != 1:
        raise ValueError(f"unexpected v2 coverage: {count2}")
    print(f"Validated {count2} source-bound v2 golden and strict version/field rejection")
    print(f"Validated {count} source-bound v1 goldens, {len(kinds)} node kinds and version/field rejection")


if __name__ == "__main__":
    main()
