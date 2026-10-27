# Markdown

`markdown` is the implementation repository for
performance-oriented native Markdown applications. Desktop applications for
Windows, macOS, and Linux come first; iOS and iPadOS follow after the desktop
core and contracts are proven.

This project is related to, but independent from, the existing
`markdown-viewer` LAN web application (see the [source review](docs/Source-Review.md)). The web
application remains the reference for behavior and fixtures; it is not being
replaced or moved into this repository.

## Current state

GATE-001 is approved (D-018); **portable-core implementation** has begun.
CORE-001A's workspace/check/CI scaffold is merged. CORE-002A now defines the
[v1 semantic model](specs/semantic-model/v1/README.md) and schema/goldens.
CORE-003A adds [bounded basic parsing](docs/Parsing.md) and conservative safety
classification. CORE-003B1 adds [bounded typed front matter](docs/Front-Matter.md).
CORE-003B2 adds opt-in [v2 references and URI targets](specs/semantic-model/v2/README.md)
while preserving default v1. Remaining extension conformance and native application shells remain
separate phases; no working reader is delivered by the core alone. The accepted
[implementation baseline](docs/Implementation-Baseline.md) and
[architecture](docs/Architecture.md) preserve the required native evidence.

Run [development checks](docs/Development.md); see
[dependency/license policy](docs/Dependencies.md). New implementation source
uses Apache-2.0; imported fixture provenance and notices remain intact.

Start with:

- [Source review](docs/Source-Review.md) — what exists and what can be reused.
- [Requirements](docs/Requirements.md) — extracted behavior and delivery tiers.
- [Product definition](docs/Product.md) — target users, scope, and parity tiers.
- [Platform strategy](docs/Platform-Strategy.md) — Rust, Swift, and C++ options.
- [Repository strategy](docs/Repository-Strategy.md) — monorepo boundaries.
- [Architecture](docs/Architecture.md) — proposed components and boundaries.
- [Implementation plan](docs/Implementation-Plan.md) — phased work and gates.
- [Fixture corpus](fixtures/README.md) — frozen semantic and performance inputs.
- [Parser evaluation](docs/Parser-Evaluation.md) — FEAS-002 measurements and
  lead core/parser recommendation.
- [Linux rendering evaluation](docs/Linux-Rendering-Evaluation.md) — GTK 4
  feasibility, native behavior, and large-document limits.
- [Windows rendering evaluation](docs/Windows-Rendering-Evaluation.md) — WinUI
  text surfaces, virtualization, host checks and measured limits.
- [Windows core integration](docs/Shared-Core-Windows-Evaluation.md) — Rust C
  ABI, ownership, cancellation and WinUI timing.
- [macOS core integration](docs/Shared-Core-macOS-Evaluation.md) — Swift/AppKit
  ownership, cancellation, packaging and memory findings.
- [Linux core integration](docs/Shared-Core-Linux-Evaluation.md) — gtk-rs C-ABI
  ownership, cancellation, debugging, GTK timing and relocatable packaging.
- [macOS rendering evaluation](docs/macOS-Rendering-Evaluation.md) — AppKit
  TextKit comparisons, native interaction, memory, and development bundles.
- [Platform capability evaluation](docs/Platform-Capability-Evaluation.md) —
  FEAS-007 per-platform matrix for files, lifecycle, diagrams, output and
  packaging, with sources and next checks.
- [Core extension evaluation](docs/Core-Extension-Evaluation.md) — FEAS-010
  front matter, math, alerts and voice-over syntax on pulldown-cmark.
- [Linux virtualized renderer evaluation](docs/Virtualized-Renderer-Linux-Evaluation.md)
  — FEAS-008 Linux part: GTK `GtkListView` semantic blocks, selection, Orca
  and a WebKitGTK baseline.
- [Windows virtualized renderer evaluation](docs/Virtualized-Renderer-Windows-Evaluation.md)
  — FEAS-008 Windows part: WinUI `ListView` semantic blocks in C# JIT,
  Native AOT and C++/WinRT, selection, Narrator and UIA, and a WebView2
  baseline.
- [Diagram and math engine screen](docs/Diagram-Math-Engine-Evaluation.md) —
  FEAS-009 bounded Mac compiler evidence and proposed post-MVP deferrals.
- [Feasibility review](docs/Feasibility-Review.md) — REV-001 quality check
  of FEAS-001–007, reproduced evidence and recommendations for GATE-001.
- [TODO](TODO.md) — human-readable roadmap; `project.yaml` is task-state truth.

## Proposed direction

```text
Windows app       macOS app       Linux app        iOS/iPadOS app
 native UI         native UI       native UI          native UI
      \                |               |                 /
       platform adapters + versioned semantic contract
                            |
              candidate high-performance shared core
```

The first releases are local, read-only desktop Markdown readers. They
prioritize startup, parsing, long-document layout, scrolling, native file
access, OS integration, and offline operation. Each platform owns its UI and
may differ where the OS makes a feature impractical. Advanced features and
mobile support follow the core desktop experience.

## Repository workflow

Agents and contributors must read [AGENTS.md](AGENTS.md). Durable context lives
in [MEMORY.md](MEMORY.md), decisions in [agents/Decisions.md](agents/Decisions.md),
and operational task state in `project.yaml`. [AGENT_WORKFLOW.md](AGENT_WORKFLOW.md)
defines the recovery-safe task loop. From core implementation onward,
[Delivery workflow](docs/Delivery-Workflow.md) defines small platform-focused
phases, feature branches, PRs and the `/grok review` trigger.

## Relationship to `markdown-viewer`

The native project may copy versioned test fixtures and documented contracts
with provenance. It must not import source through sibling-directory paths or
silently change the web project. Cross-repository changes require an explicit
task and owner approval.
