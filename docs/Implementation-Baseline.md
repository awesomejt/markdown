# GATE-001 Implementation Baseline

## Approval status

Prepared 2026-10-26. Individual choices D-008–D-017 are accepted by the owner;
**final baseline approved by the owner (D-018)**. GATE-001 may close and
portable-core implementation may start at CORE-001A. This is architecture and
sequencing approval, not a statement that product acceptance already passes.

## Selected baseline

| Area | Decision |
| --- | --- |
| Core | Rust, pulldown-cmark, bounded shared extension passes; Comrak fallback/oracle, cmark-gfm GFM reference. YAML metadata option disabled. |
| Boundary | Hand-written versioned C ABI, immutable bulk values, explicit ownership/errors/cancellation. No UI types or ambient filesystem authority. |
| macOS | AppKit/TextKit 2 documents; SwiftUI for suitable shell/settings surfaces. |
| Windows | WinUI 3, C# Native AOT, native virtualized blocks. C++/WinRT fallback only for a concrete blocker. |
| Linux | GTK 4/gtk-rs, optional libadwaita, conditional native virtualized blocks; early LNX-001D1 accessibility proof required. |
| MVP targets | macOS 27 Apple Silicon; Windows 11 x64; Linux x86-64 KDE Plasma Wayland. Other platforms/architectures remain later scope. |
| Distribution targets | Developer ID signed/notarized Mac app; Windows MSIX; Linux Flatpak plus Arch package. Signing/publishing require separate owner authorization. |
| MVP scope | Local read-only Markdown reader, files/folders, native navigation/search/access/settings. No `*.mdv.yaml` project-file support or math/diagram engine requirement. |
| Delivery order | Thin walking skeletons on all three desktops first; then MVP deepening in macOS, Linux, Windows order. iOS/iPadOS follows desktop contract maturity. |
| Initial targets | D-012 launch to presented content: ≤400 ms warm, ≤700 ms cold; off-screen jumps ≤150 ms. Validate with the MVP on named hosts, ≥5 runs with spread and explicit cache treatment. |
| Advanced engines | Deferred by D-014 to ADV-005 after DESK-002, before ADV-002. No unrun engine check is passed and no engine is adopted. |

The targets cover 1 MiB/5 MiB prose and 1 MiB long-line profiles. macOS still
needs equivalent D-012 evidence. These targets are not universal hardware or
release-readiness guarantees. Numeric memory/search budgets remain task-level
measurement work; this gate does not waive bounded-resource requirements.

## Conditions carried into implementation

- **Linux early stop point:** after LNX-001D, LNX-001D1 proves native AT-SPI
  document/heading semantics, off-screen heading navigation and continuous
  reading beyond visible rows with owner-confirmed Orca on KDE Wayland.
  Preserve focus, reading order and basic selection. LNX-001E and LNX-001F
  stay blocked until it passes. Failure returns to the owner for a revised
  accessibility design or renderer decision; shortcuts alone do not prove it.
- **Windows:** fix duplicate Narrator reading and whole-tree UIA timeouts.
  Validate packaged/MSIX launch, mixed DPI and the remaining native interaction
  cases. C++ fallback does not imply maintaining a second app host now.
- **macOS:** investigate repeated-session RSS retention and main-thread layout
  cost; preserve cancellation/close safety and verify full semantic/native
  accessibility. Earlier spike timings do not establish the D-012 target.
- **All hosts:** product model/IDs, resource and HTML policy, images, source
  mappings, global selection, restoration, memory and performance need native
  evidence. Spikes do not become production code/contracts by default.

See [Decisions.md](../agents/Decisions.md),
[Windows FEAS-008](Virtualized-Renderer-Windows-Evaluation.md),
[Linux FEAS-008](Virtualized-Renderer-Linux-Evaluation.md),
[core extensions](Core-Extension-Evaluation.md),
[Mac integration](Shared-Core-macOS-Evaluation.md) and
[engine deferral](Diagram-Math-Engine-Evaluation.md).

## Delivery and state transition

Use the [phase catalog](Implementation-Plan.md) and
[delivery workflow](Delivery-Workflow.md): one bounded phase per feature branch
and PR, a Grok review through the `grok` CLI when ready, disposition of feedback,
and merge approval: a Grok review with no serious finding (D-029), or the
owner's. Batch dependency-ready work on the current
host. Preserve the one-pass limit and all-desktop skeleton/MVP gates.

The supported state helper has materialized 36 core/early-platform delivery
tasks and 92 dependency edges in `portable-core`, including LNX-001D1. Aggregate CORE/WIN/MAC/LNX
milestones retain their acceptance criteria and depend on their children.
Children depend on specific child prerequisites rather than their own aggregate.
Do not require another platform's counterpart merely for symmetry.

Validation passed on disposable state before live materialization: the graph
is acyclic, platform child chains are independent, LNX-001D1 blocks both
LNX-001E/001F, and a simulated gate/phase closeout selects CORE-001A first.
Live graph checks also passed, with GATE-001 still open and no implementation
pass started. Fixture/capability validation, 70 local document links, Markdown
lint and whitespace checks passed. The first implementation
task is CORE-001A after GATE-001 closeout and the feasibility phase transition.
The owner has explicitly approved this transition in D-018.
