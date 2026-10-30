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
before job installation and uses a lock around job cancel/release. `close()`
detaches/cancels promptly, awaits worker destruction, and shares completion
across concurrent close callers; deinit schedules a fallback cleanup. Callers
should explicitly await close. MAC-001C still owns permission checks, bounded
reads, latest-request UI ordering and ignoring results for closed windows.

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

## Repeated-session risk remains open

Twenty serial new-session/parse/copy/close cycles of the same 5 MiB profile in
one process retained 112.2 MiB RSS after cycle 1 and 1,698.1 MiB after cycle 20.
All three native handle counts returned to zero on every cycle. Malloc
`size_in_use` stayed about 5.1–5.2 MiB (including the input buffer), and a
separate `leaks --atExit` run reported zero leaked bytes. This is consistent
with the previously observed macOS allocator retention; it does not establish
an acceptable repeated-load RSS budget. Explicit autorelease pools and moving
destruction onto the worker did not solve it. An experimental allocator
pressure-relief call also had no material benefit and is not shipped.

MAC-001B meets its package/consumer and **measurement/source-bound** criteria.
MAC-001G must resolve or explicitly obtain owner disposition of repeated-load
memory before the native skeleton aggregate is accepted. No application-wide
memory, renderer, VoiceOver, startup or scroll budget is claimed by this work.

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
