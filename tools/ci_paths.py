"""Select host checks conservatively; unknown build paths fan out to all hosts."""
import json
import os
import subprocess
import urllib.request
from collections.abc import Callable, Mapping


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


def changed(base: str, head: str, cwd=None) -> list[str]:
    # Treat renames as a deletion plus addition, preserving source-host impact.
    out = subprocess.check_output(
        ["git", "diff", "--name-only", "--no-renames", "-z", base, head], cwd=cwd)
    return [p for p in out.decode().split("\0") if p]


def checks_green(sha: str, env: Mapping[str, str]) -> bool:
    """True only when GitHub Actions' latest `complete` check on sha succeeded."""
    repo, token = env.get("REPO", ""), env.get("GH_TOKEN", "")
    if not repo or not token:
        return False
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repo}/commits/{sha}/check-runs"
        "?check_name=complete&filter=latest",
        headers={"Authorization": f"Bearer {token}",
                 "Accept": "application/vnd.github+json"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            runs = json.load(response).get("check_runs", [])
    except (OSError, ValueError):
        return False
    runs = [r for r in runs if (r.get("app") or {}).get("slug") == "github-actions"]
    return bool(runs) and all(r.get("conclusion") == "success" for r in runs)


def unchanged_since_green(env: Mapping[str, str], green: Callable[[str], bool], cwd=None) -> bool:
    # A PR push that adds only state/docs to an already-green head needs no new host
    # run (TOOL-002). Force pushes, missing commits and red or cancelled previous
    # runs fall back to the full PR rule.
    prev, head = env.get("PREV", ""), env.get("PR_HEAD", "")
    if env.get("EVENT") != "pull_request" or not prev or set(prev) == {"0"} or not head:
        return False
    try:
        subprocess.run(["git", "merge-base", "--is-ancestor", prev, head],
                       cwd=cwd, check=True, capture_output=True)
        paths = changed(prev, head, cwd)
    except subprocess.CalledProcessError:
        return False
    return all(non_build(p) for p in paths) and green(prev)


def decide(env: Mapping[str, str], *, cwd=None,
           green: Callable[[str], bool] | None = None) -> dict[str, bool]:
    base = env.get("BASE", "")
    full = not base or set(base) == {"0"} or env.get("EVENT") == "workflow_dispatch"
    if full:
        return classify([], full=True)
    if unchanged_since_green(env, green or (lambda sha: checks_green(sha, env)), cwd):
        return classify([])
    return classify(changed(base, env["HEAD"], cwd))


if __name__ == "__main__":
    selected = decide(os.environ)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        for host, value in selected.items():
            stream.write(f"{host}={str(value).lower()}\n")
    print(json.dumps(selected, sort_keys=True))
