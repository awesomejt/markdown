#!/usr/bin/env python3
"""Apply the reviewed portability patch to the owner's project-state backend.

Origin: ~/.codex/skills/project-yaml-state-management/scripts/project_state.py,
copied from the owner's Mac on 2026-10-24. Only the known source or patched
hash is accepted; unrelated local edits are never overwritten.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import tempfile
from pathlib import Path


SOURCE_SHA256 = "ec6881a156f28825424a99f094db95e67944845aecd542a78e80bfa51497828e"
PATCHED_SHA256 = "1469ea816f1186a6715a26b4cacc2f07de01dc3bfa0c38018e9c13747ad6c087"

PLATFORM_IMPORTS = '''import errno
import time

if os.name == "nt":
    import msvcrt
elif os.name == "posix":
    import fcntl
else:
    raise RuntimeError(f"Unsupported project-state platform: {os.name}")

LOCK_TIMEOUT_SECONDS = 30.0
'''

LOCK_SOURCE = '''@contextmanager
def state_lock(path: Path, *, exclusive: bool) -> Iterator[None]:
    lock_path = path.with_name(f"{path.name}.lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a+b") as handle:
        if os.name == "nt":
            # Serialize readers too. Lock byte zero, including on empty files;
            # never truncate or unlink the stable sidecar while using it.
            deadline = time.monotonic() + LOCK_TIMEOUT_SECONDS
            while True:
                handle.seek(0)
                try:
                    msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                    break
                except OSError as exc:
                    if exc.errno not in {errno.EACCES, errno.EAGAIN, errno.EDEADLK}:
                        raise
                    if time.monotonic() >= deadline:
                        raise StateError(
                            "lease_conflict", "Timed out waiting for state file lock",
                            retryable=True,
                        ) from exc
                    time.sleep(0.05)
        else:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH)
        try:
            yield
        finally:
            if os.name == "nt":
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


'''

WRITE_SOURCE = '''def atomic_write_text(path: Path, payload: str) -> None:
    """Replace a closed, synced sibling file; POSIX also syncs its directory.

    Windows uses file fsync plus os.replace, without POSIX permission bits or
    directory fsync. This is not a guarantee against power loss on every FS.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    mode = path.stat().st_mode & 0o777 if path.exists() else 0o644
    descriptor, temporary_name = tempfile.mkstemp(
        dir=path.parent, prefix=f".{path.name}.", suffix=".tmp"
    )
    temporary = Path(temporary_name)
    try:
        # Own the descriptor before any operation that could fail.
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\\n") as handle:
            if os.name == "posix":
                os.fchmod(handle.fileno(), mode)
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        if os.name == "posix":
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    finally:
        if temporary.exists():
            temporary.unlink()


def write_state(path: Path, state: dict[str, Any]) -> None:
    atomic_write_text(path, json.dumps(state, indent=2, sort_keys=True) + "\\n")


'''


def patch_source(source: str) -> str:
    if hashlib.sha256(source.encode("utf-8")).hexdigest() != SOURCE_SHA256:
        raise ValueError("Unrecognized backend version; review local changes before patching")
    source = source.replace("import fcntl\n", "", 1)
    source = source.replace("CONTRACT = ", PLATFORM_IMPORTS + "\nCONTRACT = ", 1)
    start = source.index("@contextmanager\ndef state_lock(")
    end = source.index("def load_state(", start)
    source = source[:start] + LOCK_SOURCE + source[end:]
    start = source.index("def write_state(")
    end = source.index("def task_by_ref(", start)
    result = source[:start] + WRITE_SOURCE + source[end:]
    compile(result, "project_state_backend", "exec")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", type=Path, default=Path.home() /
                        ".codex/skills/project-yaml-state-management/scripts/project_state.py")
    parser.add_argument("--check", action="store_true", help="Check without applying")
    args = parser.parse_args()
    target = args.backend.expanduser().resolve()
    original = target.read_bytes()
    source = original.decode("utf-8").replace("\r\n", "\n")
    digest = hashlib.sha256(source.encode("utf-8")).hexdigest()
    if digest == PATCHED_SHA256:
        print("Project-state backend portability patch is already installed.")
        return 0
    updated = patch_source(source)
    if args.check:
        print("Known POSIX-only backend; run this command without --check to patch it.")
        return 1
    backup = target.with_name(target.name + ".pre-portability.bak")
    with backup.open("xb") as handle:
        handle.write(original)
    descriptor, temporary = tempfile.mkstemp(dir=target.parent, suffix=".tmp")
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            if os.name == "posix":
                os.fchmod(handle.fileno(), target.stat().st_mode & 0o777)
            handle.write(updated)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, target)
    finally:
        Path(temporary).unlink(missing_ok=True)
    print(f"Patched backend: {target}\nOriginal backup: {backup}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
