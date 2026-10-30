# CORE-006 shared facade acceptance

CORE-006 closes the shared facade/package aggregate under D-027. CORE-006A
(PR #7) delivered the hand-written C ABI; CORE-005A2/B2 (PRs #9/#11) added
find/read and workspace operations under D-022. This acceptance documents
the earlier CORE-005C default change and reconciles shared/native evidence
boundaries. It changes no runtime code, header layout, function signature,
dependency, model schema or golden bytes. Review/completion evidence lives in
the task PR and CORE-006 pass record in `project.yaml`.

The [C header](../core/include/markdown_core.h) is normative.
The [ABI contract](../specs/ffi/README.md) describes negotiation, ownership,
typed failures, borrowed view lifetime and host responsibilities.

## Shared acceptance evidence

| Criterion | Delivered checks |
| --- | --- |
| Version/model negotiation | `negotiation_reports_layout_and_rejects_mismatches` verifies ABI/layout reporting and rejected versions/sizes. Session creation tests reject unsupported models; default builds offer v1, `model-v2` builds also offer v2. Hosts explicitly choose a model. |
| Ownership and lifecycle | Typed registry handles reject stale, wrong-type and duplicate releases. Snapshot views survive newer generations and session release; repeated handles share an encoding. Threaded session and lifecycle tests return live handle counts to zero. |
| Outcomes and errors | Tests cover generations/status, UTF-8/parse limits, external changes and anchor results. A job returning `MD_OK` may still have a failed/cancelled/superseded outcome; hosts inspect both layers. Invalid argument/size/buffer tests verify output and progress guarantees. |
| Cancellation | Tests cancel before/during runs, from another thread, on session release and with late jobs. The host owns worker scheduling; the core starts no thread. Search functions without cancellation handles retain their documented bounded-call limits. |
| Panic containment | Export tests inject panics, including registry-lock poisoning and post-run failure, verifying typed failures and output handling. Packages must use `panic=unwind`; abort builds cannot provide containment. |
| Bulk semantic values | V1/v2 view round trips cover the corpus and v1 goldens, including source spans, metadata, outline, diagnostics, resource intents and v2 provenance/targets. No parser arena or Rust lifetime is exposed. |
| Search/read/workspace additions | Rust/C ABI parity tests cover match/segment/section fields, snippets, anchors, flags, caller arrays, classification and accounting. CORE-005 acceptance records the v1-index/reference-syntax boundary. |
| Current-host C package smoke | `c_smoke` builds the matching profile/feature library, stages a copy and compiles a C11 host. It compares struct sizes/offsets, checks the loaded-library path/model mask, exercises the ABI and completes 200 lifecycle iterations with zero live handles. Loader override environment variables are removed. |

Evidence is in
[export tests](../core/crates/markdown-ffi/src/tests.rs),
[workspace export tests](../core/crates/markdown-ffi/src/tests/workspace.rs),
[header/C package tests](../core/crates/markdown-ffi/tests/c_abi.rs), and
[the C host](../core/crates/markdown-ffi/tests/c/smoke.c).
Foreign callers must still supply valid memory with the documented lifetimes;
argument checks cannot prove arbitrary non-null pointers are mapped/readable.

## Earlier content-search default change

The ABI contract now states the required before/after history: before
CORE-005C (PR #25), zero `md_content_options.max_file_bytes` selected 8 MiB;
afterward it selects 2 MiB. This was an ABI v1 behavior change with no layout
or signature change, made before any native host shipped. An explicit 8 MiB
still restores the larger file budget and reader node/event limits.

The regression `content_memory_guard_defaults_match_the_rust_api` checks the
zero default, hostile/narrow-table skips and explicit maximum against Rust;
the C smoke checks the one-byte-over-default admission skip. This acceptance
documents existing behavior and does not change a default again.

## Validation

Baseline: `ead60fa628f2624bf85f85f03bb909dcbe2e1cf4` (`develop`, PR #37).
Local host: Apple M5 Max, macOS 27.0.1, Rust 1.99.1, Python 3.14.7,
Node 24.14.0. With the isolated environment in [Development.md](Development.md):

```sh
RUSTUP_TOOLCHAIN=stable CHECK_BASE=origin/develop .build/feas004-venv/bin/python tools/check.py all
```

CORE-006 requires fresh hosted release-mode Rust/FFI lifecycle, error,
cancellation and version tests on macOS, Linux and Windows. A normal task PR
uses the faster tier, so the existing `Checks` workflow is also dispatched
manually at this task's branch head: manual events select every host and run
both `core` and `core-extended`. Release evidence is the all-feature build;
default-feature-only release is not a separately tested configuration.

Local `all` and staged `common` checks passed. Fresh hosted
[Checks run 41024924816](https://github.com/awesomejt/markdown/actions/runs/41024924816)
passed at `4339669265c139fa493d4e73623ad9a9c278e503` on all three OSes,
including both tiers and the final `complete` gate. The release logs confirm
lifecycle, errors, cancellation, version/model negotiation, panic containment
and content-budget regressions; the macOS/Linux compiled C smoke also passed.
The completion pass records this run and the actual PR review disposition.

The compiled C smoke is required on macOS/Linux, including release. Windows
runs Rust export and header-constant checks; its MSVC/import-library C smoke
remains WIN-001B, as D-027 assigns. Portable hosted jobs do not prove a
Swift/AppKit, C# Native AOT or gtk-rs consumer or a signed installable package.

## Remaining gates and risks

Each 001B builds `model-v2` and requests v2 under
[D-025](../agents/Decisions.md#d-025--native-hosts-build-and-request-semantic-model-v2),
validates its native adapter/package and measures flat-view encode/copy cost,
retained RSS and session memory before setting a source bound. Packaging must
preserve `panic=unwind`, symbols and loader paths on the named host.
The staged C smoke is a current-host library/host check, not a replacement
for those consumer, clean-machine, signing or distribution checks.

The known dense reader parse can peak near 1,152 MiB at 8 MiB input, and the
Mac has a separate repeated-session RSS-retention risk. Snapshot views are
borrowed until release; hosts copy needed UI data before releasing a handle
and must not race release against reads. Search work remains off the UI thread;
the handle-free functions cannot interrupt an in-progress call.

Closing CORE-006 completes the portable-core phase and activates the desktop
chains. On this Mac the next task is MAC-001A. It also reaches the D-030
shared-core milestone review boundary; only the owner merges that milestone
into `main`. No native implementation evidence is inferred from this closeout.
