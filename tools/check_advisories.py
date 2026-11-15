#!/usr/bin/env python3
"""Scan Cargo.lock files against RustSec/OSV advisories."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
OSV_BATCH_URL = "https://api.osv.dev/v1/querybatch"
OSV_VULN_URL = "https://api.osv.dev/v1/vulns/{}"
LOCKFILES = (
    ROOT / "Cargo.lock",
    ROOT / "apps" / "linux" / "Cargo.lock",
    ROOT / "core" / "fuzz" / "Cargo.lock",
)

# Explicitly dispositioned advisories: (advisory_id, package, version) -> rationale.
# Dispositioned advisories remain scoped to the exact package and version.
DISPOSITIONED: dict[tuple[str, str, str], str] = {
    ("RUSTSEC-2026-0433", "pulldown-cmark", "0.13.5"): (
        "ENABLE_WIKILINKS is not enabled in PARSER_OPTIONS; upgrading pulldown-cmark "
        "is a separate gated task per TODO.md."
    ),
}


def crates_io_packages(lockfile: Path) -> list[tuple[str, str]]:
    data = tomllib.loads(lockfile.read_text(encoding="utf-8"))
    packages = []
    for pkg in data.get("package", []):
        source = pkg.get("source", "")
        if "crates.io" in source:
            packages.append((pkg["name"], pkg["version"]))
    return packages


def query_osv_batch(
    packages: list[tuple[str, str]], timeout: float = 15.0
) -> list[list[dict]]:
    results: list[list[dict]] = []
    batch_size = 500
    for start in range(0, len(packages), batch_size):
        batch = packages[start : start + batch_size]
        payload = {
            "queries": [
                {"package": {"name": name, "ecosystem": "crates.io"}, "version": version}
                for name, version in batch
            ]
        }
        req = urllib.request.Request(
            OSV_BATCH_URL,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = json.loads(resp.read().decode("utf-8"))
        batch_results = body.get("results", [])
        if len(batch_results) != len(batch):
            raise RuntimeError(
                f"OSV batch response length mismatch: expected {len(batch)}, got {len(batch_results)}"
            )
        results.extend(entry.get("vulns", []) for entry in batch_results)
    return results


def fetch_vuln_aliases(adv_id: str, timeout: float = 15.0) -> list[str]:
    req = urllib.request.Request(OSV_VULN_URL.format(adv_id))
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        body = json.loads(resp.read().decode("utf-8"))
    return list(body.get("aliases", []))


def evaluate_advisories(
    packages: list[tuple[str, str]],
    vulns_per_pkg: list[list[dict]],
    dispositioned: dict[tuple[str, str, str], str] = DISPOSITIONED,
    alias_lookup=fetch_vuln_aliases,
) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    notes: list[str] = []
    for (name, version), vulns in zip(packages, vulns_per_pkg):
        for vuln in vulns:
            adv_id = vuln.get("id", "UNKNOWN")
            key = (adv_id, name, version)
            if key in dispositioned:
                notes.append(f"DISPOSITIONED {adv_id} ({name} {version}): {dispositioned[key]}")
                continue
            aliases = vuln.get("aliases")
            if aliases is None and alias_lookup is not None:
                try:
                    aliases = alias_lookup(adv_id)
                except (urllib.error.URLError, TimeoutError, OSError):
                    aliases = []
            matched_alias = next(
                (alias for alias in (aliases or []) if (alias, name, version) in dispositioned),
                None,
            )
            if matched_alias is not None:
                notes.append(
                    f"DISPOSITIONED {adv_id} (alias of {matched_alias}, {name} {version}): "
                    f"{dispositioned[(matched_alias, name, version)]}"
                )
            else:
                errors.append(
                    f"UNRESOLVED ADVISORY {adv_id} on {name} {version} "
                    f"(https://rustsec.org/advisories/{adv_id}.html)"
                )
    return errors, notes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    in_ci = bool(os.environ.get("CI") or os.environ.get("GITHUB_ACTIONS"))
    parser.add_argument(
        "--allow-offline",
        action="store_true",
        default=os.environ.get("MD_ADVISORY_ALLOW_OFFLINE") == "1" or not in_ci,
        help="Warn instead of failing if api.osv.dev is unreachable (default outside CI).",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Fail if api.osv.dev is unreachable even outside CI.",
    )
    args = parser.parse_args()
    allow_offline = args.allow_offline and not args.strict

    all_packages: list[tuple[str, str]] = []
    seen: set[tuple[str, str]] = set()
    for lockfile in LOCKFILES:
        if not lockfile.exists():
            raise SystemExit(f"Missing lockfile: {lockfile}")
        pkgs = crates_io_packages(lockfile)
        print(f"{lockfile.relative_to(ROOT)}: {len(pkgs)} crates.io packages")
        for pkg in pkgs:
            if pkg not in seen:
                seen.add(pkg)
                all_packages.append(pkg)

    try:
        vulns = query_osv_batch(all_packages)
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        if allow_offline:
            print(f"WARNING: Advisory scan skipped (offline/network error: {exc})", file=sys.stderr)
            return 0
        print(f"ERROR: Advisory scan failed to reach OSV API: {exc}", file=sys.stderr)
        return 1

    errors, notes = evaluate_advisories(all_packages, vulns)
    for note in notes:
        print(note)
    if errors:
        for err in errors:
            print(err, file=sys.stderr)
        return 1
    print(f"Dependency advisory scan passed across {len(all_packages)} unique crates.io packages "
          f"({len(notes)} dispositioned).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
