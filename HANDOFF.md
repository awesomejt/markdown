# Agent Session and Platform Handoff

## MAC-003 split closeout (PR #103)

Updated: 2026-11-26 (America/Denver). PR #103 merged under D-029; MAC-003
pass finished **blocked** pending MAC-003E/F/G.

- Evidence: [docs/macOS-Release-Evidence.md](docs/macOS-Release-Evidence.md)
- Join: MAC-003E ∥ MAC-003F → MAC-003G → MAC-003
- Eligible now: **MAC-003E** (cold launch; owner `sudo purge`) and
  **MAC-003F** (owner manual checklist)
- Warm baseline at `5823b1d` still misses D-016 400 ms; cold unmeasured
- `LNX-003A` needs Linux; `WIN-003A` host-blocked until Windows

## Owner actions

1. **MAC-003E:** after release rebuild,

   ```sh
   .build/feas004-venv/bin/python apps/macos/tools/measure.py launch \
     --configuration release --runs 5 --cold
   ```

   Paste the three JSON lines into the evidence doc (admin password for
   `sudo purge`).

2. **MAC-003F:** run the manual checklist in the evidence doc (About,
   VoiceOver properties / 5 MiB end line, real `/Applications` install,
   quarantine/second Mac when available).

## Resume here

```sh
/Users/jason/projects/dev/markdown/.build/feas004-venv/bin/python \
  tools/project_state.py --file project.yaml inspect
```

Start `MAC-003E` or `MAC-003F` with `task_ref` set to that task key.
