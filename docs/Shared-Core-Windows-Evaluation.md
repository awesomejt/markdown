# Shared-core Windows integration — FEAS-006 checkpoint

Date: 2026-10-25. **Windows portion evaluated; FEAS-006 remains partial.** Native
macOS/Swift and Linux/GTK bindings, debugging and packages are still required.
No production parser, semantic model, ABI or stack is accepted by this checkpoint.

Subsequent checkpoint: [macOS/Swift integration](Shared-Core-macOS-Evaluation.md)
is now evaluated as well; native Linux/GTK evidence remains outstanding. The
Windows measurements and limitations below retain their original scope.

## Outcome

A Rust/pulldown-cmark core can feed native Windows UI through a small C ABI.
The C# and C++ callers negotiate layouts, consume copied values and release
native allocations through the owning DLL. The WinUI variant runs parsing and
copy/decode on a worker and frees native snapshots before building its controls.

For 5 MiB prose, the virtualized WinUI probe reached a render callback at a
median **459 ms**, including approximately **42.6 ms** in Rust parse/model
construction and **19.9 ms** in managed copy/decode. The FEAS-003 Markdig spike
spent about 15.7 seconds in its parser on this input. The frontends' projections
differ and the scheduling changed, so this is not a controlled language or
equivalent-renderer race. It does isolate the earlier synchronous adapter as a
major cost and supports further shared-core integration.

The continuous controls still time out on large documents or repeated navigation.
Replacing the parser does not solve their native layout, memory or selection
strategy. Virtualized blocks still lack document-wide selection and a proven
complete accessibility reading order.

Source/reproduction: [spike README](../spikes/feas006/README.md).
Contract: [C header](../spikes/feas006/include/md_core.h).
Durable evidence: [Windows measurements](../spikes/feas006/results-windows-2026-10-25.yaml).

## Scope and environment

Windows 11 Pro 26H2 build 26300.9711, x64; Ryzen 9 5950X, 64 GiB, RTX 4080;
the same native machine and 150% display configuration as FEAS-003. Rust/Cargo
1.99.1, MSVC 14.51.36231, .NET SDK 10.0.402/runtime 10.0.13, Windows SDK 26100,
Windows App SDK 2.5.1 and pulldown-cmark 0.13.5. Cargo and NuGet dependencies are
locked. Source baseline: `2821ff532d6defebde69bcd204721d509a8ef2fa` plus this spike.

The Rust crate is OS-independent except its explicitly gated Windows breakpoint
test. That source shape is not native macOS/Linux validation. The Windows
native client owns file access and Windows memory/debug APIs; the core receives
only bytes, limits, values and cancellation capabilities.

## Contract and ownership evidence

ABI v1 returns one immutable snapshot: a UTF-8 text buffer plus fixed-width
block records. Windows x64 sizes negotiated successfully from both hosts:
40-byte options, 48-byte block and 56-byte snapshot. The table contains kind,
heading level, source byte range and text offsets. IDs are snapshot-local;
edit-stable IDs and the full production semantic model remain future work.

The C# caller pins input during the call, validates output shape/ranges, copies
both buffers and materializes independent strings. `SafeHandle` releases the
snapshot through Rust even if host conversion throws. C++ uses RAII for the
module and snapshot, copies its own values, and checks release/double-release.
No parser arenas, borrowed input references, UI objects or allocator ownership
cross the interface. The GUI checkpoints show zero live native snapshots and
tokens before WinUI surface setup, including the trials that later stall.

Input is capped at 16 MiB, logical table/text output at 64 MiB, with a caller
block cap. Null control arguments, invalid UTF-8, unsupported ABI/options sizes,
input/output limits, released handles and stale cancellation handles return
fixed status codes. Normal exports contain Rust unwinds; a fixed-message panic
returned `Panic`, after which subsequent calls succeeded. Valid pointer ranges
and non-overlapping buffers remain caller obligations; arbitrary invalid
pointers, allocation aborts and access violations are outside that guarantee.

The final lifecycle test passed 1,000 load/free iterations, including ten 5 MiB
documents, plus 400 concurrent small loads. It exercised host-exception cleanup
and retained managed values after native release. Native snapshot/token counters
returned to zero. Private-byte samples following forced managed GC ended around
42 MiB, with no monotonic growth across the sampled batches. This is bounded
evidence, not a universal leak proof or a total-memory quota.

Ten cancellation requests were issued after the diagnostic counter observed
native parse entry; all returned `Cancelled`. Request-to-managed-return latency
was approximately 20–28 ms in the final run. Polling is cooperative: the parser
cannot be interrupted while performing internal work before its next event.
The GUI pre-cancellation path also returned cleanly. This does not make WinUI
layout or the future multi-document generation/lifecycle controller cancellable.

## Call, copy and rendering costs

Console medians below use iterations 2–5 after first-use binding/JIT. Whole-call
time includes UTF-8 validation and publication as well as the parse/model timer;
their difference must not be described as pure P/Invoke overhead.

| Input | C# core parse/model ms | Whole native call ms | Managed copy/decode ms | C++ bulk copy ms |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB prose | 8.86 | 8.92 | 2.42 | 0.45 |
| 5 MiB prose | 42.30 | 42.64 | 20.65 | 2.09 |
| 1 MiB delimiters | 16.55 | 16.62 | 2.67 | 0.56 |
| 1 MiB long lines | 0.72 | 0.77 | 0.54 | 0.19 |

C++ copies UTF-8/table values without constructing C# strings/records, so its
copy work is different. There are two bulk buffer copies and no per-block FFI
calls. The 5 MiB input exports about 4.68 MB of text and a 5.83 MB block table;
the protocol is not zero-copy and the full model remains resident in the host.

Virtualized WinUI medians from three fresh-process trials per input:

| Input | Core parse/model ms | Managed copy/decode ms | Entry-to-render callback ms | Peak working set MiB |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB prose | 9.54 | 7.96 | 381 | 157 |
| 5 MiB prose | 42.57 | 19.91 | 459 | 217 |
| 1 MiB delimiters | 16.83 | 8.94 | 383 | 157 |
| 1 MiB long lines | 0.76 | 0.81 | 560 | 198 |

The GUI run records 41 trials: 24 semantic, 12 virtualized performance,
four continuous-control comparisons and one intentional cancellation. Thirty-seven
completed normally, one cancelled as requested and three timed out. The 1 MiB
RichTextBlock comparison reached content/selection but stalled in navigation;
both continuous 5 MiB trials stalled in layout after core parsing had completed.
RichEditBox completed 1 MiB prose at about 3.16 seconds entry-to-render in its
single comparison. These are retained failures, not successful parses relabeled
as successful document presentation.

Render timestamps have the FEAS-003 authored-entry/composition-callback scope:
they exclude earlier runtime bootstrap and are not cold-cache launch, first GPU
presentation or sustained input frame pacing. No new WPR attribution is claimed.
Virtualization and source/model projections remain provisional, and selected text
is compared to this adapter's output, not full semantic equivalence.

## Semantic checks, debugging and package

Initial outline checks caught lost dollar delimiters in an inline-math heading.
The projection was corrected to preserve readable math source, without modifying
fixture expectations. The corrected core matches level/text outlines for all
eight semantic fixtures, and the math case passes in all three WinUI surfaces.
The primary measurements retain their earlier DLL fingerprint and mismatch;
the correction and final DLL hash are separate evidence. Performance inputs have
no math events, so their projection algorithm did not change. Outline matching
is only a semantic subset: rich inline types, code language, metadata properties,
link/image policy, complete mappings and diagnostics remain unimplemented.

A native C++ caller reached the Rust breakpoint probe through the DLL. The
Win32/DbgHelp harness resolved it to `rust/src/lib.rs` using the release PDB.
This establishes native source information across the boundary, not mixed
managed/Rust source stepping or a Visual Studio IDE-debugging claim.

A fresh unsigned development publish was assembled with the **normal DLL**,
which reports probe hooks disabled, plus the C header, native client, fixtures,
Cargo/NuGet version/checksum metadata and license files. All three WinUI surfaces
and the C++ caller launched from that folder, and ZIP integrity passed. Markdig
is absent from the package. It remains framework-dependent on installed .NET,
Windows App SDK and MSVC runtimes; clean-machine deployment, signing, MSIX,
registration, stores and publishing were not exercised.

## Remaining acceptance and handoff

Keep FEAS-006 partial. Next prove the same values/ownership contract through a
native Swift/AppKit adapter and a native Linux/GTK host, including packages,
layout negotiation, errors, cancellation, repeated sessions and source debugging.
Do not substitute cross-builds or the portable source for named-host evidence.

Before GATE-001, reconcile the experimental model with the production semantic
contract and measure whether to keep this hand-written C ABI or generated
bindings. WIN-001 still needs incremental presentation, global selection/UIA
reading order, real input frame pacing and cancellation during layout. This
checkpoint supports continuing Rust integration; it does not waive those gaps.

Checks: Rust debug/release tests, formatting and warning-free Clippy; locked
native/managed builds; native/C# ABI clients; outline contracts and correction;
live ownership/concurrency/cancellation/panic tests; GUI timing/failures; native
Rust source/PDB resolution; normal-DLL package launches and license/ZIP checks;
fixture integrity, Python compilation, Markdown lint and whitespace checks.
