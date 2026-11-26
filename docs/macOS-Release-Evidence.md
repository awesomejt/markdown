# macOS release evidence (MAC-003)

Tracks unwaived macOS performance, cold launch, accessibility, display and
installation gaps carried from [MAC-002](macOS-MVP-Acceptance.md) under D-037
into DESK-002 via MAC-003. No numeric target is waived here. Investigation and
host optimizations through MAC-003A–D and CORE-007 are summarized; remaining
work is split into focused delivery phases.

## Delivery split

| Phase | Scope | Status |
| --- | --- | --- |
| MAC-003A | Warm launch milestone investigation | Done (PR #98) |
| MAC-003B | Load-stage attribution; CORE-007 materialization | Done (PR #99) |
| MAC-003C | Startup and presentation attribution | Done (PR #100) |
| MAC-003D | ReaderWindow construction reduction | Done (PR #101) |
| CORE-007 | Shared core 5 MiB parse/encode | Done (PR #102) |
| **MAC-003E** | **Cold Launch Services measurement (D-016 700 ms)** | **Next (parallel with F)** |
| **MAC-003F** | **Owner manual release checklist** | **Next (parallel with E)** |
| **MAC-003G** | **Residual warm/D-016 and display disposition** | Pending |
| MAC-003 | Aggregate closeout when E–G clear or owner-decided | Waiting on E–G |

Join: `MAC-003E → MAC-003G`, `MAC-003F → MAC-003G → MAC-003`. Cold measure and
manual checklist do not depend on each other (both are owner-gated). Existing
edges `CORE-007 → MAC-003` and `MAC-003D → MAC-003` remain. Detail for A–D and
CORE-007 lives in [macOS-Launch-Investigation.md](macOS-Launch-Investigation.md).

## Gap register (from MAC-002 / D-037 / open budgets)

| Gap | Target / expectation | Current evidence | Owner |
| --- | --- | --- | --- |
| Warm LS launch to first text | D-016 ≤ 400 ms, ≥5 runs | Misses; see warm baseline below | MAC-003G |
| Cold LS launch to first text | D-016 ≤ 700 ms, ≥5 runs | Unmeasured (`measure.py launch --cold`) | MAC-003E |
| VoiceOver Document properties | Readable group + key/value labels | Unverified manually | MAC-003F |
| VoiceOver first line near end of 5 MiB | Note pause; `accessibilityLine(for:)` layout cost | Automated note 3.6 s once; VoiceOver unconfirmed | MAC-003F |
| About Markdown appearance / credits | Contract + commit lines, no paths | Automated smoke only | MAC-003F |
| Real Finder install into `/Applications` | Drag from development DMG | Stand-in Applications folder only | MAC-003F |
| Quarantined download / second Mac | Gatekeeper path for unsigned artifact | Untested | MAC-003F |
| Mixed-scale display / pacing (CAP-14) | Named-host evidence | Unmeasured | MAC-003G |
| Footprint after external refreshes | Tracked open budget | Measured growth; disposition open | MAC-003G |

## Post-CORE-007 investigation summary

On Apple M5 Max after CORE-007 (PR #102): 5 MiB release
`core_parse_ms` median 103.63 (was 186.61) and `core_encode_ms` 11.28
(was 46.53). Load work is no longer the dominant warm Launch Services wall
clock; process-to-main (~240–275 ms) plus AppKit bring-up still dominate
`presented_ms`. MAC-003D deferred Files/Search/Find construction and cut
document-only window setup; D-016 remains unwaived. See
[macOS-Launch-Investigation.md](macOS-Launch-Investigation.md).

## Warm Launch Services baseline (this pass)

Host: Apple M5 Max, macOS 27.2.1 (26B131), Xcode 27.2 (27B52), Rust 1.99.1.
Source: `develop` at `5823b1d` (post CORE-007 merge). Fresh
`apps/macos/tools/build.py --configuration release`, then:

```sh
export RUSTUP_HOME=…/build/feas002-macos/toolchains/rustup
export CARGO_HOME=…/build/feas002-macos/toolchains/cargo
export PATH="$CARGO_HOME/bin:$PATH"
export TMPDIR=/private/tmp
.build/feas004-venv/bin/python apps/macos/tools/measure.py launch \
  --configuration release --runs 5
```

Five warm runs per profile (one unrecorded warmup each). Tool reports
`meets_target` only when every run is ≤ 400 ms.

| Profile | `presented_ms` median (min–max) | `meets_target` (400 ms) |
| --- | ---: | --- |
| prose-1mib | 582.53 (553.71–608.18) | no |
| prose-5mib | 646.97 (620.53–679.89) | no |
| long-lines-1mib | 550.94 (523.41–593.18) | no |

Selected stage medians (prose-5mib): `process_to_main_ms` 251.78,
`process_to_did_finish_ms` 489.71, `core_parse_ms` 105.70,
`core_encode_ms` 12.08, load `total_ms` 122.95, presentation `total_ms` 6.44.
Cold launch was not run: `sudo purge` requires an interactive administrator
password in this environment.

### Reconciliation with prior warm medians

Prior prose-1mib Launch Services `presented_ms` medians on this Mac class are
not a single series: MAC-003B post-fix reported ~373 ms under macOS 27.0.1 /
Xcode 27.1; MAC-003D after window-construction work reported 538.76 ms; this
pass reports 582.53 ms under **macOS 27.2.1 / Xcode 27.2**. CORE-007 and
MAC-003D reduced attributed load and window-construction cost; they did not
remove dyld/Launch Services or AppKit bring-up, which still dominate wall
time. Session-to-session LS variance and the OS/Xcode bump can move medians
by tens of milliseconds without a product regression. **MAC-003G should
rebaseline with several warm sessions on the same OS/Xcode before an owner
D-016 decision**, and prefer stage timers (`process_to_main_ms`, load
`total_ms`, presentation `total_ms`) over single-session medians alone.

## MAC-003E — Owner cold-launch procedure

Cold measurement needs cache purge between runs. The harness already calls
`sudo purge` when `--cold` is set.

1. Rebuild release if the binary is not the commit under test:

   ```sh
   export RUSTUP_HOME=/Users/jason/projects/dev/markdown/build/feas002-macos/toolchains/rustup
   export CARGO_HOME=/Users/jason/projects/dev/markdown/build/feas002-macos/toolchains/cargo
   export PATH="$CARGO_HOME/bin:$PATH"
   export TMPDIR=/private/tmp
   .build/feas004-venv/bin/python apps/macos/tools/build.py --configuration release
   ```

2. From the repo root, in a terminal where you can enter an admin password
   (fifteen `purge` prompts across three profiles × five runs, unless
   credential caching covers the session):

   ```sh
   .build/feas004-venv/bin/python apps/macos/tools/measure.py launch \
     --configuration release --runs 5 --cold
   ```

3. Paste the three JSON summary lines into this document under
   **Cold Launch Services results**, with host facts, source commit, and
   whether each profile’s max `presented_ms` is ≤ 700 ms.

4. Do not claim a cold pass from warm numbers. Do not waive a miss without an
   owner decision (MAC-003G / Decisions.md).

### Cold Launch Services results

_Pending MAC-003E. Not measured on 2026-11-26 (sudo password required)._

## MAC-003F — Owner manual release checklist

Run against the same release binary (or the relocated DMG/zip from
`apps/macos/tools/artifact.py` / `check.py`) on the named Mac. Do not toggle
VoiceOver as part of automated smokes; start with VoiceOver already on for
items that need it. Record dated notes in the table.

| # | Check | How | Result |
| --- | --- | --- | --- |
| 1 | About Markdown | Markdown → About Markdown: version, `model-v2 (ABI v1, anchor v1)`, core package id, source commit; no filesystem paths | _pending_ |
| 2 | Document properties + VoiceOver | Open a file with front matter; expand Document properties; with VoiceOver, confirm the group and key/value labels | _pending_ |
| 3 | VoiceOver 5 MiB end line | Open `build/fixtures/performance/prose-5mib.md`; move VoiceOver near the end; note first-line pause and whether scrolling stays usable | _pending_ |
| 4 | Real `/Applications` install | Drag `Markdown.app` from the development DMG into `/Applications` (not a stand-in folder); launch from there; remove when done | _pending_ |
| 5 | Quarantine / Gatekeeper | On a second Mac if available, download the unsigned zip/DMG so quarantine attaches; record Gatekeeper refusal and Privacy & Security allow path. If no second Mac, mark blocked with reason | _pending_ |

Optional cross-checks from [apps/macos/README.md](../apps/macos/README.md)
manual items 1–11 remain useful but are not re-listed here unless a regression
appears.

## MAC-003G — Residual disposition inputs

After E and F land, MAC-003G must address each still-open row in the gap
register: further host work, a focused successor task, or an explicit owner
decision recorded in `agents/Decisions.md`. Warm Launch Services remaining
above 400 ms after CORE-007/MAC-003D is primarily dyld/Launch Services and
AppKit bring-up plus TextKit first layout, not core parse/encode.

## Relationship to DESK-002

DESK-002 stays blocked on MAC-003, WIN-003 and LNX-003. Closing MAC-003
requires E–G evidence (or owner decisions) plus aggregate recording here; it
does not waive Windows or Linux release evidence.
