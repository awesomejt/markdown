# Repository Strategy

## Recommendation

Use **one monorepo for the native application family**, while keeping the
existing `markdown-viewer` web repository separate.

**Decision status:** accepted by the owner on 2026-10-24 (D-004).

The monorepo shares requirements, semantic contracts, fixture corpora,
benchmarks, version policy, release metadata, and any proven portable core. It
does **not** imply a shared UI toolkit. Windows, macOS, Linux, and iOS/iPadOS
applications remain separately buildable native products.

## Why one repository fits now

- A Markdown semantic change can update the core/model, fixtures, bindings,
  platform adapters, documentation, and capability matrix atomically.
- Early development has one product owner and one requirements source; separate
  repositories would add version coordination without creating autonomy.
- Cross-platform performance regressions can be compared against one corpus and
  benchmark manifest.
- Platform limitations remain visible rather than disappearing into separate
  issue trackers and divergent roadmaps.
- Shared code can be added conservatively and removed without publishing and
  coordinating multiple internal package repositories.
- CI can remain platform-isolated through path filters and a build matrix; a
  monorepo does not require every developer host to install every SDK.

## Proposed layout

```text
/
  AGENTS.md
  README.md
  TODO.md
  MEMORY.md
  project.yaml

  docs/
    Requirements.md
    Architecture.md
    Platform-Strategy.md
    Repository-Strategy.md
    Implementation-Plan.md
    Validation.md

  specs/
    semantic-model/
    platform-capabilities/
    settings/

  fixtures/
    manifest.yaml
    markdown/
    expected/
    performance/

  core/                    # only after the implementation gate
    crates/                # candidate Rust workspace, if selected
    include/               # stable C ABI, if selected

  apps/
    windows/               # WinUI/Windows-native project and adapter
    macos/                 # AppKit/SwiftUI project and adapter
    linux/                 # GTK/native Linux project and adapter
    ios/                   # UIKit/SwiftUI project and adapter, later

  tools/
    fixtures/
    benchmarks/
    packaging/
```

Each application directory owns its package manifests, lock files where the
toolchain expects them, tests, resources, native dependencies, and build
instructions. Root commands orchestrate only common validation; they do not
hide platform-native tools behind an opaque custom build system.

## Dependency direction

```text
requirements/specs/fixtures
            |
       portable core
      /      |      \
 Windows   macOS   Linux     (desktop first)
                       \
                       iOS   (later)
```

Platform applications may depend on the stable core facade and semantic
contract. The core never depends on an application. Platform code is not
imported across sibling applications. Shared UI abstractions are prohibited
unless a later decision demonstrates a measurable benefit without flattening
native behavior.

## CI and release isolation

- Always run documentation, schema, fixture, and core checks where applicable.
- Run Windows jobs for changes under `apps/windows`, core, specs, or fixtures.
- Run macOS jobs for changes under `apps/macos`, `apps/ios`, core, specs, or
  fixtures.
- Run Linux jobs for changes under `apps/linux`, core, specs, or fixtures.
- Produce separate platform artifacts and release notes from one coordinated
  release manifest; versions may diverge later if platform delivery requires
  it.
- A failing unavailable platform runner is not silently converted to a pass;
  release readiness records the missing evidence.

## Implementation branches and platform scheduling

From core implementation onward, each small delivery phase has one feature
branch and PR. Batch ready phases for the current host; counterpart platforms
need not advance in lockstep. Shared contract changes still require affected
native CI, and all-platform gates remain intact. Follow
[Delivery-Workflow.md](Delivery-Workflow.md) for `/grok review`, feedback, owner
merge approval and cross-machine handoffs; use the concrete catalog in
[Implementation-Plan.md](Implementation-Plan.md).

## When to split repositories

Reconsider the monorepo only when at least one concrete condition exists:

- Independent teams need separate permissions, governance, or release control.
- A platform adopts licensing or source-visibility constraints incompatible
  with the rest of the project.
- Repository size or CI fan-out creates measured, persistent productivity
  problems that sparse checkout/path filtering cannot solve.
- A platform intentionally diverges into a different product and no longer
  consumes the shared contracts on a coordinated cadence.
- The portable core becomes a separately supported public product with its own
  compatibility and release lifecycle.

If a split becomes necessary, publish/version the semantic contract and core
facade first. Avoid Git submodules as the primary coordination mechanism; they
move version synchronization into every consumer without solving contract
governance.
