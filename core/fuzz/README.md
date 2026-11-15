# Core Fuzzing and Sanitizer Targets

`core/fuzz` defines coverage-guided libFuzzer targets over `markdown-core` and
`markdown-ffi` (`model-v2` enabled) without adding external fuzz crates to the
production `Cargo.lock`:

1. `fuzz_parser` (`md_fuzz_parser`): parses arbitrary UTF-8 input through
   `parse_markdown`, `parse_markdown_v2`, and cancellable variants, validates
   snapshots, and asserts JSON round-trips.
2. `fuzz_model_decoder` (`md_fuzz_model_decoder`): exercises `Document::from_json`
   and `v2::Document::from_json` on arbitrary bytes and `(source, json)` pairs.
3. `fuzz_ffi_session` (`md_fuzz_ffi_session_find_anchor`): exercises
   `md_session_create`, `md_session_submit`, `md_job_run`, `md_snapshot_get_view`,
   `md_snapshot_find`, `md_snapshot_read_stats`, `md_snapshot_anchor_at`,
   `md_snapshot_anchor_for_node`, and `md_snapshot_resolve_anchor` with valid
   backing buffers and asserts zero leaked handles.
4. `fuzz_ffi_workspace` (`md_fuzz_ffi_workspace`): exercises
   `md_workspace_classify_path`, `md_workspace_find_names`,
   `md_workspace_content_admit`, and `md_workspace_content_search` with valid
   backing buffers.

## Running

```sh
python3 tools/fuzz.py --seconds 30
```

Outputs, corpus directories, and compiled binaries live under `build/fuzz/`
(gitignored).
