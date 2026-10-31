# macOS core package and adapter (MAC-001B)

Baseline: `20cbc44d72342615c869ad8c428bdfc779406f38` (PR #39).
This phase adds a native Swift consumer of the production
[C ABI](../specs/ffi/README.md). Rust source, public header, schema, goldens and
dependency graph are unchanged. The app validates the linked ABI at startup;
its window remains an empty shell. File access and UI publication are MAC-001C,
and document rendering starts in MAC-001D.

## Package and ownership

`apps/macos/tools/package_core.py` builds `markdown-ffi` with `model-v2`,
`panic=unwind`, a macOS 27 deployment target and `aarch64-apple-darwin`.
The generated, ignored `apps/macos/CorePackage/` holds one selected profile's
static archive, the exact public header, a Clang module map, Apache license,
and a manifest with profile/toolchain/commit, header/archive/lock hashes and
the 24 native `md_` exports. No binary or generated package is committed.

There is exactly one `libmarkdown_ffi.a` per staged package. The native build
tool selects a matching Rust and Swift profile. A generated header embeds its
profile and archive fingerprint, invalidating the imported module when the
archive changes. Tests compare that fingerprint with the actual archive bytes
and the Swift compilation configuration. This avoids the observed Xcode
SwiftPM path putting both conditional debug/release library settings on a
release link line. Preliminary measurements from that earlier mixed link were
discarded. The release probe refuses a non-release core package.

The symbol audit uses `nm --no-llvm-bc -gU`: Apple's LLVM reader cannot decode
some optional bitcode from Rust's newer LLVM. Native exports are checked, and
actual debug/release linking, hash checks and consumer tests remain required.
This does not waive a failed native symbol or link check.

The static archive is linked into the app/probe; no Rust dylib, install name
or runtime build-tree lookup is needed. Checks copy the assembled app and a
release probe to temporary directories, remove `DYLD_*` overrides and launch
there. This is local relocation evidence, not signed installation, universal
architecture, XCFramework, sandbox, notarization or clean-machine acceptance.

`MarkdownCore` contains no UI types or filesystem operations. Negotiation
compares all reported layouts, pointer/anchor versions and the addition size
constants. Every production session explicitly requests `MD_MODEL_V2` (D-025).
API status failures are typed Swift errors; job outcomes and parse failures
remain separate typed values. Unknown outcome/model/layout values fail closed.

`CoreDocument` copies source, string table and every flat record/index array
before releasing the snapshot handle. Its imported C records contain only
fixed-width scalar values/string offsets, with no pointers. The documented
`@unchecked Sendable` conformance covers those immutable, owned values.
The borrowed `md_snapshot_view` never escapes. String access checks range and
UTF-8 validity; spans remain UTF-8 byte offsets, not TextKit UTF-16 offsets.
Node IDs remain snapshot-local (D-021). Submission generation is on
`CoreResult`; an unchanged snapshot can retain an older snapshot generation.

One serial worker runs submission copying, parse, encode, Swift value copying
and model destruction. At most two parse calls are admitted globally (one
running, one queued); further calls fail with `CoreError.busy` rather than
retaining an unbounded queue of source buffers. Task cancellation is sticky
before job installation and uses a lock around job cancel/release. Cancelling
a queued call immediately releases its admission slot and owned input, so a
replacement can enter before the worker runs. Running calls retain their slot
until cleanup finishes. `close()`
detaches/cancels promptly, awaits worker destruction, and shares completion
across concurrent close callers; deinit schedules a fallback cleanup. Callers
should explicitly await close. MAC-001C still owns permission checks, bounded
reads, latest-request UI ordering and ignoring results for closed windows.

MAC-001F adds `CoreSession.find`, which calls `md_snapshot_find`
(CORE-005A2) on the same serial worker, after any queued parse, over a new
`md_session_current` handle that it releases before returning. Only that
worker publishes snapshots, so checking the published generation from
`md_session_get_status` under the session lock, before taking the handle,
guarantees that node indices belong to the snapshot the window rendered. Any
other generation fails with `CoreError.superseded`. Find takes no admission
slot: it holds only its query (at most 4,096 bytes, otherwise
`CoreError.queryTooLong`) and caller-owned result arrays sized from
`max_matches`. At the default 10,000 matches those are 240 KiB of matches and
480 KiB of segments, with one retry when matches span more nodes. A find
cancelled before the worker reaches it never calls the core. A running call
cannot be interrupted, so callers drop superseded results. Results are owned
Swift values whose segment ranges are UTF-8 offsets into node display text;
`MarkdownRender` maps them to TextKit ranges.

## Host limits and measured costs

Selected host policy:

| Resource | Limit |
| --- | ---: |
| Session source | 5 MiB |
| Nodes | 500,000 |
| Events | 1,000,000 |
| Copied parser text | 16 MiB |
| Swift snapshot payload copy | 128 MiB |
| Running heavy operations | 1 |
| Admitted parse calls, including running | 2 |

The source limit preserves the accepted 5 MiB prose profile (461,177 nodes,
about 932,000 events); all four frozen profiles pass with these limits. A
larger source produces `CoreParseFailure.sourceLimit`; density can produce
the normal node/event failure. The copy budget is checked before allocating
Swift arrays, after the core has encoded the view. These are per-call/queue
limits, **not a total app RSS bound**. Caller buffers and retained UI snapshots
also cost memory; MAC-001C must bound reads before constructing input Data.

Measured 2026-10-30 on Mac17,6, Apple M5 Max/arm64, 48 GiB RAM, macOS 27.0.1
(26A434), Xcode 27.1 (27A9275), Swift 6.4, SDK 27.0, Rust 1.99.1.
Both Swift and Rust use release builds. Five fresh-process trials use the
frozen `prose-5mib.md` (SHA-256
`43086da373aa098b0a03817236f52c82d7b5383d7564ee41098e75c58246c715`),
with warm filesystem caches and no concurrent validation suite. These are
operation costs, not D-012 launch-to-presented-content measurements.

| Operation, 5 MiB prose | Min ms | Median ms | Max ms |
| --- | ---: | ---: | ---: |
| Native job run | 160.6 | 165.8 | 177.2 |
| First flat-view encode | 33.2 | 33.4 | 36.8 |
| Owned Swift payload copy | 2.3 | 2.4 | 2.5 |
| Awaited host parse call | 198.8 | 201.7 | 213.5 |
| Awaited close/destruction | 10.8 | 10.9 | 11.1 |

The payload copy is 50,086,584 bytes (47.8 MiB), including source and all
tables; it excludes Swift container overhead. Whole-process peak RSS is
241.7–241.9 MiB; after dropping the result and closing the session, retained
RSS is 112.2–112.4 MiB. RSS comes from Mach task statistics and process peak
from `getrusage`, cross-checked against `/usr/bin/time -l` (bytes on this host).

| Fresh-process dense probe | Result | Peak process RSS MiB |
| --- | --- | ---: |
| 8 MiB `*a` repeated, raw C session/core defaults | NodeLimit | 577.9 |
| 5 MiB `*a` repeated, production host limits | NodeLimit | 361.8 |

The raw 8 MiB probe deliberately bypasses host limits to measure the existing
core default. These are resident process measurements; the earlier roughly
1,152 MiB **counted heap** maximum in [Parser-Budgets.md](Parser-Budgets.md)
is a different metric and is not superseded. Lowering source size reduces
the measured per-call peak while retaining the accepted prose corpus.

## Repeated-session memory

MAC-001B measured twenty serial new-session/parse/copy/close cycles of the
same 5 MiB profile in one process: 112.2 MiB RSS retained after cycle 1 and
1,698.1 MiB after cycle 20, with zero native handles, malloc `size_in_use`
near 5.2 MiB and no leak. MAC-001G found the cause and resolved it.

- The physical footprint (`phys_footprint`, which the probe now reports
  beside RSS) grew the same way, so the pages were not reclaimable. `vmmap`
  put 1.6 GiB in 245 "Malloc Large (empty)" regions: freed large blocks the
  allocator kept resident and dirty.
- A raw C-ABI loop without the Swift copy grew by the same ~80 MiB per
  cycle, so the core's own allocations were responsible.
- A minimal C program reproduces it on macOS 27.0.1: growing one buffer with
  `realloc` to 16 MiB leaks about 8 MiB per cycle and to 64 MiB about
  32 MiB, without bound. Fixed-size `malloc`/`free` and allocate-copy-free
  growth plateau instead. `malloc_zone_pressure_relief` changed nothing.
  The undocumented `MallocLargeCache=0` and `MallocSpaceEfficient=1`
  environment settings kept retention flat (8–9 MiB) and confirmed the
  diagnosis; they are not used.

**Fix.** `markdown-ffi` installs a global allocator on Apple targets only
(`core/crates/markdown-ffi/src/apple_alloc.rs`). It forwards every call to
the system allocator, except that a reallocation whose old or new size
reaches 1 MiB allocates, copies and frees. Linux and Windows are unchanged.
The static library therefore defines Rust's global allocator: a host that
linked a second Rust static library would need one combined Rust build.
The probe's twenty 5 MiB cycles now retain a flat 199 MiB footprint from
cycle 2 (54 MiB after cycle 1), the allocator's reused cache of freed large
blocks; the awaited parse call is unchanged at about 190–200 ms. Prose
1 MiB retains 57–58 MiB and long lines 8–9 MiB.

Using `mmap` and `munmap` for blocks of 4 MiB and more was also measured: it
retained 65 MiB but made each 5 MiB parse 15–20 ms slower and needs raw
system calls, so it is not shipped.

The whole app, which also keeps rendered text and layout, retains 227–581 MiB
over twenty 5 MiB loads and 461–572 MiB over forty, rising slowly and
dropping back as malloc trims (launch baseline 28 MiB). Without the wrapper
it reached 1,838 MiB after twenty loads. See the
[app evidence](../apps/macos/README.md#mac-001g-local-evidence). These are
measured bounds on this host, not a numeric whole-app RSS budget.

## Reproduction and validation

Follow [the Mac build instructions](../apps/macos/README.md), including the
isolated local Rust environment when needed. Run:

```sh
RUSTUP_TOOLCHAIN=stable .build/feas004-venv/bin/python apps/macos/tools/check.py
```

It tests matching debug/release packages, actual archive fingerprints,
negotiation/layout/typed failures, v2 provenance and metadata, unchanged/new
generations, owned data after release, queued/in-flight cancellation, close
during work, concurrent close, 200 lifecycle iterations, bounded admission,
all semantic fixtures and all four performance profiles. Relocated AppKit
bundle smoke and a standalone release consumer also run. No build/runtime
dependency on the sibling repository exists.

For measurements, first prepare/build release, then use the reported SwiftPM
release binary directory:

```sh
swift build --package-path apps/macos --configuration release --show-bin-path
# In that directory, invoke CoreProbe with:
# single build/fixtures/performance/prose-5mib.md  (five fresh processes)
# repeated build/fixtures/performance/prose-5mib.md
# dense-default
# dense-host
```

The probe reports counts/times/memory only, not document text, IDs or paths.
Raw logs stay ignored. CI and Grok disposition are recorded in the PR and
project-state completion pass. Next: MAC-001C, file capability and off-thread
loading/error presentation, using this adapter and its host bounds.
