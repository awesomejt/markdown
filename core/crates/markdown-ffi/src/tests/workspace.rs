//! Workspace search exports (CORE-005B2), called as a C host would and
//! checked against `markdown_core::workspace`.
use super::*;
use crate::workspace::{
    md_workspace_classify_path, md_workspace_content_admit, md_workspace_content_search,
    md_workspace_find_names,
};
use markdown_core::workspace::{self as ws, ContentQuery, DocumentOutcome, WorkspaceEntry};

// ---------------------------------------------------------------------------
// Host-side helpers

fn classify(path: &[u8]) -> md_workspace_path_info {
    let mut info = md_workspace_path_info {
        struct_size: size::<md_workspace_path_info>(),
        error: 99,
        ..Default::default()
    };
    // SAFETY: valid pointers for the call.
    let status = unsafe { md_workspace_classify_path(path.as_ptr(), path.len(), &mut info) };
    assert_eq!(status, MD_OK);
    info
}

/// One paths buffer plus entry records, as an adapter would build them.
/// Entry `i` gets ID `1000 + i`.
fn entries(paths: &[&[u8]]) -> (Vec<u8>, Vec<md_workspace_entry>) {
    let mut buffer = Vec::new();
    let mut records = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        records.push(md_workspace_entry {
            id: 1000 + i as u64,
            path_offset: buffer.len() as u64,
            path_length: path.len() as u32,
            reserved: 0,
        });
        buffer.extend_from_slice(path);
    }
    (buffer, records)
}

fn name_opts(flags: u32, max_results: u32, max_entries: u32) -> md_name_options {
    md_name_options {
        struct_size: size::<md_name_options>(),
        flags,
        max_results,
        max_entries,
        entry_size: size::<md_workspace_entry>(),
        hit_size: size::<md_name_hit>(),
    }
}
fn name_result() -> md_name_result {
    md_name_result {
        struct_size: size::<md_name_result>(),
        ..Default::default()
    }
}
fn names_into(
    paths: &[u8],
    records: &[md_workspace_entry],
    query: &[u8],
    options: &md_name_options,
    hits: &mut [md_name_hit],
    out: &mut md_name_result,
) -> md_status {
    // SAFETY: slices are valid for their lengths; empty ones pass null.
    unsafe {
        md_workspace_find_names(
            paths.as_ptr(),
            paths.len(),
            if records.is_empty() {
                null()
            } else {
                records.as_ptr()
            },
            records.len(),
            query.as_ptr(),
            query.len(),
            options,
            array(hits),
            hits.len(),
            out,
        )
    }
}
/// Size with an empty array, then fill.
fn names_all(
    paths: &[u8],
    records: &[md_workspace_entry],
    query: &[u8],
    options: &md_name_options,
) -> Result<(md_name_result, Vec<md_name_hit>), md_status> {
    let mut out = name_result();
    match names_into(paths, records, query, options, &mut [], &mut out) {
        MD_OK => {
            assert_eq!(out.hit_count, 0);
            return Ok((out, Vec::new()));
        }
        MD_ERR_BUFFER_TOO_SMALL => {}
        status => return Err(status),
    }
    let sized = out;
    let mut hits = vec![md_name_hit::default(); out.hit_count as usize];
    let mut out = name_result();
    assert_eq!(
        names_into(paths, records, query, options, &mut hits, &mut out),
        MD_OK
    );
    assert_eq!(out, sized, "a recomputed search is deterministic");
    Ok((out, hits))
}

fn content_opts(flags: u32) -> md_content_options {
    md_content_options {
        struct_size: size::<md_content_options>(),
        flags,
        snippet_context: MD_CONTENT_DEFAULT_SNIPPET_CONTEXT,
        hit_size: size::<md_content_hit>(),
        ..Default::default()
    }
}
fn progress() -> md_content_progress {
    md_content_progress {
        struct_size: size::<md_content_progress>(),
        ..Default::default()
    }
}
fn content_result() -> md_content_result {
    md_content_result {
        struct_size: size::<md_content_result>(),
        ..Default::default()
    }
}
fn admit(
    query: &[u8],
    options: &md_content_options,
    path: &[u8],
    byte_length: u64,
    progress: &mut md_content_progress,
) -> Result<(u32, u32), md_status> {
    let (mut outcome, mut reason) = (99, 99);
    // SAFETY: valid pointers for the call.
    let status = unsafe {
        md_workspace_content_admit(
            query.as_ptr(),
            query.len(),
            options,
            path.as_ptr(),
            path.len(),
            byte_length,
            progress,
            &mut outcome,
            &mut reason,
        )
    };
    match status {
        MD_OK => Ok((outcome, reason)),
        status => {
            assert_eq!((outcome, reason), (0, 0), "zeroed on failure");
            Err(status)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn search_into(
    query: &[u8],
    options: &md_content_options,
    path: &[u8],
    content: &[u8],
    progress: &mut md_content_progress,
    hits: &mut [md_content_hit],
    text: &mut [u8],
    anchors: &mut [u8],
    out: &mut md_content_result,
) -> md_status {
    // SAFETY: slices are valid for their lengths; empty ones pass null.
    unsafe {
        md_workspace_content_search(
            query.as_ptr(),
            query.len(),
            options,
            path.as_ptr(),
            path.len(),
            content.as_ptr(),
            content.len(),
            progress,
            array(hits),
            hits.len(),
            array(text),
            text.len(),
            array(anchors),
            anchors.len(),
            out,
        )
    }
}

/// One file's FFI results, copied out of the caller arrays.
struct Searched {
    result: md_content_result,
    hits: Vec<md_content_hit>,
    text: Vec<u8>,
    anchors: Vec<u8>,
}
/// The two-call protocol: a sizing call must leave progress unchanged.
fn search_all(
    query: &[u8],
    options: &md_content_options,
    path: &[u8],
    content: &[u8],
    accounting: &mut md_content_progress,
) -> Result<Searched, md_status> {
    let before = *accounting;
    let mut out = content_result();
    let status = search_into(
        query,
        options,
        path,
        content,
        accounting,
        &mut [],
        &mut [],
        &mut [],
        &mut out,
    );
    match status {
        MD_OK => {
            assert_eq!(out.hit_count, 0);
            return Ok(Searched {
                result: out,
                hits: Vec::new(),
                text: Vec::new(),
                anchors: Vec::new(),
            });
        }
        MD_ERR_BUFFER_TOO_SMALL => assert_eq!(*accounting, before, "sizing changes nothing"),
        status => {
            assert_eq!(*accounting, before, "failure changes nothing");
            return Err(status);
        }
    }
    let sized = out;
    let mut hits = vec![md_content_hit::default(); out.hit_count as usize];
    let mut text = vec![0u8; out.text_length as usize];
    let mut anchors = vec![0u8; out.anchor_length as usize];
    let mut out = content_result();
    let status = search_into(
        query,
        options,
        path,
        content,
        accounting,
        &mut hits,
        &mut text,
        &mut anchors,
        &mut out,
    );
    assert_eq!(status, MD_OK);
    assert_eq!(out, sized, "a recomputed search is deterministic");
    Ok(Searched {
        result: out,
        hits,
        text,
        anchors,
    })
}

fn rust_progress(p: &ws::ContentProgress) -> md_content_progress {
    md_content_progress {
        struct_size: size::<md_content_progress>(),
        stopped: match p.stopped {
            None => 0,
            Some(ws::StopReason::Files) => MD_STOP_FILES,
            Some(ws::StopReason::Bytes) => MD_STOP_BYTES,
            Some(ws::StopReason::Matches) => MD_STOP_MATCHES,
        },
        files_read: p.files_read,
        files_matched: p.files_matched,
        bytes_read: p.bytes_read,
        matches: p.matches,
        files_truncated: p.files_truncated,
        skipped_hidden: p.skipped.hidden,
        skipped_not_markdown: p.skipped.not_markdown,
        skipped_too_large: p.skipped.too_large,
        skipped_invalid_utf8: p.skipped.invalid_utf8,
        skipped_parse_failed: p.skipped.parse_failed,
    }
}
fn skip_code(reason: ws::SkipReason) -> u32 {
    match reason {
        ws::SkipReason::Hidden => MD_SKIP_HIDDEN,
        ws::SkipReason::NotMarkdown => MD_SKIP_NOT_MARKDOWN,
        ws::SkipReason::TooLarge => MD_SKIP_TOO_LARGE,
        ws::SkipReason::InvalidUtf8 => MD_SKIP_INVALID_UTF8,
        ws::SkipReason::ParseFailed => MD_SKIP_PARSE_FAILED,
    }
}
fn stop_code(reason: ws::StopReason) -> u32 {
    rust_progress(&ws::ContentProgress {
        stopped: Some(reason),
        ..Default::default()
    })
    .stopped
}
fn core_options(options: &md_content_options) -> ws::ContentOptions {
    let or = |value: u64, default: u64| if value == 0 { default } else { value };
    ws::ContentOptions {
        case_sensitive: options.flags & MD_CONTENT_CASE_SENSITIVE != 0,
        whole_word: options.flags & MD_CONTENT_WHOLE_WORD != 0,
        include_hidden: options.flags & MD_CONTENT_INCLUDE_HIDDEN != 0,
        limits: ws::ContentLimits {
            max_files: or(options.max_files, ws::DEFAULT_MAX_FILES),
            max_total_bytes: or(options.max_total_bytes, ws::DEFAULT_MAX_TOTAL_BYTES),
            max_file_bytes: or(options.max_file_bytes.into(), ws::MAX_FILE_BYTES as u64) as usize,
            max_matches_per_file: or(
                options.max_matches_per_file.into(),
                ws::DEFAULT_MAX_MATCHES_PER_FILE as u64,
            ) as usize,
            max_total_matches: or(
                options.max_total_matches.into(),
                ws::DEFAULT_MAX_TOTAL_MATCHES,
            ),
            snippet_context: options.snippet_context as usize,
        },
    }
}

/// Search one file through the FFI and the Rust API side by side, checking
/// the outcome, every hit field and the resulting progress.
fn assert_content_parity(
    query: &str,
    options: &md_content_options,
    path: &str,
    content: &[u8],
    accounting: &mut md_content_progress,
    rust: &mut ws::ContentProgress,
) -> Searched {
    let core = ContentQuery::new(query, core_options(options)).unwrap();
    let entry = WorkspaceEntry {
        id: 0,
        path: ws::WorkspacePath::new(path).unwrap(),
    };
    let expected = core
        .search(rust, &entry, content, &CancellationToken::new())
        .unwrap();
    let got = search_all(
        query.as_bytes(),
        options,
        path.as_bytes(),
        content,
        accounting,
    )
    .unwrap();
    assert_eq!(*accounting, rust_progress(rust), "{path} {query:?}");
    let r = &got.result;
    match expected {
        DocumentOutcome::Skipped(reason) => {
            assert_eq!(
                (r.outcome, r.reason),
                (MD_CONTENT_SKIPPED, skip_code(reason))
            );
            assert_eq!((r.hit_count, r.source_length), (0, 0));
        }
        DocumentOutcome::Stopped(reason) => {
            assert_eq!(
                (r.outcome, r.reason),
                (MD_CONTENT_STOPPED, stop_code(reason))
            );
            assert_eq!((r.hit_count, r.source_length), (0, 0));
        }
        DocumentOutcome::Searched(document) => {
            assert_eq!((r.outcome, r.reason), (MD_CONTENT_SEARCHED, 0));
            assert_eq!(r.flags & MD_CONTENT_TRUNCATED != 0, document.truncated);
            assert_eq!(r.source_length, document.source.bytes);
            assert_eq!(&r.source_sha256[..], document.source.sha256.as_bytes());
            assert_eq!(got.hits.len(), document.hits.len(), "{path} {query:?}");
            let (mut text_end, mut anchor_end) = (0usize, 0usize);
            for (h, e) in got.hits.iter().zip(&document.hits) {
                let flags = if e.exact { MD_HIT_EXACT } else { 0 }
                    | if e.snippet.leading_more {
                        MD_HIT_LEADING_MORE
                    } else {
                        0
                    }
                    | if e.snippet.trailing_more {
                        MD_HIT_TRAILING_MORE
                    } else {
                        0
                    }
                    | if e.anchor.is_some() { MD_HIT_ANCHOR } else { 0 };
                assert_eq!(
                    (
                        h.unit as usize,
                        h.flags,
                        u64::from(h.span_start),
                        u64::from(h.span_end),
                        h.line,
                        u64::from(h.match_start),
                        u64::from(h.match_end),
                    ),
                    (
                        e.unit,
                        flags,
                        e.source.start,
                        e.source.end,
                        e.line,
                        e.snippet.match_start,
                        e.snippet.match_end,
                    )
                );
                assert_eq!(h.text_offset as usize, text_end, "snippets are packed");
                text_end += h.text_length as usize;
                let snippet = &got.text[h.text_offset as usize..text_end];
                assert_eq!(snippet, e.snippet.text.as_bytes());
                assert!(snippet.len() <= MD_CONTENT_MAX_SNIPPET_BYTES as usize);
                assert_eq!(h.anchor_offset as usize, anchor_end, "anchors are packed");
                anchor_end += h.anchor_length as usize;
                let anchor = &got.anchors[h.anchor_offset as usize..anchor_end];
                let expected = e.anchor.as_ref().map(|a| anchor_bytes(a).unwrap());
                assert_eq!(anchor, expected.as_deref().unwrap_or_default());
            }
            assert_eq!((text_end, anchor_end), (got.text.len(), got.anchors.len()));
        }
    }
    got
}

/// Up to `limit` distinct words of three or more letters, in source order.
fn words(source: &str, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    source
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && seen.insert(w.to_lowercase()))
        .take(limit)
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// Path classification

#[test]
fn classify_path_matches_the_rust_rules() {
    let long = "a/".repeat(2047) + "bb";
    let deep = "a/".repeat(256) + "b.md";
    let cases: Vec<Vec<u8>> = [
        "notes.md",
        "docs/Guide.MARKDOWN",
        "a/b/c.mdx",
        "README",
        ".md",
        "a.md.txt",
        ".hidden/a.md",
        "dir/.a.md",
        "zw\u{200B}sp.md",
        "\u{FEFF}bom.md",
        "family\u{200D}emoji.md",
        "back\\slash:colon.md",
        "",
        "/abs.md",
        "a//b.md",
        "a/",
        "./a.md",
        "a/../b.md",
        "tab\t.md",
        "del\u{7F}.md",
        "c1\u{85}.md",
        "rlo\u{202E}dm.txt",
        "lrm\u{200E}.md",
        "alm\u{061C}.md",
        "iso\u{2066}.md",
        &long,
        &deep,
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .chain([
        vec![b'a'; 4097],
        b"bad\xFF.md".to_vec(),
        b"half\xE2\x82.md".to_vec(),
    ])
    .collect();
    for path in &cases {
        let info = classify(path);
        let expected = match std::str::from_utf8(path) {
            Err(_) => Err(MD_PATH_INVALID_UTF8),
            Ok(text) => ws::WorkspacePath::new(text).map_err(|e| match e {
                ws::PathError::Empty => MD_PATH_EMPTY,
                ws::PathError::TooLong => MD_PATH_TOO_LONG,
                ws::PathError::TooDeep => MD_PATH_TOO_DEEP,
                ws::PathError::Absolute => MD_PATH_ABSOLUTE,
                ws::PathError::EmptyComponent => MD_PATH_EMPTY_COMPONENT,
                ws::PathError::DotComponent => MD_PATH_DOT_COMPONENT,
                ws::PathError::Control => MD_PATH_CONTROL,
            }),
        };
        let expected = match expected {
            Ok(p) => md_workspace_path_info {
                struct_size: size::<md_workspace_path_info>(),
                error: MD_PATH_VALID,
                flags: if p.kind() == ws::EntryKind::Markdown {
                    MD_PATH_MARKDOWN
                } else {
                    0
                } | if p.is_hidden() { MD_PATH_HIDDEN } else { 0 }
                    | if p.has_invisible() {
                        MD_PATH_INVISIBLE
                    } else {
                        0
                    },
                name_start: p.name_start() as u32,
            },
            // Over-long input is rejected before decoding.
            Err(_) if path.len() > MD_WORKSPACE_MAX_PATH_BYTES as usize => md_workspace_path_info {
                struct_size: size::<md_workspace_path_info>(),
                error: MD_PATH_TOO_LONG,
                ..Default::default()
            },
            Err(error) => md_workspace_path_info {
                struct_size: size::<md_workspace_path_info>(),
                error,
                ..Default::default()
            },
        };
        assert_eq!(info, expected, "{:?}", String::from_utf8_lossy(path));
    }
    assert_eq!(classify(b"docs/a.md").name_start, 5);
    assert_eq!(
        classify(b"zw\xE2\x80\x8Bsp.md").flags,
        MD_PATH_MARKDOWN | MD_PATH_INVISIBLE
    );
    assert_eq!(classify(&[]).error, MD_PATH_EMPTY);

    let mut info = md_workspace_path_info {
        struct_size: size::<md_workspace_path_info>(),
        error: 7,
        ..Default::default()
    };
    // SAFETY: null with a nonzero length is rejected before any read.
    let status = unsafe { md_workspace_classify_path(null(), 3, &mut info) };
    assert_eq!(status, MD_ERR_INVALID_ARGUMENT);
    assert_eq!(
        (info.error, info.struct_size),
        (0, size::<md_workspace_path_info>())
    );
    info.struct_size += 4;
    info.error = 7;
    // SAFETY: valid pointers.
    let status = unsafe { md_workspace_classify_path(b"a.md".as_ptr(), 4, &mut info) };
    assert_eq!(status, MD_ERR_STRUCT_SIZE);
    assert_eq!(info.error, 7, "a rejected struct is not written");
    // SAFETY: null output is rejected.
    let status = unsafe { md_workspace_classify_path(b"a.md".as_ptr(), 4, null_mut()) };
    assert_eq!(status, MD_ERR_INVALID_ARGUMENT);
}

// ---------------------------------------------------------------------------
// File-name search

/// A deterministic workspace with Markdown, other, hidden and invalid names.
fn name_corpus() -> Vec<Vec<u8>> {
    let mut paths: Vec<Vec<u8>> = Vec::new();
    let folders = ["", "docs/", "notes/2026/", ".git/", "Guides/", "src/notes/"];
    let names = [
        "notes.md",
        "Notes Index.markdown",
        "readme.MD",
        "guide.mdx",
        "draft notes.txt",
        "image.png",
        ".hidden.md",
        "Straße.md",
        "STRASSE.md",
        "Ωmega.md",
        "zw\u{200B}notes.md",
        "a.md",
    ];
    for folder in folders {
        for name in names {
            paths.push(format!("{folder}{name}").into_bytes());
        }
    }
    // Paths the core rejects: skipped and counted, never shown.
    paths.push(b"/abs/notes.md".to_vec());
    paths.push(b"bad\xFFnotes.md".to_vec());
    paths.push(b"a//notes.md".to_vec());
    paths.push(b"../notes.md".to_vec());
    paths.push(Vec::new());
    paths.push(b"rlo\xE2\x80\xAEnotes.md".to_vec());
    // Duplicates keep entry order.
    paths.push(b"docs/notes.md".to_vec());
    paths
}

fn assert_names_parity(paths: &[Vec<u8>], query: &str, options: &md_name_options) {
    let refs: Vec<&[u8]> = paths.iter().map(Vec::as_slice).collect();
    let (buffer, records) = entries(&refs);
    let (out, hits) = names_all(&buffer, &records, query.as_bytes(), options).unwrap();

    let core = ws::NameSearchOptions {
        case_sensitive: options.flags & MD_NAMES_CASE_SENSITIVE != 0,
        include_hidden: options.flags & MD_NAMES_INCLUDE_HIDDEN != 0,
        markdown_only: options.flags & MD_NAMES_ALL_FILES == 0,
        max_results: if options.max_results == 0 {
            ws::DEFAULT_MAX_NAME_RESULTS
        } else {
            options.max_results as usize
        },
        max_entries: if options.max_entries == 0 {
            ws::DEFAULT_MAX_NAME_ENTRIES
        } else {
            options.max_entries as usize
        },
    };
    let examined = paths.len().min(core.max_entries);
    let mut valid = Vec::new();
    let mut index = Vec::new();
    for (i, path) in paths[..examined].iter().enumerate() {
        if let Some(path) = std::str::from_utf8(path)
            .ok()
            .and_then(|p| ws::WorkspacePath::new(p).ok())
        {
            valid.push(WorkspaceEntry {
                id: 1000 + i as u64,
                path,
            });
            index.push(i);
        }
    }
    let expected = ws::find_names(&valid, query, &core).unwrap();
    let invalid = (examined - valid.len()) as u64;
    assert_eq!(
        out,
        md_name_result {
            struct_size: size::<md_name_result>(),
            flags: if expected.truncated {
                MD_NAMES_TRUNCATED
            } else {
                0
            } | if paths.len() > examined {
                MD_NAMES_ENTRIES_LIMITED
            } else {
                0
            },
            hit_count: expected.hits.len() as u64,
            total_hits: expected.total_hits,
            entries_examined: examined as u64,
            skipped_hidden: expected.skipped_hidden,
            skipped_not_markdown: expected.skipped_not_markdown,
            skipped_invalid: invalid,
        },
        "{query:?} {options:?}"
    );
    for (h, e) in hits.iter().zip(&expected.hits) {
        let rank = match e.rank {
            ws::NameRank::NamePrefix => MD_NAME_RANK_PREFIX,
            ws::NameRank::Name => MD_NAME_RANK_NAME,
            ws::NameRank::Path => MD_NAME_RANK_PATH,
        };
        assert_eq!(
            *h,
            md_name_hit {
                id: e.id,
                entry: index[e.entry] as u32,
                rank,
                start: e.start as u32,
                end: e.end as u32,
            }
        );
        // The highlight range is valid UTF-8 inside the caller's path.
        let path = &paths[h.entry as usize];
        assert_eq!(h.id, 1000 + u64::from(h.entry));
        assert!(std::str::from_utf8(&path[h.start as usize..h.end as usize]).is_ok());
    }
}

#[test]
fn name_search_matches_the_rust_api() {
    let paths = name_corpus();
    let queries = [
        "notes",
        "NOTES",
        "n",
        "md",
        ".md",
        "docs/",
        "s/n",
        "straße",
        "STRASSE",
        "ẞ",
        "ωMEGA",
        "  notes ",
        "x-none",
        "zw\u{200B}",
        "notes.md",
        "/",
    ];
    let flag_sets = [
        0,
        MD_NAMES_CASE_SENSITIVE,
        MD_NAMES_INCLUDE_HIDDEN,
        MD_NAMES_ALL_FILES,
        MD_NAMES_INCLUDE_HIDDEN | MD_NAMES_ALL_FILES | MD_NAMES_CASE_SENSITIVE,
    ];
    for query in queries {
        for flags in flag_sets {
            for (max_results, max_entries) in [(0, 0), (1, 0), (5, 0), (0, 30), (3, 75)] {
                assert_names_parity(&paths, query, &name_opts(flags, max_results, max_entries));
            }
        }
    }
    // Ties on rank sort by path, then entry index.
    let (buffer, records) = entries(&[b"b/x.md", b"a/x.md", b"b/x.md", b"x.md"]);
    let (_, hits) = names_all(&buffer, &records, b"x", &name_opts(0, 0, 0)).unwrap();
    let order: Vec<u32> = hits.iter().map(|h| h.entry).collect();
    assert_eq!(order, [1, 0, 2, 3]);
    assert!(hits.iter().all(|h| h.rank == MD_NAME_RANK_PREFIX));

    // An empty or whitespace-only query succeeds with nothing examined.
    for query in [&b""[..], b"  \t"] {
        let (out, hits) = names_all(&buffer, &records, query, &name_opts(0, 0, 0)).unwrap();
        assert_eq!((out, hits.len()), (name_result(), 0));
    }
    // No entries at all.
    let (out, _) = names_all(&[], &[], b"x", &name_opts(0, 0, 0)).unwrap();
    assert_eq!(out.entries_examined, 0);
}

#[test]
fn name_search_scales_to_the_default_entry_budget() {
    let paths: Vec<Vec<u8>> = (0..100_001)
        .map(|i| format!("folder{}/note-{i:06}.md", i % 97).into_bytes())
        .collect();
    let refs: Vec<&[u8]> = paths.iter().map(Vec::as_slice).collect();
    let (buffer, records) = entries(&refs);
    let (out, hits) = names_all(&buffer, &records, b"NOTE-0999", &name_opts(0, 0, 0)).unwrap();
    assert_eq!(out.entries_examined, 100_000);
    assert_eq!(out.flags, MD_NAMES_ENTRIES_LIMITED);
    assert_eq!((out.total_hits, hits.len()), (100, 100));
    assert!(
        hits.windows(2)
            .all(|w| paths[w[0].entry as usize] < paths[w[1].entry as usize])
    );
}

#[test]
fn name_search_validates_arguments_and_zeroes_outputs() {
    let (buffer, records) = entries(&[b"a.md", b"b.md", b"ab.md"]);
    let dirty = || md_name_result {
        struct_size: size::<md_name_result>(),
        flags: 7,
        hit_count: 7,
        total_hits: 7,
        entries_examined: 7,
        skipped_hidden: 7,
        skipped_not_markdown: 7,
        skipped_invalid: 7,
    };
    let sentinel = md_name_hit {
        id: 0xAAAA,
        ..Default::default()
    };
    let check = |paths: &[u8],
                 records: &[md_workspace_entry],
                 query: &[u8],
                 options: &md_name_options,
                 expected: md_status| {
        let mut out = dirty();
        let mut hits = [sentinel; 4];
        let status = names_into(paths, records, query, options, &mut hits, &mut out);
        assert_eq!(status, expected, "{options:?}");
        assert_eq!(out, name_result(), "zeroed on failure");
        assert!(hits.iter().all(|h| *h == sentinel), "no record written");
    };
    let base = name_opts(0, 0, 0);
    for (options, expected) in [
        (
            md_name_options {
                struct_size: 0,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_name_options {
                entry_size: 16,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_name_options {
                hit_size: 32,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_name_options { flags: 8, ..base },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_name_options {
                max_results: MD_NAMES_MAX_RESULTS + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_name_options {
                max_entries: MD_NAMES_MAX_ENTRIES + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
    ] {
        check(&buffer, &records, b"a", &options, expected);
    }
    check(
        &buffer,
        &records,
        &vec![b'a'; MD_WORKSPACE_MAX_QUERY_BYTES as usize + 1],
        &base,
        MD_ERR_INVALID_ARGUMENT,
    );
    check(&buffer, &records, b"\xFF", &base, MD_ERR_INVALID_ARGUMENT);
    // Out-of-range references and a nonzero reserved field are caller bugs.
    let mut bad = records.clone();
    bad[1].path_offset = buffer.len() as u64;
    check(&buffer, &bad, b"a", &base, MD_ERR_INVALID_ARGUMENT);
    let mut bad = records.clone();
    bad[2].path_offset = u64::MAX;
    check(&buffer, &bad, b"a", &base, MD_ERR_INVALID_ARGUMENT);
    let mut bad = records.clone();
    bad[0].reserved = 1;
    check(&buffer, &bad, b"a", &base, MD_ERR_INVALID_ARGUMENT);
    // ...but only among examined entries.
    let mut tail = records.clone();
    tail[2].path_offset = u64::MAX;
    let (out, _) = names_all(&buffer, &tail, b"a", &name_opts(0, 0, 2)).unwrap();
    assert_eq!(
        (out.entries_examined, out.flags),
        (2, MD_NAMES_ENTRIES_LIMITED)
    );

    let mut out = dirty();
    // SAFETY: null entries with a nonzero count are rejected before any read.
    let status = unsafe {
        md_workspace_find_names(
            buffer.as_ptr(),
            buffer.len(),
            null(),
            1,
            b"a".as_ptr(),
            1,
            &base,
            null_mut(),
            0,
            &mut out,
        )
    };
    assert_eq!((status, out), (MD_ERR_INVALID_ARGUMENT, name_result()));
    let mut hits = [md_name_hit::default(); 3];
    let mut out = dirty();
    // SAFETY: a misaligned hit array is rejected before any write.
    let status = unsafe {
        md_workspace_find_names(
            buffer.as_ptr(),
            buffer.len(),
            records.as_ptr(),
            records.len(),
            b"a".as_ptr(),
            1,
            &base,
            hits.as_mut_ptr().cast::<u8>().add(1).cast(),
            2,
            &mut out,
        )
    };
    assert_eq!((status, out), (MD_ERR_INVALID_ARGUMENT, name_result()));
    // SAFETY: null hits with a nonzero capacity are rejected.
    let status = unsafe {
        md_workspace_find_names(
            buffer.as_ptr(),
            buffer.len(),
            records.as_ptr(),
            records.len(),
            b"a".as_ptr(),
            1,
            &base,
            null_mut(),
            2,
            &mut out,
        )
    };
    assert_eq!(status, MD_ERR_INVALID_ARGUMENT);

    // Aliased ranges cannot multiply work: the examined path total is
    // capped before any copy (PR #11 review).
    let path = vec![b'p'; MD_WORKSPACE_MAX_PATH_BYTES as usize];
    let at_cap = (MD_NAMES_MAX_PATH_BYTES / u64::from(MD_WORKSPACE_MAX_PATH_BYTES)) as usize;
    let aliased = vec![
        md_workspace_entry {
            id: 1,
            path_offset: 0,
            path_length: MD_WORKSPACE_MAX_PATH_BYTES,
            reserved: 0,
        };
        at_cap + 1
    ];
    check(&path, &aliased, b"q", &base, MD_ERR_LIMIT);
    let (out, _) = names_all(
        &path,
        &aliased,
        b"q",
        &name_opts(MD_NAMES_ALL_FILES, 0, at_cap as u32),
    )
    .unwrap();
    assert_eq!((out.entries_examined, out.total_hits), (at_cap as u64, 0));

    // Too small: counts are reported, records are not written.
    let mut out = name_result();
    let mut hits = [sentinel; 1];
    assert_eq!(
        names_into(&buffer, &records, b"a", &base, &mut hits, &mut out),
        MD_ERR_BUFFER_TOO_SMALL
    );
    assert_eq!(
        (out.hit_count, out.total_hits, out.entries_examined),
        (2, 2, 3)
    );
    assert_eq!(hits[0], sentinel);
    // A capacity larger than needed leaves the tail untouched.
    let mut hits = [sentinel; 4];
    assert_eq!(
        names_into(&buffer, &records, b"a", &base, &mut hits, &mut out),
        MD_OK
    );
    assert_eq!((hits[0].id, hits[1].id), (1000, 1002));
    assert_eq!(&hits[2..], &[sentinel; 2]);
}

// ---------------------------------------------------------------------------
// Content search

#[test]
fn content_search_matches_the_rust_api_over_the_corpus() {
    let _s = serial();
    let sources = corpus();
    for flags in [
        0,
        MD_CONTENT_CASE_SENSITIVE,
        MD_CONTENT_WHOLE_WORD,
        MD_CONTENT_CASE_SENSITIVE | MD_CONTENT_WHOLE_WORD,
    ] {
        for (context, per_file) in [(MD_CONTENT_DEFAULT_SNIPPET_CONTEXT, 0), (0, 3), (200, 1)] {
            let options = md_content_options {
                snippet_context: context,
                max_matches_per_file: per_file,
                max_total_matches: MD_CONTENT_MAX_TOTAL_MATCHES,
                ..content_opts(flags)
            };
            for (i, source) in sources.iter().enumerate() {
                let path = format!("corpus/doc{i}.md");
                let mut queries = words(source, 20);
                queries.extend(["a", "the", "ß", "\u{1F600}", "日本"].map(String::from));
                let (mut accounting, mut rust) = (progress(), ws::ContentProgress::default());
                for query in &queries {
                    assert_content_parity(
                        query,
                        &options,
                        &path,
                        source.as_bytes(),
                        &mut accounting,
                        &mut rust,
                    );
                }
            }
        }
    }
}

/// Hits navigate into a session opened on the same bytes: equal source info
/// means spans (and, for a v1 view, units) apply directly, and every anchor
/// resolves exactly.
#[test]
fn content_hits_navigate_into_an_unchanged_session_snapshot() {
    let _s = serial();
    let mut checked = 0;
    for model in models() {
        for (i, source) in corpus().iter().enumerate() {
            let session = create(model);
            let snapshot = publish(session, source);
            let v = view(snapshot);
            let t = Tables { v: &v };
            let nodes = t.nodes();
            for query in words(source, 10) {
                let path = format!("doc{i}.md");
                let got = search_all(
                    query.as_bytes(),
                    &content_opts(0),
                    path.as_bytes(),
                    source.as_bytes(),
                    &mut progress(),
                )
                .unwrap();
                assert_eq!(got.result.outcome, MD_CONTENT_SEARCHED);
                assert_eq!(got.result.source_length, v.source_length);
                assert_eq!(
                    t.str(v.source_sha256).as_bytes(),
                    &got.result.source_sha256[..]
                );
                for h in &got.hits {
                    // Units index a v1 view only (v2 adds nodes); spans
                    // and anchors work for both models.
                    if model == MD_MODEL_V1 {
                        let node = &nodes[h.unit as usize];
                        assert!(node.span_start <= h.span_start && h.span_end <= node.span_end);
                    }
                    let anchor = &got.anchors
                        [h.anchor_offset as usize..(h.anchor_offset + h.anchor_length) as usize];
                    let resolved = resolve(snapshot, anchor).unwrap();
                    assert_eq!(resolved.quality, MD_ANCHOR_EXACT);
                    assert_eq!(resolved.source_offset, u64::from(h.span_start));
                    checked += 1;
                }
            }
            assert_eq!(md_snapshot_release(snapshot), MD_OK);
            assert_eq!(md_session_release(session), MD_OK);
        }
    }
    assert!(checked >= 100 * models().len(), "{checked}");
}

#[test]
fn content_admission_skips_stops_and_budgets_match_the_rust_api() {
    let deep = ">".repeat(300) + " x\n";
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("a.md", b"alpha beta alpha\n".to_vec()),
        (".hidden/a.md", b"alpha\n".to_vec()),
        ("notes.txt", b"alpha\n".to_vec()),
        ("bad.md", b"alpha \xFF\n".to_vec()),
        ("deep.md", deep.into_bytes()),
        ("big.md", vec![b'a'; 2048]),
        ("b.md", b"alpha\nalpha\nalpha\nalpha\n".to_vec()),
        ("c.md", b"no match here\n".to_vec()),
        ("d.md", b"alpha alpha\n".to_vec()),
        ("e.md", b"alpha\n".to_vec()),
        ("f.md", b"alpha\n".to_vec()),
    ];
    let limits = [
        content_opts(0),
        md_content_options {
            max_file_bytes: 1024,
            ..content_opts(MD_CONTENT_INCLUDE_HIDDEN)
        },
        md_content_options {
            max_files: 3,
            ..content_opts(0)
        },
        md_content_options {
            max_total_bytes: 60,
            ..content_opts(0)
        },
        md_content_options {
            max_matches_per_file: 2,
            max_total_matches: 5,
            ..content_opts(0)
        },
        md_content_options {
            max_total_matches: 1,
            ..content_opts(0)
        },
    ];
    for options in &limits {
        let core = ContentQuery::new("alpha", core_options(options)).unwrap();
        // Admit first, as a host avoiding reads would, on one pair...
        let (mut accounting, mut rust) = (progress(), ws::ContentProgress::default());
        // ...and search without admitting on the other.
        let (mut direct, mut rust_direct) = (progress(), ws::ContentProgress::default());
        for (path, content) in &files {
            let entry = WorkspaceEntry {
                id: 0,
                path: ws::WorkspacePath::new(*path).unwrap(),
            };
            let expected = core.admit(&mut rust, &entry, content.len() as u64);
            let got = admit(
                b"alpha",
                options,
                path.as_bytes(),
                content.len() as u64,
                &mut accounting,
            )
            .unwrap();
            let want = match expected {
                ws::Admission::Read => (MD_CONTENT_READ, 0),
                ws::Admission::Skip(r) => (MD_CONTENT_SKIPPED, skip_code(r)),
                ws::Admission::Stop(r) => (MD_CONTENT_STOPPED, stop_code(r)),
            };
            assert_eq!(got, want, "{path} {options:?}");
            assert_eq!(accounting, rust_progress(&rust));
            if got.0 == MD_CONTENT_READ {
                assert_content_parity("alpha", options, path, content, &mut accounting, &mut rust);
            }
            assert_content_parity(
                "alpha",
                options,
                path,
                content,
                &mut direct,
                &mut rust_direct,
            );
        }
    }
    // Each budget ends in a sticky stop with the expected reason.
    let run = |options: &md_content_options| {
        let mut accounting = progress();
        for (path, content) in &files {
            let _ = search_all(b"alpha", options, path.as_bytes(), content, &mut accounting);
        }
        let after = accounting;
        let again = search_all(b"alpha", options, b"z.md", b"alpha", &mut accounting).unwrap();
        if after.stopped != 0 {
            assert_eq!(accounting, after, "a stop is permanent");
        }
        (after, again.result)
    };
    let (p, r) = run(&limits[2]);
    assert_eq!((p.stopped, p.files_read), (MD_STOP_FILES, 3));
    assert_eq!((r.outcome, r.reason), (MD_CONTENT_STOPPED, MD_STOP_FILES));
    let (p, _) = run(&limits[3]);
    assert_eq!(p.stopped, MD_STOP_BYTES);
    assert!(p.bytes_read <= 60);
    let (p, _) = run(&limits[4]);
    assert_eq!((p.stopped, p.matches), (MD_STOP_MATCHES, 5));
    assert!(p.files_truncated >= 1);
    let (p, _) = run(&limits[1]);
    assert_eq!(
        (
            p.stopped,
            p.skipped_too_large,
            p.skipped_hidden,
            p.skipped_not_markdown
        ),
        (0, 1, 0, 1)
    );
    assert_eq!((p.skipped_invalid_utf8, p.skipped_parse_failed), (1, 1));
    // A wrong size estimate is safe: search re-checks with the real length.
    let mut accounting = progress();
    let options = md_content_options {
        max_file_bytes: 1024,
        ..content_opts(0)
    };
    assert_eq!(
        admit(b"alpha", &options, b"big.md", 10, &mut accounting).unwrap(),
        (MD_CONTENT_READ, 0)
    );
    let got = search_all(
        b"alpha",
        &options,
        b"big.md",
        &[b'a'; 2048],
        &mut accounting,
    )
    .unwrap();
    assert_eq!(
        (got.result.outcome, got.result.reason),
        (MD_CONTENT_SKIPPED, MD_SKIP_TOO_LARGE)
    );
    assert_eq!(
        (accounting.files_read, accounting.skipped_too_large),
        (0, 1)
    );
}

#[test]
fn content_search_buffer_sizing_from_limits_and_too_small_arrays() {
    let content = "alpha one\n\nalpha two *alpha* three\n\n- alpha\n".repeat(10);
    let options = md_content_options {
        max_matches_per_file: 25,
        ..content_opts(0)
    };
    let query = b"alpha";
    let path = b"doc.md";
    let reference = search_all(query, &options, path, content.as_bytes(), &mut progress()).unwrap();
    assert_eq!(reference.result.hit_count, 25);
    assert_eq!(reference.result.flags, MD_CONTENT_TRUNCATED);

    // Arrays sized from the limits succeed in one call.
    let n = options.max_matches_per_file as usize;
    let mut hits = vec![md_content_hit::default(); n];
    let mut text = vec![0u8; n * MD_CONTENT_MAX_SNIPPET_BYTES as usize];
    let mut anchors = vec![0u8; n * MD_ANCHOR_MAX_BYTES as usize];
    let mut accounting = progress();
    let mut out = content_result();
    assert_eq!(
        search_into(
            query,
            &options,
            path,
            content.as_bytes(),
            &mut accounting,
            &mut hits,
            &mut text,
            &mut anchors,
            &mut out
        ),
        MD_OK
    );
    assert_eq!(out, reference.result);
    assert_eq!(&hits[..], &reference.hits[..]);
    assert_eq!(&text[..reference.text.len()], &reference.text[..]);
    assert_eq!(&anchors[..reference.anchors.len()], &reference.anchors[..]);
    assert_eq!((accounting.files_read, accounting.matches), (1, 25));

    // Any one array too small: counts reported, nothing else written and
    // progress unchanged.
    let r = &reference.result;
    let sizes = [
        (r.hit_count - 1, r.text_length, r.anchor_length),
        (r.hit_count, r.text_length - 1, r.anchor_length),
        (r.hit_count, r.text_length, r.anchor_length - 1),
        (0, 0, 0),
    ];
    for (h, t, a) in sizes {
        let hit_sentinel = md_content_hit {
            unit: 0xBBBB,
            ..Default::default()
        };
        let mut hits = vec![hit_sentinel; h as usize];
        let mut text = vec![0xEEu8; t as usize];
        let mut anchors = vec![0xEEu8; a as usize];
        let mut accounting = progress();
        accounting.files_read = 4;
        let before = accounting;
        let mut out = content_result();
        assert_eq!(
            search_into(
                query,
                &options,
                path,
                content.as_bytes(),
                &mut accounting,
                &mut hits,
                &mut text,
                &mut anchors,
                &mut out
            ),
            MD_ERR_BUFFER_TOO_SMALL
        );
        assert_eq!(
            out, reference.result,
            "the result struct is written in full"
        );
        assert_eq!(accounting, before);
        assert!(hits.iter().all(|x| *x == hit_sentinel));
        assert!(text.iter().chain(&anchors).all(|&b| b == 0xEE));
    }
}

#[test]
fn content_search_validates_arguments_and_leaves_progress_unchanged() {
    let accounting = md_content_progress {
        files_read: 2,
        bytes_read: 20,
        skipped_hidden: 1,
        ..progress()
    };
    let check = |query: &[u8],
                 options: &md_content_options,
                 path: &[u8],
                 progress: md_content_progress,
                 expected: md_status| {
        let mut p = progress;
        assert_eq!(
            admit(query, options, path, 5, &mut p),
            Err(expected),
            "admit {options:?}"
        );
        assert_eq!(p, progress);
        let mut out = md_content_result {
            outcome: 9,
            hit_count: 9,
            ..content_result()
        };
        let mut hits = [md_content_hit {
            unit: 0xBBBB,
            ..Default::default()
        }; 2];
        let mut text = [0xEEu8; 64];
        let mut anchors = [0xEEu8; 64];
        let status = search_into(
            query,
            options,
            path,
            b"alpha\n",
            &mut p,
            &mut hits,
            &mut text,
            &mut anchors,
            &mut out,
        );
        assert_eq!(status, expected, "search {options:?}");
        assert_eq!(p, progress, "progress unchanged");
        assert_eq!(out, content_result(), "zeroed on failure");
        assert!(hits.iter().all(|h| h.unit == 0xBBBB));
        assert!(text.iter().chain(&anchors).all(|&b| b == 0xEE));
    };
    let base = content_opts(0);
    for (options, expected) in [
        (
            md_content_options {
                struct_size: 8,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_content_options {
                hit_size: 40,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_content_options { flags: 8, ..base },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                reserved: 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                max_files: MD_CONTENT_MAX_FILES + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                max_total_bytes: MD_CONTENT_MAX_TOTAL_BYTES + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                max_file_bytes: MD_CONTENT_MAX_FILE_BYTES + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                max_matches_per_file: MD_CONTENT_MAX_MATCHES_PER_FILE + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                max_total_matches: MD_CONTENT_MAX_TOTAL_MATCHES + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_content_options {
                snippet_context: MD_CONTENT_MAX_SNIPPET_CONTEXT + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
    ] {
        check(b"alpha", &options, b"a.md", accounting, expected);
    }
    for query in [&b""[..], b" \n ", b"\xFF"] {
        check(query, &base, b"a.md", accounting, MD_ERR_INVALID_ARGUMENT);
    }
    let long = vec![b'a'; MD_WORKSPACE_MAX_QUERY_BYTES as usize + 1];
    check(&long, &base, b"a.md", accounting, MD_ERR_INVALID_ARGUMENT);
    for path in [&b""[..], b"/a.md", b"a/../b.md", b"bad\xFF.md", b"tab\t.md"] {
        check(b"alpha", &base, path, accounting, MD_ERR_INVALID_ARGUMENT);
    }
    let forged = md_content_progress {
        stopped: 4,
        ..accounting
    };
    check(b"alpha", &base, b"a.md", forged, MD_ERR_INVALID_ARGUMENT);
    let wrong = md_content_progress {
        struct_size: 80,
        ..accounting
    };
    check(b"alpha", &base, b"a.md", wrong, MD_ERR_STRUCT_SIZE);

    // Null and misaligned pointers.
    let mut p = accounting;
    let (mut outcome, mut reason) = (9, 9);
    // SAFETY: a null progress is rejected before any read.
    let status = unsafe {
        md_workspace_content_admit(
            b"a".as_ptr(),
            1,
            &base,
            b"a.md".as_ptr(),
            4,
            1,
            null_mut(),
            &mut outcome,
            &mut reason,
        )
    };
    assert_eq!((status, outcome, reason), (MD_ERR_INVALID_ARGUMENT, 0, 0));
    // SAFETY: a null output is rejected before any other work.
    let status = unsafe {
        md_workspace_content_admit(
            b"a".as_ptr(),
            1,
            &base,
            b"a.md".as_ptr(),
            4,
            1,
            &mut p,
            null_mut(),
            &mut reason,
        )
    };
    assert_eq!((status, p), (MD_ERR_INVALID_ARGUMENT, accounting));
    let mut out = content_result();
    let mut text = [0u8; 8];
    // SAFETY: null content with a nonzero length is rejected.
    let status = unsafe {
        md_workspace_content_search(
            b"a".as_ptr(),
            1,
            &base,
            b"a.md".as_ptr(),
            4,
            null(),
            5,
            &mut p,
            null_mut(),
            0,
            text.as_mut_ptr(),
            text.len(),
            null_mut(),
            0,
            &mut out,
        )
    };
    assert_eq!((status, p), (MD_ERR_INVALID_ARGUMENT, accounting));
    // SAFETY: null text with a nonzero capacity is rejected.
    let status = unsafe {
        md_workspace_content_search(
            b"a".as_ptr(),
            1,
            &base,
            b"a.md".as_ptr(),
            4,
            b"a".as_ptr(),
            1,
            &mut p,
            null_mut(),
            0,
            null_mut(),
            8,
            null_mut(),
            0,
            &mut out,
        )
    };
    assert_eq!((status, p), (MD_ERR_INVALID_ARGUMENT, accounting));
    let mut hits = [md_content_hit::default(); 2];
    // SAFETY: a misaligned hit array is rejected before any write.
    let status = unsafe {
        md_workspace_content_search(
            b"a".as_ptr(),
            1,
            &base,
            b"a.md".as_ptr(),
            4,
            b"a".as_ptr(),
            1,
            &mut p,
            hits.as_mut_ptr().cast::<u8>().add(4).cast(),
            1,
            null_mut(),
            0,
            null_mut(),
            0,
            &mut out,
        )
    };
    assert_eq!((status, p), (MD_ERR_INVALID_ARGUMENT, accounting));

    // Forged counters at the maximum saturate instead of overflowing.
    let mut p = md_content_progress {
        files_matched: u64::MAX,
        files_truncated: u64::MAX,
        skipped_hidden: u64::MAX,
        skipped_invalid_utf8: u64::MAX,
        ..progress()
    };
    let tight = md_content_options {
        max_matches_per_file: 1,
        ..base
    };
    search_all(b"alpha", &tight, b"a.md", b"alpha alpha\n", &mut p).unwrap();
    search_all(b"alpha", &tight, b".h/a.md", b"alpha\n", &mut p).unwrap();
    search_all(b"alpha", &tight, b"b.md", b"\xFF", &mut p).unwrap();
    assert_eq!(
        (
            p.files_matched,
            p.files_truncated,
            p.skipped_hidden,
            p.skipped_invalid_utf8
        ),
        (u64::MAX, u64::MAX, u64::MAX, u64::MAX)
    );
    assert_eq!(p.files_read, 2);
}

#[test]
fn workspace_exports_contain_panics_and_leave_outputs_untouched() {
    let mut info = md_workspace_path_info {
        struct_size: size::<md_workspace_path_info>(),
        flags: 9,
        ..Default::default()
    };
    inject_panic();
    // SAFETY: valid pointers.
    let status = unsafe { md_workspace_classify_path(b"a.md".as_ptr(), 4, &mut info) };
    assert_eq!((status, info.flags), (MD_ERR_PANIC, 0));

    let (buffer, records) = entries(&[b"a.md"]);
    let sentinel = md_name_hit {
        id: 0xAAAA,
        ..Default::default()
    };
    let mut hits = [sentinel; 2];
    let mut out = name_result();
    out.hit_count = 9;
    inject_panic();
    assert_eq!(
        names_into(
            &buffer,
            &records,
            b"a",
            &name_opts(0, 0, 0),
            &mut hits,
            &mut out
        ),
        MD_ERR_PANIC
    );
    assert_eq!(out, name_result());
    assert!(hits.iter().all(|h| *h == sentinel));

    let before = md_content_progress {
        files_read: 1,
        ..progress()
    };
    let mut p = before;
    inject_panic();
    assert_eq!(
        admit(b"alpha", &content_opts(0), b"a.md", 5, &mut p),
        Err(MD_ERR_PANIC)
    );
    assert_eq!(p, before);
    let mut hits = [md_content_hit {
        unit: 0xBBBB,
        ..Default::default()
    }; 2];
    let mut text = [0xEEu8; 256];
    let mut anchors = [0xEEu8; 2048];
    let mut out = content_result();
    out.hit_count = 9;
    inject_panic();
    assert_eq!(
        search_into(
            b"alpha",
            &content_opts(0),
            b"a.md",
            b"alpha\n",
            &mut p,
            &mut hits,
            &mut text,
            &mut anchors,
            &mut out
        ),
        MD_ERR_PANIC
    );
    assert_eq!((out, p), (content_result(), before));
    assert!(hits.iter().all(|h| h.unit == 0xBBBB));
    assert!(text.iter().chain(&anchors).all(|&b| b == 0xEE));

    // Still usable afterwards, and from several threads at once, each with
    // its own progress.
    let sources = corpus();
    let reference: Vec<Vec<md_content_hit>> = sources
        .iter()
        .map(|s| {
            search_all(
                b"the",
                &content_opts(0),
                b"a.md",
                s.as_bytes(),
                &mut progress(),
            )
            .unwrap()
            .hits
        })
        .collect();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let mut p = progress();
                for (source, expected) in sources.iter().zip(&reference) {
                    let got =
                        search_all(b"the", &content_opts(0), b"a.md", source.as_bytes(), &mut p)
                            .unwrap();
                    assert_eq!(&got.hits, expected);
                }
                assert_eq!(p.files_read, sources.len() as u64);
            });
        }
    });
}

#[test]
fn abi_constants_equal_the_core_constants() {
    assert_eq!(MD_WORKSPACE_MAX_PATH_BYTES as usize, ws::MAX_PATH_BYTES);
    assert_eq!(MD_WORKSPACE_MAX_PATH_DEPTH as usize, ws::MAX_PATH_DEPTH);
    assert_eq!(
        MD_WORKSPACE_MAX_QUERY_BYTES as usize,
        markdown_core::search::MAX_QUERY_BYTES
    );
    assert_eq!(
        MD_NAMES_DEFAULT_MAX_RESULTS as usize,
        ws::DEFAULT_MAX_NAME_RESULTS
    );
    assert_eq!(MD_NAMES_MAX_RESULTS as usize, ws::MAX_NAME_RESULTS);
    assert_eq!(
        MD_NAMES_DEFAULT_MAX_ENTRIES as usize,
        ws::DEFAULT_MAX_NAME_ENTRIES
    );
    assert_eq!(MD_NAMES_MAX_ENTRIES as usize, ws::MAX_NAME_ENTRIES);
    assert_eq!(MD_CONTENT_DEFAULT_MAX_FILES, ws::DEFAULT_MAX_FILES);
    assert_eq!(MD_CONTENT_MAX_FILES, ws::MAX_FILES);
    assert_eq!(
        MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES,
        ws::DEFAULT_MAX_TOTAL_BYTES
    );
    assert_eq!(MD_CONTENT_MAX_TOTAL_BYTES, ws::MAX_TOTAL_BYTES);
    assert_eq!(MD_CONTENT_MAX_FILE_BYTES as usize, ws::MAX_FILE_BYTES);
    assert_eq!(
        MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE as usize,
        ws::DEFAULT_MAX_MATCHES_PER_FILE
    );
    assert_eq!(
        MD_CONTENT_MAX_MATCHES_PER_FILE as usize,
        ws::MAX_MATCHES_PER_FILE
    );
    assert_eq!(
        u64::from(MD_CONTENT_DEFAULT_MAX_TOTAL_MATCHES),
        ws::DEFAULT_MAX_TOTAL_MATCHES
    );
    assert_eq!(
        u64::from(MD_CONTENT_MAX_TOTAL_MATCHES),
        ws::MAX_TOTAL_MATCHES
    );
    assert_eq!(
        MD_CONTENT_DEFAULT_SNIPPET_CONTEXT as usize,
        ws::DEFAULT_SNIPPET_CONTEXT
    );
    assert_eq!(
        MD_CONTENT_MAX_SNIPPET_CONTEXT as usize,
        ws::MAX_SNIPPET_CONTEXT
    );
    assert_eq!(
        MD_CONTENT_MAX_SNIPPET_BYTES as usize,
        (2 * ws::MAX_SNIPPET_CONTEXT + ws::MAX_SNIPPET_MATCH_CHARS) * 4
    );
}
