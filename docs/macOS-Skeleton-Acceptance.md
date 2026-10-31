# macOS skeleton acceptance

MAC-001I completes the macOS walking skeleton: MAC-001A to MAC-001I, merged
into `develop` through PRs #39 to #46 and this task's PR. This page checks
the [Phase 3 slice](Implementation-Plan.md#phase-3--desktop-walking-skeletons)
and the [MAC-001 milestone](../TODO.md) on this host, and lists what stays
open. The MAC-001 aggregate closes on it. Per-task details, commands and
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
| Local development artifact, license inventory, relocated launch | MAC-001I | Unsigned arm64 zip with license inventory and notices for the 43 linked packages, no build-machine paths; extracted outside the checkout, launched directly and through Launch Services ([details](../apps/macos/README.md#development-artifact-and-licenses-mac-001i)) |

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

## Validation

```sh
.build/feas004-venv/bin/python apps/macos/tools/check.py
.build/feas004-venv/bin/python tools/check.py common
```

`check.py` ran on this host in debug and release at this task's head; the
results and the hosted CI run are in the MAC-001I pass record and PR.
The milestone review of the macOS skeleton (D-030 rule 3) follows the
MAC-001 aggregate.
