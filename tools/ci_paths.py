"""Select affected host checks; shared contracts/tools fan out to every host."""
import json
import os
import subprocess


def classify(paths: list[str], full: bool = False) -> dict[str, bool]:
    shared = full or any(
        p in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "LICENSE", "NOTICE"}
        or p.startswith(("core/", "specs/", "fixtures/", "tools/", ".github/"))
        for p in paths)
    return {host: shared or any(p.startswith(prefix) for p in paths)
            for host, prefix in (("linux", "apps/linux/"), ("windows", "apps/windows/"),
                                 ("macos", ("apps/macos/", "apps/ios/")))}


if __name__ == "__main__":
    base, head = os.environ.get("BASE", ""), os.environ["HEAD"]
    full = not base or set(base) == {"0"} or os.environ.get("EVENT") == "workflow_dispatch"
    paths = [] if full else subprocess.check_output(
        ["git", "diff", "--name-only", "-z", base, head]).decode().rstrip("\0").split("\0")
    selected = classify(paths, full)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        for host, value in selected.items():
            stream.write(f"{host}={str(value).lower()}\n")
    print(json.dumps(selected, sort_keys=True))
