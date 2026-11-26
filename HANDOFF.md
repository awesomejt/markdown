# Agent Session and Platform Handoff

## MAC-003D window construction closeout

Updated: 2026-11-25 (America/Denver), MAC-003D approved closeout on PR #101.

- [PR #101](https://github.com/awesomejt/markdown/pull/101) defers Files/Search/Find
  until a folder is granted or Find is shown; `openAtLaunch` builds hidden then
  shows once; evidence in
  [macOS-Launch-Investigation.md](docs/macOS-Launch-Investigation.md).
- [Grok review on `bd46a08`](https://github.com/awesomejt/markdown/pull/101#pullrequestreview-6069659101)
  (follow-up): 0 Blockers, 0 Concerns, 0 Nits after fixing the
  `revealInFiles` eager-build concern from the first review of `52e6552`.
  Approved under D-029. Hosted CI run
  [45769107588](https://github.com/awesomejt/markdown/actions/runs/45769107588)
  green (including macos-native).
- Residual: warm Launch Services 1 MiB still misses 400 ms (D-016 unwaived);
  LS prose 1 MiB presented median ~539 ms after this work (was ~618). Shared
  core 5 MiB parse/encode remains CORE-007; cold launch and manual checks
  remain for MAC-003 / DESK-002.
- Next on this Mac: `CORE-007` (shared core 5 MiB) — **clear context** before
  starting it; this pass was AppKit launch chrome. `LNX-003A` needs Linux;
  `WIN-003A` stays host-blocked until Windows.

## Previous handoff (MAC-003C)

See git history / [memory archive](agents/Memory-Archive.md) for MAC-003C and
earlier closeouts. Superseded for next-step routing by the MAC-003D closeout
above.

## Resume here — desktop foundations

Inspect state before any pass:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

On the Mac, Homebrew `python3` lacks PyYAML, so use
`/Users/jason/projects/dev/markdown/.build/feas004-venv/bin/python`.
