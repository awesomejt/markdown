"""Select host checks conservatively; unknown build paths fan out to all hosts."""
import json
import os
import subprocess
from collections.abc import Mapping


HOST_PATHS = {"linux": ("apps/linux/",), "windows": ("apps/windows/",),
              "macos": ("apps/macos/", "apps/ios/")}


def non_build(path: str) -> bool:
    # Only explicitly known prose/operational state can bypass host checks.
    # core/ (even its docs) and unknown app/platform roots remain conservative.
    return (path.startswith(("docs/", "agents/", ".agents/"))
            or path == "project.yaml"
            or (path.endswith(".md") and "/" not in path))


def classify(paths: list[str], full: bool = False) -> dict[str, bool]:
    native_roots = tuple(prefix for roots in HOST_PATHS.values() for prefix in roots)
    shared = full or any(not non_build(p) and not p.startswith(native_roots) for p in paths)
    return {host: shared or any(p.startswith(roots) for p in paths)
            for host, roots in HOST_PATHS.items()}


def decide(env: Mapping[str, str], *, cwd=None) -> dict[str, bool]:
    base = env.get("BASE", "")
    full = not base or set(base) == {"0"} or env.get("EVENT") == "workflow_dispatch"
    if full:
        return classify([], full=True)
    # Treat renames as a deletion plus addition, preserving source-host impact.
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", "--no-renames", "-z", base, env["HEAD"]], cwd=cwd)
    return classify([p for p in changed.decode().split("\0") if p])


if __name__ == "__main__":
    selected = decide(os.environ)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        for host, value in selected.items():
            stream.write(f"{host}={str(value).lower()}\n")
    print(json.dumps(selected, sort_keys=True))
