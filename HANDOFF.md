# Agent Session and Platform Handoff

## CORE-007 closeout

Updated: 2026-11-26 (America/Denver), CORE-007 approved closeout on PR #102.

- [PR #102](https://github.com/awesomejt/markdown/pull/102): dense parser IDs,
  trusted fingerprint skip, Apple-only sha2 asm. Evidence in
  [macOS-Launch-Investigation.md](docs/macOS-Launch-Investigation.md).
- Grok follow-up on `b8ecae2`: 0 Blockers, 0 Concerns, 1 Nit (doc wording
  fixed in `890083a`). Approved under D-029. Hosted CI run
  [45777967360](https://github.com/awesomejt/markdown/actions/runs/45777967360)
  green (including windows-native / macos-native after relaunch flake cleared).
- Residual: D-016 unwaived; 5 MiB LS presented still > 400 ms. Next on Mac:
  `MAC-003` aggregate / cold-manual evidence. `LNX-003A` needs Linux;
  `WIN-003A` stays host-blocked until Windows.

## Previous handoff (MAC-003D)

See [memory archive](agents/Memory-Archive.md) / git history.

## Resume here — desktop foundations

Inspect state before any pass:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

On the Mac, Homebrew `python3` lacks PyYAML, so use
`/Users/jason/projects/dev/markdown/.build/feas004-venv/bin/python`.
Always pass `task_ref` (not `task_key`) to `start_pass`.
