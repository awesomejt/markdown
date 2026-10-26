# Shared-Core Linux Integration Evaluation

## Decision summary

FEAS-006 now has native integration evidence on Windows, macOS, and Linux. On
the measured Linux workstation, a gtk-rs host consumed the existing experimental
Rust core through its exported C ABI, copied and validated all values, released
native snapshots and cancellation tokens before GTK construction, and retained
zero native owners through repeated, concurrent, error, panic, and cancellation
paths. Runtime layout negotiation, Rust source debugging, and a relocated
normal-library development package also passed.

This completes the FEAS-006 evidence task. It does **not** accept the spike ABI,
flat semantic model, Rust/pulldown-cmark stack, GTK document strategy, performance
budgets, or any production package. FEAS-007 and GATE-001 still own capability
reconciliation and the owner's architecture decision.

Durable measurements are in
[the Linux result summary](../spikes/feas006/results-linux-2026-10-25.yaml).
Reproduction commands and the cross-host boundary are in the
[spike README](../spikes/feas006/README.md).

## Environment and method

The final corrected run used CachyOS Linux with kernel 7.3.0-4-cachyos and
glibc 2.44 on x86-64, an AMD Ryzen 9 5950X, 64 GiB RAM, an NVIDIA RTX 4080,
and a KDE Wayland session. Tool versions were Rust/Cargo 1.99.1, GCC 16.2.1,
GDB 18.1, GTK 4.24.1, and Pango 1.58.2.

The Linux host is a separate gtk-rs binary with no Rust dependency on the core
crate. It uses `libloading` to open `libfeas006_core.so` beside its executable,
loads typed `extern "C"` function pointers, and negotiates ABI version, pointer
width, and structure sizes at runtime. A separately compiled C header probe,
the Rust host layouts, and the loaded core all agreed on 40-byte options,
48-byte blocks, and 56-byte snapshots.

Console measurements ran five iterations for each of eight semantic and four
generated performance inputs. Reported console medians discard the first
iteration. GTK ran one fresh process for every input plus pre-cancelled and
observed-entry cancellation cases. The automated callback named
`first-useful-tick` is a GTK main-loop tick after synchronous buffer construction;
it is not GPU presentation, cold launch, or interactive frame pacing.

## Boundary and lifecycle findings

The host represents foreign statuses as raw `u32` values and rejects unknown
values before conversion. It validates payload and block counts, checked slice
ranges, sequential IDs, source spans, kinds, heading levels, and UTF-8 before
materializing host-owned strings and blocks. Snapshot and token owners use
explicit disarmed handles; fallback drops do not panic. No native pointer,
handle, `Library`, or GTK type escapes into the copied document.

Parsing and copying run on a worker. The host writes a `host-copy-released`
checkpoint after explicit snapshot release and before synchronous
`GtkTextBuffer` construction, so an externally retained layout timeout would
not hide successful ownership cleanup. GTK objects are created only on the main
thread. A disconnected or failed worker and checkpoint-write failure are
reported distinctly instead of waiting for the external timeout.

The final self-test passed:

- 1,000 sequential loads, including ten 5 MiB documents;
- 400 concurrent small-document loads;
- ten cancellation requests after observing native parse entry;
- typed ABI, argument, UTF-8, limit, cancellation, and invalid-handle errors;
- copied values surviving native release and host-conversion failure cleanup;
- fixed-message panic containment followed by a successful parse; and
- zero live snapshots and cancellation tokens after every lifecycle group.

All eight frozen outline contracts matched. The final matrix completed twelve
GTK presentations and two intended cancellations without a timeout or nonzero
exit. Pre-cancellation and observed-entry cancellation both returned
`Cancelled`; native owners were zero before GTK in both cases.

## Performance findings

The console isolates the candidate core/model call from host copy/decode. Warm
median results were:

| Input | Core/model | Native call | Copy/decode | End to end |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB prose | 5.49 ms | 5.53 ms | 0.57 ms | 6.11 ms |
| 5 MiB prose | 29.80 ms | 30.11 ms | 3.26 ms | 33.43 ms |
| 1 MiB delimiter storm | 11.58 ms | 11.64 ms | 0.52 ms | 12.19 ms |
| 1 MiB logical line | 0.75 ms | 0.78 ms | 0.51 ms | 1.30 ms |

Fresh GTK processes reached the first useful main-loop tick in about 277 ms for
1 MiB prose and 498 ms for 5 MiB prose, with sampled peak RSS of about 249 MiB
and 295 MiB respectively. The 1 MiB logical-line case took about 11.4 seconds
despite about 2 ms of combined core/copy work. This reproduces FEAS-005's main
conclusion: GTK is viable for the native shell and bounded text, but one
monolithic `GtkTextView` is not an acceptable unbounded document strategy.
LNX-001 must still prove virtualized semantic blocks, document-wide selection,
and AT-SPI reading order.

GTK emitted nonfatal theme `GtkGizmo` negative-slider-size warnings in automated
runs. No functional failure accompanied them, but the warning is retained rather
than silently removed from the evidence.

## Debugging and package findings

A GDB 18.1 batch run stopped inside Rust `md_parse_v1`, resolved the Rust source,
showed gtk-rs host and Rust-core frames across the indirect C call, and continued
to a normal exit. This proves Rust-to-Rust source debugging across the C ABI on
this host; it is not a mixed-language debugger comparison. GDB also reported a
missing optional auto-load script and disabled interactive debuginfod, neither of
which prevented source or frame resolution.

The development package contains the normal core library, release host, public
C header, fixtures/provenance, metadata, and license files for 72 locked Rust
dependency records. It contains no probe exports and the host has no core
`DT_NEEDED`; the host loads only its adjacent library by default. Package copies
have DWARF and debug-script sections removed and every packaged ELF is scanned
for checkout-root bytes. Separate ignored debug artifacts retain debug info.

A fresh external-directory relocation passed the 100-load normal lifecycle
check, console parse, and GTK smoke with `LD_LIBRARY_PATH` removed from every
command, including when the caller supplied a hostile value. GTK and compatible
system libraries remain external dependencies. This is same-host relocation,
not clean-machine, minimum-distro, Flatpak/AppImage, installer, sandbox, signing,
or publication evidence.

## Cross-platform interpretation

The three hosts support continuing with a coarse value-oriented boundary:

- Windows proved C#/C++ ownership, cancellation, native source/PDB debugging,
  normal-DLL packaging, and WinUI consumption, while continuous controls and
  virtualized selection/accessibility remain open.
- macOS proved Swift ownership, strict concurrency/main-actor checks, AppKit
  consumption, live Swift/Rust debugging, and relocated dylib bundles, while
  repeated-session allocator retention and synchronous presentation remain open.
- Linux proved runtime C-facade consumption from gtk-rs, host-owned values before
  GTK, GDB source frames, and a path-sanitized adjacent-library package, while
  monolithic layout, global selection, and AT-SPI behavior remain open.

The spike model is still incomplete: IDs are snapshot-local; inline/source
mapping, typed links/images, metadata, diagnostics, search, incremental updates,
and production resource policy are absent. Cooperative cancellation polls at
event boundaries and cannot interrupt parser-internal work. Unsafe calls still
depend on the documented pointer, alignment, and lifetime contract. These are
GATE-001 and implementation-plan inputs, not accepted production behavior.
