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
defines the recovery-safe task loop.

## Relationship to `markdown-viewer`

The native project may copy versioned test fixtures and documented contracts
with provenance. It must not import source through sibling-directory paths or
silently change the web project. Cross-repository changes require an explicit
task and owner approval.
