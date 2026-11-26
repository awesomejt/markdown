# Agent Session and Platform Handoff

## MAC-003D window construction (in progress / ready for PR)

Updated: 2026-11-25 (America/Denver), MAC-003D implementation complete on
`feat/mac-003d-window-construction`; PR/review/closeout next.

- Defers Files/Search controllers and Find bar until a folder is granted or
  Find is shown; outline-only sidebar on document-only launch.
- `openAtLaunch` builds with `makeWindow(show: false)` then shows once after
  open. Opt-in `MARKDOWN_WINDOW_TIMING=1` stamps init stages.
- Evidence in [macOS-Launch-Investigation.md](docs/macOS-Launch-Investigation.md):
  init ~73–80 ms (was ~130 ms); LS prose 1 MiB presented median ~539 ms (was
  ~618); `menus_to_did_finish_ms` median ~126 ms (was ~144). D-016 unwaived.
- Native `apps/macos/tools/check.py` passed (debug+release smoke, relaunch,
  preferences). Local `tools/check.py` hits a pre-existing
  `/private/var` vs `/var` path assert in `test_ci_paths.py` on this Mac;
  unrelated to this change.
- Next on this Mac after MAC-003D closeout: `CORE-007` (shared core 5 MiB
  parse/encode) — clear context when switching to it. `MAC-003` aggregate and
  cold/manual evidence remain for DESK-002. `LNX-003A` needs Linux;
  `WIN-003A` is host-blocked until Windows.

## Previous handoff (MAC-003C)

See git history / [memory archive](agents/Memory-Archive.md) for MAC-003C and
earlier closeouts. Superseded for next-step routing by the MAC-003D note above.

## Resume here — desktop foundations

Inspect state before any pass:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

On the Mac, Homebrew `python3` lacks PyYAML, so use
`/Users/jason/projects/dev/markdown/.build/feas004-venv/bin/python`.
