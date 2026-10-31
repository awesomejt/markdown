# macOS skeleton acceptance

MAC-001I completed the macOS walking skeleton: MAC-001A to MAC-001I, merged
into `develop` through PRs #39 to #47. This page checks
the [Phase 3 slice](Implementation-Plan.md#phase-3--desktop-walking-skeletons)
and the [MAC-001 milestone](../TODO.md) on this host, and lists what stays
open. The MAC-001 aggregate closes on it, with the
[aggregate evidence](#mac-001-aggregate) below. Per-task details, commands and
measurements are in the [macOS app README](../apps/macos/README.md); review
and completion evidence is in each task's PR and `project.yaml` pass.

Host: Apple M5 Max (Mac17,6), macOS 27.0.1 (26A434), Xcode 27.1, Swift 6.4,
Rust 1.99.1. Product target: macOS 27 on Apple Silicon (D-018); other OS
versions and architectures are not claimed.

## Slice criteria

| Criterion | Delivered by | Evidence on this host |
| --- | --- | --- |
| Native launch, windows and commands | MAC-001A (PR #39) | AppKit shell; bundle smoke (windows, menus, shortcuts, close/reopen, minimize, Quit) in debug and release, locally and in the required `macos-native` CI job |
| Core package and adapter | MAC-001B (PR #40) | Static model-v2 `markdown-ffi` archive, ABI/model negotiation, owned snapshot copies, bounded worker and admission, cancellation; release consumer relocated without the repository ([integration report](macOS-Core-Integration.md)) |
| File open, association and errors | MAC-001C (PR #41) | Open panel, Viewer/Alternate document type recorded by Launch Services, `open -a` delivery, typed path-free errors (missing, unavailable disk, permission, revoked, folder, type, size, UTF-8), generation-safe off-main loading |
| Headings, prose, lists, links, code; outline; selection | MAC-001D (PR #42) | TextKit 2 renderer off the main thread; all 8 corpus outlines match; fragment and external link policy; one continuous selectable view |
| Tables, task lists, images and resource policy | MAC-001E (PR #43) | Aligned table rows with grid fragments and TSV copy; task markers; contained, size- and pixel-bounded image reads; remote and unsafe targets never read |
| Find, theme and text controls | MAC-001F (PR #44) | Core find over the shown snapshot with exact highlights and Unicode rules; nine text sizes; System/Light/Dark; Settings |
| Large documents, global selection, accessibility | MAC-001G (PR #45) | Bounded repeated-load memory (allocator fix), lead-first display, fast replacement, lazy RTF copy, heading/level/table/image rotors; measured launch and interaction budgets |
| External refresh and reading restoration | MAC-001H (PR #46) | kqueue watcher and activation check through core change dispositions; anchored top line and selection across revisions; stale notice; AppKit restoration with a real two-launch check |
| Local development artifact, license inventory, relocated launch | MAC-001I (PR #47) | Unsigned arm64 zip with license inventory and notices for the 43 linked packages, no build-machine paths; extracted outside the checkout, launched directly and through Launch Services ([details](../apps/macos/README.md#development-artifact-and-licenses-mac-001i)) |

Boundaries held throughout: the core receives bytes and opaque revision
tokens, never paths (file access stays in `MarkdownDocument`); the UI uses
owned model values, never core pointers or lifetimes; raw HTML stays inert
text; no WebView renders documents; no third-party Swift package is used.

## What remains open

None of these blocks the skeleton; each is recorded where it will be
resolved, and none is waived.

- **Budgets** ([TODO.md](../TODO.md#open-native-budgets)): prose 5 MiB warm
  launch (median 547 ms against 400 ms), unmeasured cold launches, the first
  VoiceOver line query on 5 MiB (3.6 s, then about 1 GiB), mixed-scale
  displays and display pacing, and the footprint after external refreshes
  of a 5 MiB document (887–1,043 MiB).
- **Manual checks not performed by the agent**
  ([README](../apps/macos/README.md#manual-shell-check)): 8 (find and
  settings with the physical keyboard and VoiceOver announcements), 9
  (VoiceOver reading, rotors and the 5 MiB pause, styled copy after quit)
  and 10 (real editors, Finder deletion, the system Close-windows setting).
  The interactive open panel and Finder Open With (check 6) are also the
  owner's. The smoke covers the same code paths with synthetic input.
- **Capability deviations** ([matrix](Platform-Capability-Evaluation.md)):
  sibling images need a folder grant once sandboxed (CAP-04, MAC-002A);
  network-volume change detection (CAP-06), background work when not
  frontmost (CAP-08) and mixed scaling (CAP-14) are deferred. The artifact is verified only on the Mac that built it (CAP-28).
- **Deferred review findings and CI** ([TODO.md](../TODO.md#deferred-review-findings)):
  duplicate windows for a case-variant path or hard link (MAC-002A), and
  the hosted `macos-native` runner flakiness.
- **Not in the skeleton:** sandboxing, folder grants and recents
  (MAC-002A), signing and notarization (owner approval), heading roles for
  continuous VoiceOver reading, and diagrams and math (ADV-005).

## MAC-001 aggregate

The aggregate adds no implementation; it records already-merged evidence
against the MAC-001 validation expectations (basic corpus, native
interaction, accessibility, refresh, restoration and package, plus the
native CI job from 001A and the release-mode core consumer from 001B that
CORE-001 and CORE-006 no longer carry, D-027).

**AppKit and SwiftUI.** The milestone asked for AppKit/SwiftUI
responsibilities to be decided from measurements. The
[rendering evaluation](macOS-Rendering-Evaluation.md) measured the TextKit 2
surface directly and hosted in SwiftUI with similar timing, so hosting gave
no advantage, and D-010 limits SwiftUI to shell surfaces. The skeleton uses
AppKit throughout: windows, menus, the outline sidebar, the reading surface,
find and the Settings window (`SettingsWindowController`, one small
`NSWindowController`). No target imports SwiftUI. SwiftUI stays allowed for
later shell surfaces, such as MAC-002 preferences, when a surface gains from
it; that choice needs no new decision.

**Fresh native evidence.** At the `develop` head after PR #47
(`96d8aea`, source unchanged; only docs (this page, `TODO.md`) and task state
were edited),
`apps/macos/tools/check.py` passed on the host above, about 3 minutes:

- `swift format` lint (strict) and generated performance fixtures;
- in debug and in release: 30 render, 11 document and 13 core tests, no
  build-tree or dynamic Rust dependency (`otool -L`), the full bundle smoke
  (windows, commands, close/reopen, minimize, documents, errors, reading,
  outline, tables, images, find, text size, appearance, Settings, external
  refresh, anchors and restoration) and the real two-launch relaunch check;
- in debug, the smoke from a bundle copied outside the checkout;
- in release, the development artifact extracted outside the checkout,
  verified (43 linked packages inventoried) and launched directly and
  through Launch Services, and the relocated `CoreProbe` release consumer
  on a 5 MiB synthetic input (177,726 nodes, parse 81 ms, zero native
  handles, retained footprint 31.6 MiB).

**Hosted CI.** The `develop` push run for PR #47's merge,
[41215648620](https://github.com/awesomejt/markdown/actions/runs/41215648620)
at `96d8aea`, passed every job, including the required `macos-native`
debug/release job and `core` on all three OSes. The previous `develop` push
run, [41204938464](https://github.com/awesomejt/markdown/actions/runs/41204938464)
at `941a571`, failed `macos-native` before any project code ran, with the
known rustup `detected conflict: 'bin/cargo-fmt'`
([hosted flakiness entry](../TODO.md#deferred-review-findings)).

**Milestone.** The skeleton is the D-030 rule 3 boundary for macOS (and the
8-PR cap has passed since `m-core-fix`). The milestone review follows this
aggregate: freeze green `develop` with no task PR waiting as
`review/<milestone>`, then Claude prepares the packet and only the owner
merges into `main`. The open items above go into that packet; the hosted
`macos-native` fix is recommended before it.

## Validation

```sh
.build/feas004-venv/bin/python apps/macos/tools/check.py
.build/feas004-venv/bin/python tools/check.py common
```

`check.py` ran on this host in debug and release at MAC-001I's head and
again for the aggregate at `96d8aea`. Results and hosted runs are in the
MAC-001I and MAC-001 pass records and PRs.
