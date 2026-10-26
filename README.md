# Markdown

`markdown` is the planning and future implementation repository for
performance-oriented native Markdown applications. Desktop applications for
Windows, macOS, and Linux come first; iOS and iPadOS follow after the desktop
core and contracts are proven.

This project is related to, but independent from, the existing
[`markdown-viewer`](../markdown-viewer/README.md) LAN web application. The web
application remains the reference for behavior and fixtures; it is not being
replaced or moved into this repository.

## Current state

The repository is in **feasibility**. No production stack has been scaffolded.
The architecture in [docs/Architecture.md](docs/Architecture.md) is a proposed
baseline with explicit decision gates, not an irreversible choice.

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
