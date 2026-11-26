# Agent Session and Platform Handoff

## MAC-003 aggregate split / evidence package

Updated: 2026-11-26 (America/Denver), PR #103 review fixes on
`docs/mac-003-release-evidence-split`.

- Materialized **MAC-003E** and **MAC-003F** in parallel into **MAC-003G →
  MAC-003** (cold launch and owner manual checklist do not block each other).
- Evidence package: [docs/macOS-Release-Evidence.md](docs/macOS-Release-Evidence.md)
  (gap register, post-CORE-007 warm baseline at `5823b1d` with prior-median
  reconciliation, cold procedure, manual checklist).
- Warm LS medians after fresh release rebuild: prose-1mib 582.53 ms,
  prose-5mib 646.97 ms, long-lines-1mib 550.94 ms (all miss 400 ms; D-016
  unwaived). Cold not run: interactive `sudo purge` password required.
- **Owner action for MAC-003E:** from repo root, after release rebuild, run
  `.build/feas004-venv/bin/python apps/macos/tools/measure.py launch
  --configuration release --runs 5 --cold` in a terminal that can enter an
  admin password; paste JSON summaries into the evidence doc.
- **Owner action for MAC-003F:** run the manual checklist in the evidence doc
  (About, VoiceOver properties / 5 MiB end line, real `/Applications` install,
  quarantine/second Mac when available).
- Next eligible after this MAC-003 pass is blocked for the split: **MAC-003E**
  or **MAC-003F**. `LNX-003A` needs Linux; `WIN-003A` stays host-blocked until
  Windows.

## Previous handoff (CORE-007)

See [memory archive](agents/Memory-Archive.md) / git history. PR #102 merged.

## Resume here — desktop foundations

Inspect state before any pass:

```sh
/Users/jason/projects/dev/markdown/.build/feas004-venv/bin/python \
  tools/project_state.py --file project.yaml inspect
```

On the Mac, Homebrew `python3` lacks PyYAML, so use that venv. Pass
`task_ref` as the task key (for example `MAC-003E`); the wrapper resolves it.
