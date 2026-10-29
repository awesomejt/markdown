//! CORE-005B: capability-fed workspace search primitives and budgets.
use markdown_core::search::*;
use markdown_core::session::*;
use markdown_core::workspace::*;
use markdown_core::*;
use std::path::PathBuf;
use std::sync::Arc;

fn repo(path: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(path),
    )
    .unwrap()
}
const CORPUS: &[&str] = &[
    "fixtures/markdown/foundation/gfm-tour.md",
    "fixtures/markdown/foundation/unsafe-html.md",
    "fixtures/markdown/foundation/code-fences.md",
    "fixtures/markdown/advanced/math-alerts.md",
    "fixtures/markdown/advanced/inline-diagrams.md",
    "fixtures/markdown/advanced/voice-parity.md",
    "fixtures/markdown/paths/nfd-filename.md",
    "fixtures/markdown/paths/unicode-filename.md",
    "specs/semantic-model/v1/fixtures/containers.source.txt",
    "specs/semantic-model/v1/fixtures/fallback.source.txt",
    "specs/semantic-model/v1/fixtures/gfm.source.txt",
    "specs/semantic-model/v1/fixtures/resources.source.txt",
    "specs/semantic-model/v1/fixtures/unicode.source.txt",
    "specs/parsing/basic/input.source.txt",
];

fn entry(id: u64, path: &str) -> WorkspaceEntry {
    WorkspaceEntry {
        id,
        path: WorkspacePath::new(path).unwrap(),
    }
}
fn md(id: u64) -> WorkspaceEntry {
    entry(id, &format!("doc{id}.md"))
}
fn query(text: &str) -> ContentQuery {
    ContentQuery::new(text, ContentOptions::default()).unwrap()
}
/// The reader's parse ceiling, which callers may choose over the default.
fn largest_file() -> ContentLimits {
    ContentLimits {
        max_file_bytes: MAX_FILE_BYTES,
        ..ContentLimits::default()
    }
}
fn with_limits(text: &str, limits: ContentLimits) -> ContentQuery {
    ContentQuery::new(
        text,
        ContentOptions {
            limits,
            ..ContentOptions::default()
        },
    )
    .unwrap()
}
fn search(
    q: &ContentQuery,
    progress: &mut ContentProgress,
    e: &WorkspaceEntry,
    source: &str,
) -> DocumentOutcome {
    q.search(progress, e, source.as_bytes(), &CancellationToken::new())
        .unwrap()
}
fn hits(outcome: DocumentOutcome) -> DocumentHits {
    match outcome {
        DocumentOutcome::Searched(hits) => hits,
        other => panic!("not searched: {other:?}"),
    }
}
fn one(source: &str, text: &str) -> DocumentHits {
    hits(search(
        &query(text),
        &mut ContentProgress::default(),
        &md(1),
        source,
    ))
}
fn snippet_texts(found: &DocumentHits) -> Vec<(String, String)> {
    found
        .hits
        .iter()
        .map(|h| {
            let s = &h.snippet;
            let (a, b) = (s.match_start as usize, s.match_end as usize);
            (s.text.clone(), s.text[a..b].to_string())
        })
        .collect()
}
fn parse(source: &str) -> Document {
    parse_markdown(
        source,
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap()
}
/// Case-folded, whitespace-collapsed, format-free text, as find compares it.
fn normalize(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else if c.is_control() {
            out.push('\u{FFFD}');
        } else if !is_format(c) {
            out.push(fold_case(c));
        }
    }
    out.trim_end().to_string()
}

// ---- Paths and the shared entry filter ----

#[test]
fn paths_validate_without_normalizing() {
    for ok in [
        "a.md",
        "docs/a b/ß.md",
        "a\\b.md",
        "x:y.md",
        "Cafe\u{301}.md",
        "emoji/👩‍💻.md",
    ] {
        assert_eq!(WorkspacePath::new(ok).unwrap().as_str(), ok);
    }
    let cases: &[(&str, PathError)] = &[
        ("", PathError::Empty),
        ("/a.md", PathError::Absolute),
        ("a//b.md", PathError::EmptyComponent),
        ("a/", PathError::EmptyComponent),
        ("./a.md", PathError::DotComponent),
        ("a/../b.md", PathError::DotComponent),
        ("..", PathError::DotComponent),
        ("a\u{0}b.md", PathError::Control),
        ("a\nb.md", PathError::Control),
        ("a\u{7f}.md", PathError::Control),
        ("a\u{85}.md", PathError::Control),
        ("evil\u{202E}dm.txt", PathError::Control),
        ("x\u{2066}.md", PathError::Control),
        // PR #10 review: bidi marks reorder display like the embeddings do.
        ("readme\u{200F}.md", PathError::Control),
        ("readme\u{200E}.md", PathError::Control),
        ("x\u{61C}.md", PathError::Control),
    ];
    for (path, error) in cases {
        assert_eq!(WorkspacePath::new(*path), Err(*error), "{path:?}");
    }
    assert_eq!(
        WorkspacePath::new("a".repeat(MAX_PATH_BYTES)).map(|p| p.as_str().len()),
        Ok(MAX_PATH_BYTES)
    );
    assert_eq!(
        WorkspacePath::new("a".repeat(MAX_PATH_BYTES + 1)),
        Err(PathError::TooLong)
    );
    let deep = |n: usize| vec!["d"; n].join("/");
    assert!(WorkspacePath::new(deep(MAX_PATH_DEPTH)).is_ok());
    assert_eq!(
        WorkspacePath::new(deep(MAX_PATH_DEPTH + 1)),
        Err(PathError::TooDeep)
    );
    // Errors never echo the path.
    assert_eq!(PathError::Control.to_string(), "control");
}

#[test]
fn invisible_format_characters_are_flagged_not_rejected() {
    for invisible in [
        "readme\u{200B}.md",
        "a\u{FEFF}.md",
        "a\u{2060}b.md",
        "soft\u{AD}hyphen.md",
        // Blank default-ignorable characters that are not Cf.
        "\u{115F}.md",
        "a\u{1160}.md",
        "dir\u{3164}/a.md",
        "a\u{FFA0}.md",
        "braille\u{2800}blank.md",
        "khmer\u{17B4}.md",
        "a\u{FFF0}.md",
        "a\u{E0080}.md",
    ] {
        let path = WorkspacePath::new(invisible).unwrap();
        assert!(path.has_invisible(), "{invisible:?}");
        assert_eq!(path.as_str(), invisible, "never rewritten");
    }
    // ZWJ emoji, ZWNJ (Persian, Indic) and display modifiers are ordinary.
    for visible in [
        "emoji/👩\u{200D}💻.md",
        "\u{645}\u{6CC}\u{200C}\u{62E}\u{648}\u{627}\u{647}\u{645}.md",
        "plain.md",
        "Cafe\u{301}.md",
        "love\u{2764}\u{FE0F}.md",
        "\u{845B}\u{E0100}.md",
        "\u{1820}\u{180B}.md",
        "\u{5D0}\u{5B8}\u{34F}\u{5B7}.md",
        "\u{1100}\u{1161}.md",
        "\u{2801}.md",
    ] {
        assert!(
            !WorkspacePath::new(visible).unwrap().has_invisible(),
            "{visible:?}"
        );
    }
}

#[test]
fn path_parts_hidden_and_kind() {
    let p = WorkspacePath::new("notes/2026/Plan.MD").unwrap();
    assert_eq!(p.name(), "Plan.MD");
    assert_eq!(p.name_start(), "notes/2026/".len());
    assert!(!p.is_hidden());
    assert_eq!(p.kind(), EntryKind::Markdown);
    let top = WorkspacePath::new("readme.markdown").unwrap();
    assert_eq!((top.name(), top.name_start()), ("readme.markdown", 0));
    assert!(WorkspacePath::new(".git/x.md").unwrap().is_hidden());
    assert!(WorkspacePath::new("a/.hidden.md").unwrap().is_hidden());
    assert!(!WorkspacePath::new("a/b.c/x.md").unwrap().is_hidden());
    for (name, kind) in [
        ("a.md", EntryKind::Markdown),
        ("A.Md", EntryKind::Markdown),
        ("a.markdown", EntryKind::Markdown),
        ("a.MDX", EntryKind::Markdown),
        ("a.b.md", EntryKind::Markdown),
        (".md", EntryKind::Other),
        ("a.md.txt", EntryKind::Other),
        ("a.mdown", EntryKind::Other),
        ("README", EntryKind::Other),
        ("a.", EntryKind::Other),
        ("a.mḋ", EntryKind::Other),
    ] {
        assert_eq!(classify_name(name), kind, "{name}");
    }
}

// ---- File-name search ----

fn name_entries() -> Vec<WorkspaceEntry> {
    [
        "notes/plan.md",
        "planning.md",
        "docs/plan/x.md",
        "a/Explanation.md",
        ".hidden/plan.md",
        "plan.txt",
        "PLAN-b.md",
        "unrelated.md",
    ]
    .iter()
    .enumerate()
    .map(|(i, p)| entry(100 + i as u64, p))
    .collect()
}
fn ranked(entries: &[WorkspaceEntry], results: &NameResults) -> Vec<(NameRank, String, String)> {
    results
        .hits
        .iter()
        .map(|h| {
            let path = entries[h.entry].path.as_str();
            assert_eq!(h.id, entries[h.entry].id);
            (
                h.rank,
                path.to_string(),
                path[h.start as usize..h.end as usize].to_string(),
            )
        })
        .collect()
}

#[test]
fn names_rank_order_ranges_and_filters() {
    let entries = name_entries();
    let results = find_names(&entries, " plan ", &NameSearchOptions::default()).unwrap();
    let s = |r, p: &str, m: &str| (r, p.to_string(), m.to_string());
    assert_eq!(
        ranked(&entries, &results),
        vec![
            s(NameRank::NamePrefix, "PLAN-b.md", "PLAN"),
            s(NameRank::NamePrefix, "notes/plan.md", "plan"),
            s(NameRank::NamePrefix, "planning.md", "plan"),
            s(NameRank::Name, "a/Explanation.md", "plan"),
            s(NameRank::Path, "docs/plan/x.md", "plan"),
        ]
    );
    assert_eq!(results.total_hits, 5);
    assert!(!results.truncated);
    assert_eq!(results.entries_examined, 8);
    assert_eq!(
        (results.skipped_hidden, results.skipped_not_markdown),
        (1, 1)
    );
    assert!(!results.entries_limited);

    let all = NameSearchOptions {
        include_hidden: true,
        markdown_only: false,
        ..NameSearchOptions::default()
    };
    let results = find_names(&entries, "plan", &all).unwrap();
    assert_eq!(results.total_hits, 7);
    assert_eq!(
        (results.skipped_hidden, results.skipped_not_markdown),
        (0, 0)
    );
    // A hidden folder match ranks by the file name.
    assert!(ranked(&entries, &results).contains(&s(
        NameRank::NamePrefix,
        ".hidden/plan.md",
        "plan"
    )));

    let sensitive = NameSearchOptions {
        case_sensitive: true,
        ..NameSearchOptions::default()
    };
    let results = find_names(&entries, "PLAN", &sensitive).unwrap();
    assert_eq!(
        ranked(&entries, &results),
        vec![s(NameRank::NamePrefix, "PLAN-b.md", "PLAN")]
    );

    // Queries may cross folders; the match is then a path match.
    let results = find_names(&entries, "notes/PL", &NameSearchOptions::default()).unwrap();
    assert_eq!(
        ranked(&entries, &results),
        vec![s(NameRank::Path, "notes/plan.md", "notes/pl")]
    );
}

#[test]
fn names_budgets_truncation_and_errors() {
    let entries = name_entries();
    let two = NameSearchOptions {
        max_results: 2,
        ..NameSearchOptions::default()
    };
    let results = find_names(&entries, "plan", &two).unwrap();
    assert_eq!(results.hits.len(), 2);
    assert_eq!(results.total_hits, 5);
    assert!(results.truncated);
    assert_eq!(ranked(&entries, &results)[1].1, "notes/plan.md");

    let first_three = NameSearchOptions {
        max_entries: 3,
        ..NameSearchOptions::default()
    };
    let results = find_names(&entries, "plan", &first_three).unwrap();
    assert!(results.entries_limited);
    assert_eq!(results.entries_examined, 3);
    assert_eq!(results.total_hits, 3);

    let o = NameSearchOptions::default();
    assert_eq!(
        find_names(&entries, " \t", &o),
        Err(WorkspaceError::EmptyQuery)
    );
    assert_eq!(
        find_names(&entries, &"x".repeat(MAX_QUERY_BYTES + 1), &o),
        Err(WorkspaceError::QueryTooLong)
    );
    for bad in [
        NameSearchOptions {
            max_results: 0,
            ..o
        },
        NameSearchOptions {
            max_results: MAX_NAME_RESULTS + 1,
            ..o
        },
        NameSearchOptions {
            max_entries: 0,
            ..o
        },
        NameSearchOptions {
            max_entries: MAX_NAME_ENTRIES + 1,
            ..o
        },
    ] {
        assert_eq!(
            find_names(&entries, "plan", &bad),
            Err(WorkspaceError::InvalidOptions)
        );
    }
    let token = CancellationToken::new();
    token.cancel();
    assert_eq!(
        find_names_cancellable(&entries, "plan", &o, &token),
        Err(WorkspaceError::Cancelled)
    );
    assert_eq!(find_names(&[], "plan", &o).unwrap(), NameResults::default());
}

#[test]
fn names_unicode_folding_and_boundaries() {
    let entries = vec![
        entry(1, "Grüße/Straẞe.md"),
        entry(2, "ΣΟΦΙΑ.md"),
        entry(3, "Cafe\u{301}.md"),
        entry(4, "Café.md"),
    ];
    let o = NameSearchOptions::default();
    let r = find_names(&entries, "STRAßE", &o).unwrap();
    assert_eq!(ranked(&entries, &r)[0].2, "Straẞe");
    let r = find_names(&entries, "grüße/", &o).unwrap();
    assert_eq!(ranked(&entries, &r)[0].2, "Grüße/");
    let r = find_names(&entries, "σοφια", &o).unwrap();
    assert_eq!(ranked(&entries, &r)[0].2, "ΣΟΦΙΑ");
    // No normalization: precomposed and decomposed names differ (as in find).
    let r = find_names(&entries, "café", &o).unwrap();
    assert_eq!(
        ranked(&entries, &r),
        vec![(NameRank::NamePrefix, "Café.md".into(), "Café".into())]
    );
    let r = find_names(&entries, "e\u{301}", &o).unwrap();
    assert_eq!(r.hits.len(), 1);
    assert_eq!(r.hits[0].id, 3);
}

#[test]
fn names_scale_and_determinism() {
    let entries: Vec<WorkspaceEntry> = (0..100_000u64)
        .map(|i| entry(i, &format!("dir{}/sub{}/file-{i}.md", i % 97, i % 13)))
        .collect();
    let o = NameSearchOptions::default();
    let a = find_names(&entries, "file-12", &o).unwrap();
    let b = find_names(&entries, "file-12", &o).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.total_hits, 1 + 10 + 100 + 1000);
    assert!(a.truncated);
    assert_eq!(a.hits.len(), DEFAULT_MAX_NAME_RESULTS);
    let paths: Vec<&str> = a
        .hits
        .iter()
        .map(|h| entries[h.entry].path.as_str())
        .collect();
    assert!(paths.windows(2).all(|w| w[0] <= w[1]));
}

// ---- Content search: parity, snippets, lines, anchors ----

#[test]
fn content_hits_match_document_find_over_corpus() {
    let mut checked = 0;
    for path in CORPUS {
        let source = repo(path);
        let doc = parse(&source);
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        let words: Vec<&str> = source
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 3)
            .take(40)
            .collect();
        for word in words {
            let expected = find(
                &view,
                word,
                &FindOptions {
                    max_matches: DEFAULT_MAX_MATCHES_PER_FILE,
                    ..FindOptions::default()
                },
            )
            .unwrap();
            let found = one(&source, word);
            assert_eq!(found.truncated, expected.truncated, "{path} {word}");
            assert_eq!(found.hits.len(), expected.matches.len());
            assert_eq!(found.source, doc.input().source);
            for (hit, m) in found.hits.iter().zip(&expected.matches) {
                assert_eq!(
                    (hit.unit, hit.source, hit.exact),
                    (m.unit, m.source, m.exact)
                );
                check_snippet(&hit.snippet, word, DEFAULT_SNIPPET_CONTEXT);
                let line = source[..hit.source.start as usize].matches('\n').count() as u64 + 1;
                assert_eq!(hit.line, line);
                let anchor = hit.anchor.as_ref().expect("anchorable");
                assert_eq!(anchor.source_offset, hit.source.start);
                checked += 1;
            }
        }
    }
    assert!(checked > 400, "{checked}");
}

fn check_snippet(s: &Snippet, query: &str, context: usize) {
    let (a, b) = (s.match_start as usize, s.match_end as usize);
    assert!(a <= b && b <= s.text.len());
    assert!(s.text.is_char_boundary(a) && s.text.is_char_boundary(b));
    assert!(!s.text.contains(['\n', '\r', '\t']), "{:?}", s.text);
    assert!(!s.text.contains("  "), "{:?}", s.text);
    assert_eq!(s.text.trim(), s.text, "{:?}", s.text);
    if s.text[a..b].chars().count() < MAX_SNIPPET_MATCH_CHARS {
        assert_eq!(normalize(&s.text[a..b]), normalize(query), "{:?}", s.text);
    }
    assert!(s.text[..a].chars().count() <= context);
    assert!(s.text[b..].chars().count() <= context + 1);
}

#[test]
fn snippets_show_visible_text_around_matches() {
    let found = one("Hello **bold** world and more text here", "BOLD world");
    let s = &found.hits[0].snippet;
    assert_eq!(
        snippet_texts(&found),
        vec![(
            "Hello bold world and more text here".into(),
            "bold world".into()
        )]
    );
    assert!(!s.leading_more && !s.trailing_more);

    let narrow = ContentLimits {
        snippet_context: 3,
        ..ContentLimits::default()
    };
    let q = with_limits("bold", narrow);
    let found = hits(search(
        &q,
        &mut ContentProgress::default(),
        &md(1),
        "Hello **bold** world",
    ));
    assert_eq!(
        snippet_texts(&found),
        vec![("lo bold wo".into(), "bold".into())]
    );
    let s = &found.hits[0].snippet;
    assert!(s.leading_more && s.trailing_more);

    // Breaks, entities and code spans.
    let found = one("Tom &amp; Jerry\nsaw `x  y` here", "& jerry saw");
    assert_eq!(
        snippet_texts(&found)[0],
        ("Tom & Jerry saw x y here".into(), "& Jerry saw".into())
    );
    let found = one("```\nfn main() {\n    let x;\n}\n```\n", "main");
    assert_eq!(snippet_texts(&found)[0].0, "fn main() { let x; }");
    // Image alt text is searchable and shown; destinations are not.
    let found = one("See ![a red fox](fox.png) now", "red fox");
    assert_eq!(
        snippet_texts(&found)[0],
        ("See a red fox now".into(), "red fox".into())
    );
    // Format characters inside the match are dropped from the snippet.
    let found = one("foo\u{AD}bar baz", "foobar");
    assert_eq!(
        snippet_texts(&found)[0],
        ("foobar baz".into(), "foobar".into())
    );
}

#[test]
fn snippets_handle_dense_overlapping_and_clipped_matches() {
    let q = with_limits(
        "a",
        ContentLimits {
            snippet_context: 2,
            ..ContentLimits::default()
        },
    );
    let found = hits(search(
        &q,
        &mut ContentProgress::default(),
        &md(1),
        "a b a b a",
    ));
    assert_eq!(
        snippet_texts(&found),
        vec![
            ("a b".into(), "a".into()),
            ("b a b".into(), "a".into()),
            ("b a".into(), "a".into()),
        ]
    );
    let flags: Vec<(bool, bool)> = found
        .hits
        .iter()
        .map(|h| (h.snippet.leading_more, h.snippet.trailing_more))
        .collect();
    assert_eq!(flags, vec![(false, true), (true, true), (true, false)]);

    let zero = with_limits(
        "b",
        ContentLimits {
            snippet_context: 0,
            ..ContentLimits::default()
        },
    );
    let found = hits(search(
        &zero,
        &mut ContentProgress::default(),
        &md(1),
        "a b c",
    ));
    assert_eq!(snippet_texts(&found), vec![("b".into(), "b".into())]);

    // A long match is clipped and ends the snippet.
    let long = "x".repeat(MAX_SNIPPET_MATCH_CHARS + 50);
    let found = one(&format!("lead {long} tail"), &long);
    let s = &found.hits[0].snippet;
    assert_eq!(
        s.text[s.match_start as usize..].chars().count(),
        MAX_SNIPPET_MATCH_CHARS
    );
    assert_eq!(s.match_end as usize, s.text.len());
    assert!(s.trailing_more);
    assert!(s.text.starts_with("lead x"));

    // Snippets stay bounded inside one very large paragraph, above the
    // default file budget.
    let big = format!(
        "{}needle{}",
        "word ".repeat(400_000),
        " tail".repeat(400_000)
    );
    let q = with_limits("needle", largest_file());
    let found = hits(search(&q, &mut ContentProgress::default(), &md(1), &big));
    let s = &found.hits[0].snippet;
    assert!(s.text.chars().count() <= 2 * DEFAULT_SNIPPET_CONTEXT + "needle".len());
    assert!(s.leading_more && s.trailing_more);
}

#[test]
fn line_numbers_count_lf_endings() {
    let found = one("# T\r\n\r\npara one\r\n\r\nfoo here\n\nfoo again", "foo");
    let lines: Vec<u64> = found.hits.iter().map(|h| h.line).collect();
    assert_eq!(lines, vec![5, 7]);
    let found = one("| a | b |\n|---|---|\n| x | foo |\n", "foo");
    assert_eq!(found.hits[0].line, 3);
}

#[test]
fn anchors_and_source_info_navigate_to_the_opened_file() {
    let source = "# Title\n\nIntro text.\n\n## Part\n\nThe needle is here.\n";
    let found = one(source, "needle");
    let hit = &found.hits[0];
    let anchor = hit.anchor.clone().unwrap();

    let open = |text: &str| {
        let session =
            DocumentSession::new(DocumentId("doc".into()), SessionOptions::default()).unwrap();
        let job = session.submit(text.as_bytes().to_vec(), None).unwrap();
        assert!(matches!(job.run(), Outcome::Published { .. }));
        session.current().unwrap()
    };
    // Unchanged file: source info matches, so the span is used directly.
    let same = open(source);
    assert_eq!(same.source_info(), &found.source);
    let resolved = same.resolve(&anchor).unwrap().unwrap();
    assert_eq!(
        (resolved.quality, resolved.source_offset),
        (AnchorMatch::Exact, hit.source.start)
    );

    // Changed file: source info differs; the anchor still finds the match.
    let edited = format!("New first paragraph.\n\n{source}");
    let changed = open(&edited);
    assert_ne!(changed.source_info(), &found.source);
    let resolved = changed.resolve(&anchor).unwrap().unwrap();
    assert_eq!(resolved.quality, AnchorMatch::Exact);
    let at = resolved.source_offset as usize;
    assert_eq!(&edited[at..at + "needle".len()], "needle");
}

// ---- Content search: filters, skips, budgets, cancellation ----

#[test]
fn admission_filters_and_skips() {
    let q = query("x");
    let mut p = ContentProgress::default();
    assert_eq!(
        q.admit(&mut p, &entry(1, ".git/a.md"), 1),
        Admission::Skip(SkipReason::Hidden)
    );
    assert_eq!(
        q.admit(&mut p, &entry(2, "a.txt"), 1),
        Admission::Skip(SkipReason::NotMarkdown)
    );
    assert_eq!(
        q.admit(&mut p, &md(3), MAX_FILE_BYTES as u64 + 1),
        Admission::Skip(SkipReason::TooLarge)
    );
    assert_eq!(q.admit(&mut p, &md(4), 10), Admission::Read);
    // Read admission records nothing; skips are counted once each.
    assert_eq!(
        p.skipped,
        SkipCounts {
            hidden: 1,
            not_markdown: 1,
            too_large: 1,
            ..SkipCounts::default()
        }
    );
    assert_eq!(p.files_read, 0);

    // `search` re-checks with the real bytes, so a wrong estimate is safe.
    let small = with_limits(
        "x",
        ContentLimits {
            max_file_bytes: 8,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    assert_eq!(small.admit(&mut p, &md(1), 4), Admission::Read);
    assert_eq!(
        search(&small, &mut p, &md(1), "x is longer than eight"),
        DocumentOutcome::Skipped(SkipReason::TooLarge)
    );
    assert_eq!(
        small.search(&mut p, &md(2), b"x \xff\xfe", &CancellationToken::new()),
        Ok(DocumentOutcome::Skipped(SkipReason::InvalidUtf8))
    );
    assert_eq!(p.skipped.too_large, 1);
    assert_eq!(p.skipped.invalid_utf8, 1);
    // The too-large skip happens before the read is charged; the invalid
    // UTF-8 file was admitted and read, so it counts against the budgets.
    assert_eq!((p.files_read, p.bytes_read), (1, 4));

    let hidden = ContentQuery::new(
        "x",
        ContentOptions {
            include_hidden: true,
            ..ContentOptions::default()
        },
    )
    .unwrap();
    let mut p = ContentProgress::default();
    let found = hits(search(&hidden, &mut p, &entry(9, ".notes/a.md"), "x"));
    assert_eq!((found.id, found.hits.len()), (9, 1));
}

#[test]
fn parse_failures_are_skipped_and_counted() {
    let deep = format!("{}x", "> ".repeat(300));
    let q = query("x");
    let mut p = ContentProgress::default();
    assert_eq!(
        search(&q, &mut p, &md(1), &deep),
        DocumentOutcome::Skipped(SkipReason::ParseFailed)
    );
    assert_eq!(p.skipped.parse_failed, 1);
    assert_eq!((p.files_read, p.bytes_read), (1, deep.len() as u64));
    let found = hits(search(&q, &mut p, &md(2), "x"));
    assert_eq!(found.hits.len(), 1);
}

#[test]
fn file_and_byte_budgets_stop_permanently() {
    let q = with_limits(
        "x",
        ContentLimits {
            max_files: 2,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    hits(search(&q, &mut p, &md(1), "x"));
    hits(search(&q, &mut p, &md(2), "none"));
    assert_eq!(
        search(&q, &mut p, &md(3), "x"),
        DocumentOutcome::Stopped(StopReason::Files)
    );
    assert_eq!(p.stopped, Some(StopReason::Files));
    // Sticky, even for entries that would be skipped.
    assert_eq!(
        q.admit(&mut p, &entry(4, "a.txt"), 1),
        Admission::Stop(StopReason::Files)
    );
    assert_eq!(
        (p.files_read, p.files_matched, p.bytes_read, p.matches),
        (2, 1, 5, 1)
    );

    let q = with_limits(
        "x",
        ContentLimits {
            max_total_bytes: 10,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    assert_eq!(q.admit(&mut p, &md(1), 6), Admission::Read);
    hits(search(&q, &mut p, &md(1), "x 3456"));
    assert_eq!(q.admit(&mut p, &md(2), 4), Admission::Read);
    assert_eq!(
        search(&q, &mut p, &md(2), "x 345"),
        DocumentOutcome::Stopped(StopReason::Bytes)
    );
    assert_eq!(p.bytes_read, 6);
    assert_eq!(p.stopped, Some(StopReason::Bytes));
}

#[test]
fn post_read_skips_consume_file_and_byte_budgets() {
    // PR #10 review: invalid UTF-8 and parse-failing files are read (and
    // parsed) before they are skipped, so they must exhaust the budgets.
    let invalid = vec![0xffu8; 512 * 1024];
    let q = with_limits(
        "x",
        ContentLimits {
            max_total_bytes: 1024 * 1024,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    let token = CancellationToken::new();
    let outcomes: Vec<DocumentOutcome> = (0..100)
        .map(|i| q.search(&mut p, &md(i), &invalid, &token).unwrap())
        .collect();
    assert_eq!(
        outcomes[0],
        DocumentOutcome::Skipped(SkipReason::InvalidUtf8)
    );
    assert_eq!(
        outcomes[1],
        DocumentOutcome::Skipped(SkipReason::InvalidUtf8)
    );
    assert!(
        outcomes[2..]
            .iter()
            .all(|o| *o == DocumentOutcome::Stopped(StopReason::Bytes))
    );
    assert_eq!((p.files_read, p.bytes_read), (2, 1024 * 1024));
    assert_eq!(p.skipped.invalid_utf8, 2);

    let deep = format!("{}x", "> ".repeat(300));
    let q = with_limits(
        "x",
        ContentLimits {
            max_files: 10,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    let outcomes: Vec<DocumentOutcome> =
        (0..50).map(|i| search(&q, &mut p, &md(i), &deep)).collect();
    assert!(
        outcomes[..10]
            .iter()
            .all(|o| *o == DocumentOutcome::Skipped(SkipReason::ParseFailed))
    );
    assert!(
        outcomes[10..]
            .iter()
            .all(|o| *o == DocumentOutcome::Stopped(StopReason::Files))
    );
    assert_eq!((p.files_read, p.skipped.parse_failed), (10, 10));
}

#[test]
fn match_budgets_truncate_files_and_stop() {
    let q = with_limits(
        "x",
        ContentLimits {
            max_matches_per_file: 2,
            max_total_matches: 3,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    let first = hits(search(&q, &mut p, &md(1), "x x x x x"));
    assert_eq!((first.hits.len(), first.truncated), (2, true));
    assert_eq!(p.stopped, None);
    let second = hits(search(&q, &mut p, &md(2), "x x"));
    assert_eq!((second.hits.len(), second.truncated), (1, true));
    assert_eq!(p.stopped, Some(StopReason::Matches));
    assert_eq!((p.matches, p.files_truncated, p.files_matched), (3, 2, 2));
    assert_eq!(
        search(&q, &mut p, &md(3), "x"),
        DocumentOutcome::Stopped(StopReason::Matches)
    );

    // Reaching the total exactly also stops the next file.
    let q = with_limits(
        "x",
        ContentLimits {
            max_total_matches: 1,
            ..ContentLimits::default()
        },
    );
    let mut p = ContentProgress::default();
    let only = hits(search(&q, &mut p, &md(1), "x"));
    assert!(!only.truncated);
    assert_eq!(p.stopped, Some(StopReason::Matches));
}

#[test]
fn query_and_limit_validation() {
    let o = ContentOptions::default();
    assert_eq!(
        ContentQuery::new("  \u{AD} ", o).err(),
        Some(WorkspaceError::EmptyQuery)
    );
    assert_eq!(
        ContentQuery::new(&"x".repeat(MAX_QUERY_BYTES + 1), o).err(),
        Some(WorkspaceError::QueryTooLong)
    );
    let l = ContentLimits::default();
    for limits in [
        ContentLimits { max_files: 0, ..l },
        ContentLimits {
            max_files: MAX_FILES + 1,
            ..l
        },
        ContentLimits {
            max_total_bytes: 0,
            ..l
        },
        ContentLimits {
            max_total_bytes: MAX_TOTAL_BYTES + 1,
            ..l
        },
        ContentLimits {
            max_file_bytes: 0,
            ..l
        },
        ContentLimits {
            max_file_bytes: MAX_FILE_BYTES + 1,
            ..l
        },
        ContentLimits {
            max_matches_per_file: 0,
            ..l
        },
        ContentLimits {
            max_matches_per_file: MAX_MATCHES_PER_FILE + 1,
            ..l
        },
        ContentLimits {
            max_total_matches: 0,
            ..l
        },
        ContentLimits {
            max_total_matches: MAX_TOTAL_MATCHES + 1,
            ..l
        },
        ContentLimits {
            snippet_context: MAX_SNIPPET_CONTEXT + 1,
            ..l
        },
    ] {
        let options = ContentOptions { limits, ..o };
        assert_eq!(
            ContentQuery::new("x", options).err(),
            Some(WorkspaceError::InvalidOptions)
        );
    }
    let edge = ContentLimits {
        max_files: MAX_FILES,
        max_total_bytes: MAX_TOTAL_BYTES,
        max_file_bytes: 1,
        max_matches_per_file: MAX_MATCHES_PER_FILE,
        max_total_matches: MAX_TOTAL_MATCHES,
        snippet_context: MAX_SNIPPET_CONTEXT,
    };
    assert!(ContentQuery::new("x", ContentOptions { limits: edge, ..o }).is_ok());
    assert_eq!(WorkspaceError::EmptyQuery.to_string(), "empty_query");
}

#[test]
fn whole_word_and_case_options_pass_through() {
    let q = ContentQuery::new(
        "Cat",
        ContentOptions {
            case_sensitive: true,
            whole_word: true,
            ..ContentOptions::default()
        },
    )
    .unwrap();
    let found = hits(search(
        &q,
        &mut ContentProgress::default(),
        &md(1),
        "Cat cat Category Cat.",
    ));
    assert_eq!(found.hits.len(), 2);
    assert_eq!(snippet_texts(&found)[1].1, "Cat");
}

#[test]
fn cancellation_returns_nothing_and_keeps_progress() {
    let q = query("needle");
    let mut p = ContentProgress::default();
    hits(search(&q, &mut p, &md(1), "needle"));
    let before = p;
    let token = CancellationToken::new();
    token.cancel();
    assert_eq!(
        q.search(&mut p, &md(2), b"needle", &token),
        Err(WorkspaceError::Cancelled)
    );
    assert_eq!(p, before);

    // Concurrent cancellation of a large file: either a full result or
    // `Cancelled` with untouched progress, never a partial result.
    let q = with_limits("needle", largest_file());
    let big: Arc<str> = Arc::from(format!("{}needle", "word ".repeat(1_500_000)));
    for delay in [0u64, 1, 5] {
        let token = CancellationToken::new();
        let canceller = {
            let token = token.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                token.cancel();
            })
        };
        let mut p = ContentProgress::default();
        match q.search(&mut p, &md(3), big.as_bytes(), &token) {
            Err(WorkspaceError::Cancelled) => assert_eq!(p, ContentProgress::default()),
            Ok(DocumentOutcome::Searched(found)) => {
                assert_eq!(found.hits.len(), 1);
                assert_eq!(p.files_read, 1);
            }
            other => panic!("{other:?}"),
        }
        canceller.join().unwrap();
    }
}

#[test]
fn workspace_run_over_corpus_is_deterministic() {
    fn run() -> (Vec<DocumentOutcome>, ContentProgress) {
        let q = query("the");
        let mut p = ContentProgress::default();
        let mut outcomes = Vec::new();
        for (i, path) in CORPUS.iter().enumerate() {
            let display = path.replace(".source.txt", ".md");
            let e = entry(i as u64, &display);
            let source = repo(path);
            if q.admit(&mut p, &e, source.len() as u64) != Admission::Read {
                continue;
            }
            outcomes.push(search(&q, &mut p, &e, &source));
        }
        (outcomes, p)
    }
    let (a, pa) = run();
    let (b, pb) = run();
    assert_eq!(a, b);
    assert_eq!(pa, pb);
    assert_eq!(pa.files_read, CORPUS.len() as u64);
    assert!(pa.matches > 0);
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ContentQuery>();
    assert_send_sync::<ContentProgress>();
}

#[test]
fn generated_inputs_keep_find_parity_and_snippet_invariants() {
    const PARTS: &[&str] = &[
        "a",
        "b",
        "ab ",
        " ",
        "  ",
        "\n",
        "\r\n",
        "\n\n",
        "*",
        "**",
        "`",
        "``",
        "&amp;",
        "&#x41;",
        "\\*",
        "    ",
        "> ",
        "- ",
        "# ",
        "```",
        "~~~",
        "[x](y)",
        "![a b](i.png)",
        "<b>",
        "é",
        "e\u{301}",
        "\u{AD}",
        "\u{345}",
        "👍🏽",
        "🇺🇸",
        "中",
        "|",
        "---",
        "1.5",
        "'",
        "\t",
        "\u{1}",
        "\u{7f}",
    ];
    const QUERIES: &[&str] = &[
        "a", "b", " a", "a b", "ab", "e", "*", "&", "中", "1", "\u{1}", "a\u{1}b",
    ];
    let count: u64 = std::env::var("WORKSPACE_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4096);
    let mut state: u64 = 0x5eed_005b;
    let mut next = |n: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % n
    };
    for _ in 0..count {
        let len = 1 + next(24);
        let source: String = (0..len).map(|_| PARTS[next(PARTS.len())]).collect();
        let text = QUERIES[next(QUERIES.len())];
        let context = next(6);
        let options = ContentOptions {
            case_sensitive: next(2) == 0,
            whole_word: next(2) == 0,
            limits: ContentLimits {
                snippet_context: context,
                ..ContentLimits::default()
            },
            ..ContentOptions::default()
        };
        let q = ContentQuery::new(text, options).unwrap();
        let outcome = search(&q, &mut ContentProgress::default(), &md(1), &source);
        let Ok(doc) = parse_markdown(
            &source,
            DocumentId("doc".into()),
            RevisionId("r1".into()),
            ParseLimits::default(),
        ) else {
            assert_eq!(outcome, DocumentOutcome::Skipped(SkipReason::ParseFailed));
            continue;
        };
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        let find_options = FindOptions {
            case_sensitive: options.case_sensitive,
            whole_word: options.whole_word,
            max_matches: DEFAULT_MAX_MATCHES_PER_FILE,
        };
        let expected = find(&view, text, &find_options).unwrap();
        let found = hits(outcome);
        assert_eq!(
            found.hits.len(),
            expected.matches.len(),
            "{source:?} {text:?}"
        );
        for (hit, m) in found.hits.iter().zip(&expected.matches) {
            assert_eq!((hit.unit, hit.source), (m.unit, m.source));
            check_snippet(&hit.snippet, text, context);
            assert!(hit.anchor.is_some());
        }
    }
}

#[test]
fn control_characters_keep_snippets_aligned() {
    let found = one("a\u{1}b then \u{1}c", "\u{1}");
    assert_eq!(
        snippet_texts(&found),
        vec![
            ("a\u{FFFD}b then \u{FFFD}c".into(), "\u{FFFD}".into()),
            ("a\u{FFFD}b then \u{FFFD}c".into(), "\u{FFFD}".into()),
        ]
    );
    // Find matches a non-whitespace control literally; every later snippet in
    // the block must still be produced.
    let found = one("x\u{1}y then x\u{1}y and more", "x\u{1}y");
    assert_eq!(
        snippet_texts(&found),
        vec![
            (
                "x\u{FFFD}y then x\u{FFFD}y and more".into(),
                "x\u{FFFD}y".into()
            ),
            (
                "x\u{FFFD}y then x\u{FFFD}y and more".into(),
                "x\u{FFFD}y".into()
            ),
        ]
    );
}
