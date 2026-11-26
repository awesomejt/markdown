# Agent Session and Platform Handoff

## CORE-007 shared-core 5 MiB parse/encode

Updated: 2026-11-26 (America/Denver), active pass on `feat/core-007-parse-encode-opt`.

- Dense parser IDs skip SipHash maps in validation and flat-view encode;
  parser trusts builder `SourceInfo`; `sha2` `asm` enabled (sha2-asm 0.6.4).
- Evidence: Release `measure.py launch --profile prose-5mib --runs 5` —
  `core_parse_ms` 103.63, `core_encode_ms` 11.28, `core_total_ms` 117.44
  (MAC-003B baseline 186.61 / 46.53 / 235.57). See
  [macOS-Launch-Investigation.md](docs/macOS-Launch-Investigation.md).
- `tools/check.py core` and release core/ffi tests passed locally. Push PR
  into `develop`, monitor Grok + CI, then D-029 closeout.
- Residual: D-016 unwaived (`presented_ms` still > 400 ms on 5 MiB LS).
  `LNX-003A` / `WIN-003A` stay host-blocked until Linux / Windows.

## Previous handoff (MAC-003D)

See [memory archive](agents/Memory-Archive.md) / git history. Superseded for
next-step routing by CORE-007 above.

## Resume here — desktop foundations

Inspect state before any pass:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

On the Mac, Homebrew `python3` lacks PyYAML, so use
`/Users/jason/projects/dev/markdown/.build/feas004-venv/bin/python`.
Always pass `task_ref` (not `task_key`) to `start_pass`.
