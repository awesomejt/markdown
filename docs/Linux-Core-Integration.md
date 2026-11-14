# Linux core package and adapter (LNX-001B)

Baseline: `8084c5e` (`develop` after PR #49).
This phase adds a native Rust consumer of the production
[C ABI](../specs/ffi/README.md) to the [GTK 4 shell](../apps/linux/README.md).
Rust core source, the public header, schemas, goldens and the core dependency
graph are unchanged. The app validates the linked ABI at startup and its smoke
parses through the adapter; the window remains an empty shell. File access is
LNX-001C, and document rendering starts in LNX-001D.

## Package and linking

`apps/linux/tools/package_core.py --configuration <debug|release>` builds
`markdown-ffi` with `model-v2`, `panic=unwind` and the pinned Rust 1.99.1 for
`x86_64-unknown-linux-gnu`, into `build/linux/rust`. It stages, under the
ignored `build/linux/core-package/<configuration>/`, the shared library
`lib/libmarkdown_ffi.so`, the exact public header, the Apache license, a
manifest (target, models, profile, toolchain, commit, header/library/lock
hashes and exports) and a two-line `identity` file for the build script. The
package directory is replaced whole, so a failed run never leaves a mixed one.
No binary or generated package is committed.

The audit requires the library's dynamic symbol table to define exactly the
24 `md_` functions of the header and nothing else. The library needs only
`libc.so.6`, `libgcc_s.so.1` and the dynamic loader, has no `SONAME`, and
contains no checkout or home-directory path (the build remaps both prefixes,
as on macOS).

The Rust host links the **cdylib**, not the staticlib the Mac uses. A Rust
static archive carries a private copy of `std`; linked into a Rust binary it
worked in an unoptimized build but failed the release link (thin LTO) with
hundreds of duplicate `std`, `alloc` and `rustc_demangle` symbols. The cdylib
exports only the C functions, so the two standard libraries never meet, which
is also the shape FEAS-006 proved on this host. Both the core and the host
allocate through glibc `malloc`, and ownership never crosses the boundary
anyway: snapshots are released through `md_snapshot_release`.

The adapter crate's `build.rs` selects the package matching Cargo's `PROFILE`
(`debug` for builds, tests and Clippy; `release` for `--release`) and refuses
a package of the other configuration, so a debug host cannot link a release
core or the reverse. `MARKDOWN_CORE_PACKAGE` overrides the directory. It
embeds the package configuration and library hash, which the tests compare
with the staged `identity`, and `tools/check.py` re-verifies the library hash
against the manifest after each build. Development binaries find the library
through a `RUNPATH` relative to `$ORIGIN` (with a second entry for test
binaries in `deps/`), so no absolute path is embedded when the package and
target directory share a root. The GTK app has its own small build script,
because Cargo does not pass a dependency's link arguments to dependents.

This is a local development layout, not an installation: Flatpak and Arch
packaging (LNX-001I) install the library and set the installed binary's
`RUNPATH`, and add the notices for the core's linked Rust packages.

## Adapter and ownership

`apps/linux/core` (crate `markdown-linux-core`) is a member of the Linux
workspace with no dependencies and no UI types. Its `ffi` module hand-writes
the header's records and the 14 functions it uses (D-009); find, reading
statistics, anchors and workspace search are declared when a phase adopts
them, and their size constants are then checked the same way. `validate()`
negotiates ABI v1 and requires model v2, anchor version 1, the anchor size,
the pointer size and every negotiated record size to match the Rust layouts.
The app runs it before creating the GTK application and exits with an error
otherwise.

Every `CoreSession` requests `MD_MODEL_V2` (D-025) with the host limits
below. API status failures are `CoreError::Status`; job outcomes and parse
failures are separate typed values, and an unknown outcome or a failed
outcome without a known failure fails closed.

`CoreDocument` copies the source, string table and every flat record and
index array out of the view while the snapshot handle is live, then the
handle is released. The records are the header's fixed-width scalar and
string-offset structs, with no pointers, so the document is plain owned data
(`Send + Sync`) that outlives its session. View arrays must be non-null and
aligned when non-empty. After copying, one pass bounds-checks every
cross-table index and range (roots, children, parents, child ranges, node and
intent spans, outline and diagnostic nodes, property string lists and every
string range) and returns `CoreError::InvalidSnapshot` on any violation, so a
core bug cannot become an index panic in the renderer. String access also
checks UTF-8. Spans remain
UTF-8 byte offsets and node IDs stay snapshot-local (D-021). LNX-001H decides
whether a document keeps its snapshot handle for anchors, as MAC-001H did.

## Worker, admission, cancellation and close

One process-wide worker thread (`markdown-core`) runs submission, parse,
flat-view encode, copy and every session release, never the GTK main thread.
At most two parse calls are admitted **process-wide**, across every window
and session (one running, one queued); another returns `CoreError::Busy`
rather than queuing more source buffers. Two windows reparsing at once make a
third window's parse busy, so LNX-001C coalesces or retries rather than
reporting `Busy` to the reader. The
caller's source moves into the request and is dropped as soon as the core has
copied it.

`CoreSession::parse` takes a completion callback that runs exactly once: on
the worker when the call ends, or on the cancelling thread when a queued call
is cancelled. Cancelling a queued request answers `Cancelled` at once, drops
its input and frees its admission slot before the worker reaches it, so a
replacement can enter. Cancelling a running request calls `md_job_cancel`
under the request lock, which also guards the job release, and the result is
`Cancelled`. A panic inside the worker-side call still calls back
(`CoreError::Internal`) and releases admission. `parse_blocking` waits for the
callback and is refused on the worker (`CoreError::OnWorker`), as is `close`.

`close()` detaches the handle and cancels in-flight work immediately, then
waits for the release on the worker; concurrent and repeated callers share
one result. That release queues behind every call already on the shared
worker, including another window's parse of up to about half a second, so
`close()` must not run on the GTK main thread. `close_then` detaches the same
way but returns at once and calls back when the release has finished; window
teardown uses it or a plain drop. Dropping an unclosed session detaches and queues the release
without waiting. Work queued before a close finds the handle detached and
returns `CoreError::Closed`. LNX-001C owns permission checks, bounded reads,
newest-request UI ordering and ignoring results for closed windows.

## Host limits

| Resource | Limit |
| --- | ---: |
| Session source (`max_source_bytes`) | 5 MiB |
| Nodes | 500,000 |
| Events | 1,000,000 |
| Copied parser text | 16 MiB |
| Owned snapshot copy | 128 MiB |
| Running heavy operations | 1 |
| Admitted parse calls, including running | 2 |

These match the macOS host. The source limit keeps the accepted 5 MiB prose
profile (461,177 nodes) and every frozen profile within limits, while the
measured dense peak at 5 MiB (379 MiB) is well below the 652 MiB of the core's
8 MiB default on this host. A larger source fails with
`CoreParseFailure::SourceLimit`; density can produce the node or event
failure. The copy budget is priced from the view's counts, with checked
arithmetic, before any array is allocated. These are per-call and queue
limits, **not a whole-app memory bound**.

## Measurements

Measured 2026-10-31 on an AMD Ryzen 9 5950X (x86-64, 64 GiB), CachyOS with
kernel 7.3.0-4-cachyos and glibc 2.44, Rust 1.99.1, release core and release
probe (`build/linux/target/release/core-probe`), warm filesystem cache and no
concurrent build. Five fresh processes parsed the frozen `prose-5mib.md`
(SHA-256 `43086da373aa098b0a03817236f52c82d7b5383d7564ee41098e75c58246c715`)
through `CoreSession`. These are operation costs, not D-012
launch-to-content measurements.

| Operation, 5 MiB prose | Min ms | Median ms | Max ms |
| --- | ---: | ---: | ---: |
| Native job run (parse and model) | 345.0 | 356.7 | 386.2 |
| First flat-view encode | 103.7 | 116.8 | 120.5 |
| Owned Rust copy and index check | 7.8 | 8.7 | 9.2 |
| Blocking host parse call | 460.0 | 483.3 | 513.0 |
| Awaited close and destruction | 25.1 | 26.6 | 28.0 |

The copy row was re-measured (five more fresh processes) after the review
added the index check, which costs about 2.3 ms over the 6.4 ms median copy
alone; the other rows of that run matched within 3%. The copy is
50,086,584 bytes (47.8 MiB) for all tables and the source, the
same as macOS. Process RSS is 8.1 MiB before the session, 214–216 MiB with the
result held, and peaks at 245–246 MiB. After dropping the result and closing
the session, RSS is 129.7–129.8 MiB, but glibc's in-use heap (`mallinfo2`) is
5.0 MiB and `malloc_trim(0)` returns RSS to 45.8–45.9 MiB: the retained RSS is
allocator caching of freed memory, not live data. RSS and the per-trial peak
come from `/proc/self/status` (`VmRSS`, `VmHWM`, reset through
`/proc/self/clear_refs` before each trial).

Twenty sessions in one process (`core-probe repeated`) keep zero native
handles after every close. Retained RSS settles at 136.5–142.1 MiB after a
first-cycle high of 170.7 MiB, and per-cycle peaks stay at 254–266 MiB; after
`malloc_trim` the process returns to 22.4–22.6 MiB from the fourth cycle on.
The in-use heap after each close grows from 5.04 to 5.08 MiB over the twenty
cycles (about 2 KiB per session). That growth is below anything that would
cause a problem here and was not traced further. Unlike MAC-001B, repeated
loads do not grow RSS on this host.

| Fresh-process dense probe, three runs | Result | Peak RSS MiB |
| --- | --- | ---: |
| 8 MiB `*a` repeated, core defaults (`dense-default`) | NodeLimit | 652.2–652.7 |
| 5 MiB `*a` repeated, host limits (`dense-host`) | NodeLimit | 378.0–379.2 |

`dense-default` deliberately bypasses host policy by passing zero limits; the
probe's own copy of its input is included. `/usr/bin/time` reported 651–653 MiB
maximum resident size for the same mode. The dense runs parse in about 258 ms
(5 MiB) and 455–471 ms (8 MiB) before the node limit stops them.

Parsing is about twice as slow here as the 166 ms MAC-001B measured on an
Apple M5 Max, and the first flat-view encode three times as slow (33 ms
there). Both are measured costs on this host, not regressions against a
Linux budget; LNX-001G owns large-document and launch budgets. The allocator
caching above suggests LNX-001G should evaluate `malloc_trim` after closing a
large document.

## Reproduce

From the repository root, in a graphical session:

```sh
python3 tools/generate_performance_fixtures.py
python3 apps/linux/tools/check.py
for i in 1 2 3 4 5; do build/linux/target/release/core-probe single build/fixtures/performance/prose-5mib.md; done
build/linux/target/release/core-probe repeated build/fixtures/performance/prose-5mib.md
build/linux/target/release/core-probe dense-host
build/linux/target/release/core-probe dense-default
```

The probe refuses a debug package. Each line is one JSON object; byte fields
are bytes.
