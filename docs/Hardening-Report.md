# Shared Core Release Hardening Report (HARD-001)

This report records the `HARD-001` release hardening evidence for
`markdown-core` and `markdown-ffi` ahead of `DESK-002`: coverage-guided
fuzzing, sanitizer verification, and dependency advisory scanning.

## Named Host and Toolchain

- **OS / Kernel:** Linux (CachyOS / Arch Linux `x86_64`)
- **CPU / Memory:** AMD Ryzen 9 5950X
- **Rust:** `rustc 1.99.1 (58cf8f9c3 2026-10-13)`
- **C/Fuzzer Toolchain:** `clang 23.1.4` (`libclang_rt.fuzzer-x86_64.a`)

## 1. Coverage-Guided Fuzzing

[`core/fuzz/`](../core/fuzz/README.md) defines four coverage-guided libFuzzer
targets compiled with Rust sanitizer coverage (`-C passes=sancov-module`,
`inline-8bit-counters`, `pc-table`, `trace-compares`) and linked with
`clang -fsanitize=fuzzer,address,undefined` via [`tools/fuzz.py`](../tools/fuzz.py):

| Target | Scope | Duration | Units executed | Exec/s | New corpus units | Peak RSS | Crashes / leaks |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `fuzz_parser` | `parse_markdown`, `parse_markdown_v2`, cancellable variants, JSON round-trip | 16 s | 2,378 | 148 | 242 | 465 MB | 0 |
| `fuzz_model_decoder` | `Document::from_json` (v1) and `v2::Document::from_json` | 16 s | 654,575 | 40,910 | 2,315 | 576 MB | 0 |
| `fuzz_ffi_session` | `md_abi_negotiate`, `md_session_*`, `md_job_*`, `md_snapshot_find`, `md_snapshot_read_stats`, `md_snapshot_anchor_*`, `md_snapshot_resolve_anchor` with valid backing buffers and `md_live_handles == (0,0,0)` | 16 s | 26,050 | 1,628 | 1,198 | 467 MB | 0 |
| `fuzz_ffi_workspace` | `md_workspace_classify_path`, `md_workspace_find_names`, `md_workspace_content_admit`, `md_workspace_content_search` with valid backing buffers | 16 s | 144,998 | 9,062 | 2,802 | 469 MB | 0 |

Seed corpora for `fuzz_parser` and `fuzz_ffi_session` include `fixtures/markdown/*.md`
plus a nested image wikilink input (`![[` × 128).

## 2. Sanitizer Verification

- **Fuzzers (ASan + UBSan):** All four fuzz targets ran with AddressSanitizer
  across the Rust staticlib and C harness and UndefinedBehaviorSanitizer on the
  C `LLVMFuzzerTestOneInput` harness, with zero sanitizer findings.
- **Workspace Test Suite (ASan):** Executed
  `RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Z sanitizer=address" cargo test --workspace --all-features --locked --target x86_64-unknown-linux-gnu --target-dir build/asan-target`:
  - All 14 `markdown-core` integration/unit test binaries (206 tests) and all 36
    `markdown-ffi` unit tests plus `header_constants_match_rust` (243 tests
    total) passed with zero AddressSanitizer or LeakSanitizer findings.
  - `c_abi::c_smoke` did not run under ASan because it spawns a nested
    `cargo build` without `--target`, causing proc-macros to fail when
    inheriting `-Z sanitizer=address`.
- **Hostile / Pathological Regression:** Added `"nested image wikilinks"`
  (`![[` × 25,000 + `]]` × 25,000) to `core/crates/markdown-core/tests/hostile.rs`
  `catalog()`, guarded by the 5-second release ceiling.

## 3. Dependency Advisory Scan

[`tools/check_advisories.py`](../tools/check_advisories.py) scans
`Cargo.lock` (59 crates.io packages), `apps/linux/Cargo.lock` (64 crates.io
packages), and `core/fuzz/Cargo.lock` (54 crates.io packages; 120 unique
packages total) against the OSV.dev RustSec batch API and is wired into
`tools/check.py core` (with unit tests in
[`tools/test_check_advisories.py`](../tools/test_check_advisories.py) wired
into `tools/check.py common`).

| Advisory | Package | Status | Disposition |
| --- | --- | --- | --- |
| `RUSTSEC-2026-0433` | `pulldown-cmark 0.13.5` | Dispositioned | Upstream quadratic parsing of nested image wikilinks requires `Options::ENABLE_WIKILINKS`. `PARSER_OPTIONS` in `core/crates/markdown-core/src/parse.rs` does not enable `ENABLE_WIKILINKS`, and `hostile.rs` verifies linear/bounded handling. Upgrading `pulldown-cmark` is a separate gated task per `TODO.md`. |
| `RUSTSEC-2026-0592` | `url` (was 2.5.8) | Resolved | Stack overflow in `Url::origin()` on nested `blob:` URLs. The core pins `url = "=2.5.9"` (patched). The reader does not call `Url::origin()`; it uses `Url::parse` and host/scheme getters for http/https/mailto policy only. |

## Residual Risks

- Fuzzing and ASan runs were executed on Linux `x86_64`; macOS and Windows
  sanitizer runs (e.g. ASan/TSan on Apple Silicon or MSVC ASan) were not run in
  this pass.
- `tools/check_advisories.py` queries `https://api.osv.dev/v1/querybatch`
  (strict in CI; warns on network errors locally unless `--strict` is passed).
- Short-duration (16 s/target) fuzzing does not exhaustively explore deep state
  spaces; longer scheduled fuzz runs remain beneficial before public releases.
