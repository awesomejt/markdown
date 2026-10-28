# Shared CORE review — Grok

- **Date:** 2026-10-28
- **Reviewer:** Grok (xAI), independent review of the shared CORE phase
- **Baseline:** `review/shared-core` at `146ea51` (implementation equivalent to `main` at `39d1a56` plus prior review commits)
- **Scope:** CORE-001A through CORE-006A (workspace scaffold, v1/v2 semantic models, bounded parsing with extensions, sessions/anchors, document & workspace search, hand-written C ABI). Platform shells, rendering, and ADV phases out of scope.
- **Purpose:** Holistic assessment for the upcoming compare-notes phase. Complements the Claude and Codex reviews; focuses on practical readiness for native adapters, residual risks that should be prioritized together, and one additional consistency observation.

## Verdict

The shared core is a strong, carefully engineered foundation. Ownership boundaries are enforced in code (`#![forbid(unsafe_code)]` in `markdown-core`, no I/O, capability-based intents that grant nothing). The C ABI is disciplined: registry handles, panic containment, exact struct-size negotiation, and no Rust lifetimes crossing the boundary. Validation is independent and strict. Tests are deep (conformance, hostile mutation, budgets, session races, FFI layout/panic). Known limits are documented honestly in Parser-Budgets.md.

No correctness, memory-safety, or arbitrary-file-access defect was found in the inspected code. The main practical risks for the next phase are the already-identified ones: memory amplification under pathological input, the default model’s restrictiveness for real documents, and import-policy parity. One additional consistency note (workspace content search always uses v1) is called out below.

The core is ready for the platform 001A shell phases. The 001B adapter phases should not begin until the model-version decision (D-025) and the memory/search defaults are aligned across hosts.

## Evidence

- Full tree and key modules read: `parse.rs` (URI policy, builder, limits), `validate.rs`, `session/`, `search/`, `workspace/` (path, content, snippet), `markdown-ffi` (registry, boundary helpers, flat view).
- Contracts and budgets: Architecture.md, Parser-Budgets.md, specs for semantic model, session, search, workspace, and C ABI.
- Prior reviews (Claude F1–F7, Codex C1–C7) cross-checked against the code.
- Local inspection of pinned dependencies, Cargo.toml feature gating, and `#![forbid(unsafe_code)]`.
- Not performed: full test suite re-run (prior reviews already passed `tools/check.py all`), native host measurements, coverage-guided fuzzing, or dependency advisory scan.

## What is working well

- **Boundaries are real.** The core never opens paths, creates windows, or persists preferences. Sessions receive bytes from the adapter. Workspace search uses opaque entry IDs and validated display paths only. Link/resource intents are pure data.
- **FFI is production-grade.** Handles are keys into a process-wide registry; stale/double-release is detected. Every entry point is wrapped in `catch_unwind`. Outputs are zeroed on entry and written once after fallible work. Versioned structs require exact `struct_size`. The flat view is a second, immutable encoding owned by the snapshot handle.
- **Failure modes are closed.** Limits produce typed errors without source text. Offset conflicts localize to the affected top-level block rather than discarding the whole document. Cancellation is cooperative and polled.
- **Testing depth.** Corpus reconciliation, 4 000 mutation cases, pathological catalog, counted heap budgets, and FFI tests for panics and stale handles give high confidence in the current surface.
- **Documentation honesty.** Residual risks (memory amplification, uninterruptible phases, lack of coverage-guided fuzzing) are written down rather than papered over.

## Findings

Severity reflects impact on the upcoming platform phases, not code defects.

### G1 — High (existing, confirmed): memory amplification is the dominant platform risk

Parser-Budgets.md and both prior reviews already document this. A 5 MiB prose document peaks at ~145 MiB and retains ~100 MiB. An 8 MiB dense delimiter storm peaks at ~1 152 MiB before `NodeLimit`. Workspace content search defaults `max_file_bytes` to 8 MiB and parses every admitted file. The FFI flat view is a second copy. macOS feasibility work showed retained RSS growth across repeated parses.

**Recommendation (aligns with Claude F2 / Codex C1 / CORE-005C):**  
Before any platform 002C (workspace search UI), lower the content-search file default (e.g. 2 MiB) and/or add a cheap density pre-check that produces a typed skip. Measure flat-view encode cost and retained RSS in each 001B phase. Keep host concurrency under control. Do not treat the current node/event ceilings as a practical memory budget.

### G2 — High (existing, confirmed): default model is too restrictive for real documents

v1 is the default in Rust and in the default `markdown-ffi` build. Its `safe_uri`/`local` policy rejects parent-relative paths, percent-encoding, and non-ASCII filenames. v2 (opt-in) handles these under the reviewed URI policy. D-025 (accepted) directs native packages to build with `model-v2` and request `MD_MODEL_V2`.

**Recommendation (aligns with Claude F1):** Record and enforce D-025 before the first 001B phase so the three hosts do not diverge. Keep v1 for compatibility and goldens.

### G3 — Medium (existing, confirmed): import-policy gap on both v1 and v2

v1 validation uses looser `relative`/`external` checks than the parser. Shared metadata validation does not re-apply the producer’s control/Cf filter to imported titles or string properties. v2 re-uses the same structural validator. A future cache or Rust consumer can therefore treat a “validated” document as producer-policy-checked when it is not. The C ABI has no JSON import today, and native sessions parse source, so this is not an active exploit path.

**Recommendation (aligns with Claude F3 / Codex C2 / CORE-002B):** Extend the accepted CORE-002B work to cover both model versions’ metadata strings. Prefer re-using the producer’s classification functions over documenting the gap as “structural only.”

### G4 — Medium (new observation): workspace content search is hard-coded to v1

`workspace/content.rs` always calls `parse_markdown_cancellable` (v1) regardless of the session’s model version. With D-025 selecting v2 for native hosts, document-level and workspace-level views of the same file can differ in capabilities, link dispositions, and diagnostics. Search results themselves are source-mapped and mostly insensitive to this, but any future feature that surfaces outline, alerts, or link intents from the workspace scan will see the v1 view.

**Recommendation:** Before 002C, either:
- make content search accept the same model version the session uses, or
- explicitly document that workspace search is a v1 text index and keep any richer semantics on the document session.

Prefer the first if the cost of a second v2 parse is acceptable; the budgets doc already shows v2 is only modestly more expensive on dense-link input.

### G5 — Low (existing): documentation drift and missing owner decisions

MEMORY.md, HANDOFF.md, and some decision statuses are stale relative to merged state. Bare GFM autolinks are required by R1.2 but deliberately disabled, with no recorded disposition. Filename invisible-character warnings are Cf-only (Hangul fillers, Braille blank, etc. are not flagged). Snippet contract text slightly over-promises normalization.

**Recommendation (aligns with Claude F5 / Codex C3/C4/C6 / DOC-001):** One docs-only pass that trims the required-reading files, records the bare-autolink decision, and clarifies the snippet and filename-warning contracts. These are low-risk and high-leverage for the next agent/human handoff.

### G6 — Low: remaining quality gates for a public release

- Windows C ABI smoke is still skipped until WIN-001B; make the MSVC compile a required CI step there.
- No coverage-guided fuzzing of the parser, model decoder, or safe FFI inputs.
- No current dependency advisory scan was run in this review (pins are tight, but the check is still useful).

These do not block the platform phases; they belong on the path to a public release.

## Decisions and ordering for the compare-notes phase

Agree with the prior reviews’ proposed order:

1. **Docs cleanup (DOC-001)** — low cost, removes ambiguity for every subsequent session.
2. **CORE-002B** (v1/v2 import-policy parity + host guidance for non-cancellable FFI search) — small, closes a real validation gap.
3. **CORE-005C** (content-search memory defaults / density guard) — required before any 002C.
4. **Platform 001A shells** can proceed in parallel; they depend only on CORE-001A.
5. **Enforce D-025** and the flat-view/RSS measurements in every 001B before those phases start.
6. Address G4 (workspace model consistency) as part of CORE-005C or a tiny follow-up.

No finding requires a redesign of the architecture. The small, already-accepted follow-ups plus the model-version consistency note are sufficient to keep the three native hosts aligned.

## Summary for the owner

The shared CORE phase is in good shape. The code quality, security posture, and test depth support proceeding to platform work. The items that should be settled together in the compare-notes phase are the model version default (D-025), the memory/search defaults (CORE-005C), the import-policy parity (CORE-002B), and the documentation cleanup. Everything else can wait for the relevant platform acceptance gates or a later release hardening pass.
