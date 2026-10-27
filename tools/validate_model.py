"""Validate original v1 wire goldens independently of the Rust type decoder."""
from pathlib import Path
import hashlib
import json

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/semantic-model/v1"


def main() -> None:
    schema = json.loads((SPEC / "document.schema.json").read_text())
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)
    count = 0
    kinds = set()
    for path in sorted((SPEC / "fixtures").glob("*.json")):
        data = json.loads(path.read_text())
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
    print(f"Validated {count} source-bound v1 goldens, {len(kinds)} node kinds and version/field rejection")


if __name__ == "__main__":
    main()
