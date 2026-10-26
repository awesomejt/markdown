# FEAS-009 — bounded diagram and math engine screen

Disposable feasibility code, not an application renderer, accepted engine,
production sanitizer or hardened FFI. The owner requested best-effort work on
2026-10-26 so post-MVP features do not consume implementation time prematurely.

## Scope and results

The [evaluation](../../docs/Diagram-Math-Engine-Evaluation.md) distinguishes
measured Mac compiler behavior, source-reviewed alternatives and deferred work.
[Mac measurements](results-macos-2026-10-26.json) contain every retained sample.
All probe inputs are small original synthetic cases. They exercise the same
basic language constructs as the frozen corpus without copying source from
another repository. This is not the complete product corpus or a fidelity test.

- `EngineProbe.swift`: KaTeX in JavaScriptCore without a view; Mermaid in a
  nonpersistent offscreen WKWebView. Trusted bundles are injected locally; CSP
  denies network resources. No content is presented on screen or snapshotted.
- `d2probe.go`: direct in-process Go library with dagre, an empty capability
  filesystem for imports and a three-second compile context. The SVG renderer
  itself has no demonstrated hard cancellation. Raw remote-icon/link output
  deliberately remains visible to the checker.
- `check.py`: checks expected valid/invalid outcomes and expected raw-output
  hazards; passing it does not establish that generated output is safe.
- `IOSFrameworkCheck.swift`: framework compile/link only. No simulator app,
  engine archive, device behavior or distribution is established.

## Reproduce on macOS

Existing Xcode, Node/npm, Go and Python suffice. No Homebrew package or global
installation was required. `npm ci` disables lifecycle scripts and writes only
ignored `node_modules`; Go uses its normal module/build cache. The lock files
pin dependencies. Rust candidates are research only and are not built here.

From the repository root:

```sh
npm ci --prefix spikes/feas009 --ignore-scripts --no-audit --no-fund
mkdir -p build/feas009
xcrun swiftc -O -swift-version 5 -warnings-as-errors spikes/feas009/EngineProbe.swift -o build/feas009/EngineProbe
go build -C spikes/feas009 -mod=readonly -trimpath -ldflags='-s -w' -o ../../build/feas009/D2Probe .
python3 spikes/feas009/measure_macos.py
python3 spikes/feas009/check.py
xcrun --sdk iphonesimulator swiftc -warnings-as-errors -target arm64-apple-ios27.0-simulator -emit-library spikes/feas009/IOSFrameworkCheck.swift -o build/feas009/libIOSFrameworkCheck.dylib
```

Keep generated binaries, npm modules and raw diagnostic stderr ignored. The
committed result includes environment/limitations added at checkpoint closeout;
the runner regenerates raw measurements under `build/feas009` for comparison.
Use `check.py spikes/feas009/results-macos-2026-10-26.json` to check the retained
checkpoint without rebuilding.

## Measurement boundaries

Five fresh processes per engine; six operations per case, with the first
discarded. Report median/min/max of the 25 retained operation samples. These
processes use unpurged caches, not a cold-cache protocol. Process-to-result
includes all cases and iterations, so it is not a per-diagram latency.

`time -l` peak RSS covers only the host. In particular it excludes WebKit child
processes; it cannot rank total engine memory. Bundle/executable file sizes are
component observations, not installed application or iOS package deltas. No
D-012 launch-to-presented-content claim is made.

## Short counterpart checks

Windows and Linux should reuse these synthetic sources/options and the pinned
engine versions, not Mac build products. In a bounded native harness, test
WebView2/WebKitGTK compilation, invalid-input handling, remote resource denial,
five compile samples, total process-tree memory and package dependencies. Test
the same Go library/capability filesystem on each host. Use a process timeout;
record unavailable engines rather than spending a session installing a new
stack. Do not build full document surfaces or repeat FEAS-008.

Full fidelity, sanitizer, inline typography, Rust comparison and iOS engine
runtime work belong after the MVP unless the owner explicitly needs a result
to resolve GATE-001. Counterpart checks themselves may be deferred by an
explicit owner gate decision. The owner has now accepted that deferral (D-014): FEAS-009 closes on its
bounded evidence and ADV-005 tracks the unrun work after DESK-002, before
ADV-002. No further checks are needed for FEAS-009 before the core gate.
