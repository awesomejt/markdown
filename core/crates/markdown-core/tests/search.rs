//! CORE-005A: source-accurate find, word count and section timing.
use markdown_core::search::*;
use markdown_core::session::*;
use markdown_core::*;
use std::path::PathBuf;
use std::sync::Arc;

fn parse(source: &str) -> Document {
    parse_markdown(
        source,
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap()
}
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

fn find_in(source: &str, query: &str, options: FindOptions) -> FindResults {
    let doc = parse(source);
    let view = SearchDocument::from_v1(&doc, source).unwrap();
    let results = find(&view, query, &options).unwrap();
    check_invariants(&doc, source, query, &options, &results);
    results
}
fn ci(source: &str, query: &str) -> FindResults {
    find_in(source, query, FindOptions::default())
}
fn slices<'a>(source: &'a str, results: &FindResults) -> Vec<&'a str> {
    results
        .matches
        .iter()
        .map(|m| m.source.slice(source).unwrap())
        .collect()
}
fn normalize(text: &str, case_sensitive: bool) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else if !is_format(c) {
            out.push(if case_sensitive { c } else { fold_case(c) });
        }
    }
    out
}
fn display_text<'a>(node: &'a Node, source: &'a str) -> &'a str {
    match &node.content {
        NodeKind::Text { text }
        | NodeKind::CodeSpan { text }
        | NodeKind::CodeBlock { text, .. } => text,
        NodeKind::Image { alt, .. } => alt,
        NodeKind::UnsupportedBlock { .. } | NodeKind::UnsupportedInline { .. } => {
            node.span.slice(source).unwrap()
        }
        _ => "",
    }
}

/// Range, boundary and round-trip properties every result must satisfy.
fn check_invariants(
    doc: &Document,
    source: &str,
    query: &str,
    options: &FindOptions,
    results: &FindResults,
) {
    let nodes = &doc.input().nodes;
    assert!(results.matches.len() <= options.max_matches);
    let mut last_start = 0;
    for m in &results.matches {
        let unit = &nodes[m.unit];
        assert!(
            m.source.slice(source).is_some(),
            "match span on UTF-8 boundaries"
        );
        assert!(unit.span.start <= m.source.start && m.source.end <= unit.span.end);
        assert!(
            m.source.start >= last_start,
            "reading order is source order here"
        );
        last_start = m.source.start;
        assert!(!m.segments.is_empty());
        let mut joined = String::new();
        for s in &m.segments {
            let node = &nodes[s.node];
            let text = display_text(node, source);
            let piece = text
                .get(s.text_start as usize..s.text_end as usize)
                .expect("segment text range on UTF-8 boundaries");
            assert!(s.source.slice(source).is_some());
            assert!(m.source.start <= s.source.start && s.source.end <= m.source.end);
            assert!(node.span.start <= s.source.start && s.source.end <= node.span.end);
            if matches!(
                node.content,
                NodeKind::SoftBreak {} | NodeKind::HardBreak {}
            ) {
                joined.push(' ');
            } else {
                if s.exact {
                    assert_eq!(
                        s.source.slice(source),
                        Some(piece),
                        "exact segment copies source"
                    );
                }
                joined.push_str(piece);
            }
        }
        // Exact matches consist only of exact segments; any derived
        // (atomic) character, whitespace included, clears `exact`.
        assert_eq!(m.exact, m.segments.iter().all(|s| s.exact));
        assert_eq!(
            normalize(&joined, options.case_sensitive).trim(),
            normalize(query, options.case_sensitive).trim(),
            "segments round-trip to the normalized query"
        );
    }
}

#[test]
fn literal_case_insensitive_by_default() {
    let source = "Hello world. HELLO again, hello!\n";
    let r = ci(source, "hello");
    assert_eq!(slices(source, &r), ["Hello", "HELLO", "hello"]);
    assert!(r.matches.iter().all(|m| m.exact) && !r.truncated);
    let sensitive = FindOptions {
        case_sensitive: true,
        ..FindOptions::default()
    };
    assert_eq!(
        slices(source, &find_in(source, "HELLO", sensitive)),
        ["HELLO"]
    );
    // Literal, not regex.
    assert!(ci(source, "h.llo").matches.is_empty());
    assert_eq!(ci("a.b a+b\n", "a+b").matches.len(), 1);
}

#[test]
fn matches_cross_inline_boundaries_with_exact_spans() {
    let source = "foo **bar** and *b*[a](x)z\n";
    let r = ci(source, "foo bar");
    assert_eq!(slices(source, &r), ["foo **bar"]);
    let m = &r.matches[0];
    assert!(m.exact);
    assert_eq!(m.segments.len(), 2);
    let r = ci(source, "baz");
    assert_eq!(slices(source, &r), ["b*[a](x)z"]);
    assert_eq!(r.matches[0].segments.len(), 3);
}

#[test]
fn whitespace_breaks_and_crlf_are_normalized() {
    for source in [
        "foo\nbar\n",
        "foo\r\nbar\r\n",
        "foo  \nbar\n",
        "foo\\\nbar\n",
        "foo   bar\n",
    ] {
        let r = ci(source, "foo bar");
        assert_eq!(r.matches.len(), 1, "{source:?}");
        assert_eq!(
            r.matches[0].source.slice(source).unwrap().trim_end(),
            source.trim_end()
        );
        assert_eq!(ci(source, "foo \t\n bar").matches.len(), 1);
    }
    // Matches never cross block (unit) boundaries.
    assert!(ci("foo\n\nbar\n", "foo bar").matches.is_empty());
    assert!(
        ci("| foo | bar |\n|---|---|\n| a | b |\n", "foo bar")
            .matches
            .is_empty()
    );
}

#[test]
fn derived_text_maps_atomically() {
    let source = "a &amp; b, \\* star\n";
    let r = ci(source, "a & b");
    assert_eq!(slices(source, &r), ["a &amp; b"]);
    assert!(!r.matches[0].exact);
    let amp = &r.matches[0].segments[1];
    assert_eq!(amp.source.slice(source), Some("&amp;"));
    assert_eq!((amp.text_start, amp.text_end), (0, 1));
    // Escapes are excluded from text spans by the parser, so stay exact.
    let r = ci(source, "* star");
    assert_eq!(slices(source, &r), ["* star"]);
    assert!(r.matches[0].exact);
}

#[test]
fn code_spans_and_blocks_map_to_source_lines() {
    let source =
        "Use `` a`b `` and `x`.\n\n```rust\nlet x = 1;\r\nlet y = 2;\n```\n\n    indented y\n";
    let r = ci(source, "a`b");
    assert_eq!(slices(source, &r), ["a`b"]);
    assert!(r.matches[0].exact);
    let r = ci(source, "y");
    assert_eq!(slices(source, &r), ["y", "y"]);
    assert!(r.matches.iter().all(|m| m.exact));
    let r = ci(source, "1; let");
    assert_eq!(slices(source, &r), ["1;\r\nlet"]);
    // Code blocks inside block quotes keep container prefixes out of spans.
    let quoted = "> ```\n> alpha\n> beta\n> ```\n";
    let r = ci(quoted, "alpha beta");
    assert_eq!(slices(quoted, &r), ["alpha\n> beta"]);
    assert!(r.matches[0].exact);
}

#[test]
fn images_unsupported_and_footnotes() {
    let source = "See ![a cat](cat.png) and <span>raw</span> text[^1].\n\n[^1]: Footnote cat.\n\n```mermaid\ngraph cat\n```\n";
    let r = ci(source, "cat");
    assert_eq!(slices(source, &r), ["cat", "cat", "cat"]);
    let doc = parse(source);
    let kinds: Vec<_> = r
        .matches
        .iter()
        .map(|m| match doc.input().nodes[m.segments[0].node].content {
            NodeKind::Image { .. } => "image",
            NodeKind::Text { .. } => "text",
            NodeKind::UnsupportedBlock { .. } => "unsupported",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["image", "text", "unsupported"]);
    assert_eq!(ci(source, "<span>").matches.len(), 1);
    // The basic parser does not enable footnotes yet (CORE-003B3), so
    // `[^1]` and its definition stay literal text: `text[^1].` contains
    // neither `text1` nor `text.`. Footnote nodes are covered by the v1
    // goldens in `model_goldens_cover_every_node_kind`.
    assert!(ci(source, "text1").matches.is_empty());
    assert!(ci(source, "text.").matches.is_empty());
}

#[test]
fn front_matter_is_not_document_text() {
    let source = "---\ntitle: Secret title\n---\n\nBody title.\n";
    assert_eq!(slices(source, &ci(source, "title")), ["title"]);
}

#[test]
fn case_folding_rules() {
    assert_eq!(fold_case('ẞ'), 'ß');
    assert_eq!(fold_case('ß'), 'ß');
    assert_eq!(fold_case('Σ'), 'σ');
    assert_eq!(fold_case('ς'), 'σ');
    assert_eq!(fold_case('\u{212A}'), 'k'); // Kelvin sign
    assert_eq!(fold_case('\u{B5}'), 'μ'); // micro sign
    assert_eq!(fold_case('ϐ'), fold_case('Β'));
    assert_eq!(fold_case('I'), 'i');
    assert_eq!(fold_case('ı'), 'ı'); // no Turkish tailoring
    assert_eq!(fold_case('İ'), 'İ'); // lowercase expands; simple folding has none
    assert_eq!(fold_case('Ꭰ'), fold_case('ꭰ')); // Cherokee pair
    let source = "STRASSE straße STRAẞE ΟΔΟΣ οδος οδοσ İstanbul istanbul ıi\n";
    assert_eq!(slices(source, &ci(source, "straße")), ["straße", "STRAẞE"]);
    assert_eq!(
        ci(source, "strasse").matches.len(),
        1,
        "ß is not expanded to ss"
    );
    assert_eq!(ci(source, "οδος").matches.len(), 3);
    assert_eq!(slices(source, &ci(source, "istanbul")), ["istanbul"]);
    assert_eq!(slices(source, &ci(source, "İSTANBUL")), ["İstanbul"]);
    let dotted = ci(source, "I");
    assert_eq!(slices(source, &dotted), ["i", "i"]);
    assert!(
        dotted
            .matches
            .iter()
            .all(|m| m.source.slice(source) != Some("ı"))
    );
}

#[test]
fn unicode_clusters_and_normalization() {
    let source = repo("specs/semantic-model/v1/fixtures/unicode.source.txt");
    // The fixture has NFC "Café" in the heading and NFD "cafe\u{301}" in body.
    let r = ci(&source, "café");
    assert_eq!(
        r.matches.len(),
        1,
        "no normalization: NFC does not match NFD"
    );
    let r = ci(&source, "cafe\u{301}");
    assert_eq!(slices(&source, &r), ["cafe\u{301}"]);
    // A prefix that would split a combining sequence is not a match.
    assert!(ci(&source, "cafe").matches.is_empty());
    assert!(ci(&source, "\u{301}").matches.is_empty());

    let emoji = "Family 👨‍👩‍👧 here; flags 🇺🇸🇬🇧; thumbs 👍🏽 ok.\n";
    assert!(
        ci(emoji, "👨").matches.is_empty(),
        "ZWJ sequence is not split"
    );
    assert_eq!(ci(emoji, "👨‍👩‍👧").matches.len(), 1);
    assert!(
        ci(emoji, "🇸🇬").matches.is_empty(),
        "regional-indicator pairs are not split"
    );
    assert_eq!(slices(emoji, &ci(emoji, "🇬🇧")), ["🇬🇧"]);
    assert!(
        ci(emoji, "👍").matches.is_empty(),
        "skin-tone modifier extends"
    );

    let rtl = "שלום עולם ومرحبا بالعالم\n";
    assert_eq!(slices(rtl, &ci(rtl, "עולם")), ["עולם"]);
    assert_eq!(slices(rtl, &ci(rtl, "بالعالم")), ["بالعالم"]);
    let cjk = "日本語のテキストと中文。\n";
    assert_eq!(slices(cjk, &ci(cjk, "テキスト")), ["テキスト"]);
}

#[test]
fn whole_word_option() {
    let options = FindOptions {
        whole_word: true,
        ..FindOptions::default()
    };
    let source = "cat concat cat_s cats Cat. (cat) café cafés\n";
    assert_eq!(
        slices(source, &find_in(source, "cat", options)),
        ["cat", "Cat", "cat"]
    );
    assert_eq!(slices(source, &find_in(source, "cat.", options)), ["Cat."]);
    assert_eq!(slices(source, &find_in(source, "café", options)), ["café"]);
    // Ideographs are words of their own.
    let cjk = "我中文好\n";
    assert_eq!(find_in(cjk, "中文", options).matches.len(), 1);
}

#[test]
fn empty_long_and_invalid_queries() {
    let source = "some text\n";
    let doc = parse(source);
    let view = SearchDocument::from_v1(&doc, source).unwrap();
    let options = FindOptions::default();
    assert_eq!(find(&view, "", &options).unwrap(), FindResults::default());
    assert_eq!(
        find(&view, " \n\t", &options).unwrap(),
        FindResults::default()
    );
    assert!(
        find(&view, &"x".repeat(MAX_QUERY_BYTES), &options)
            .unwrap()
            .matches
            .is_empty()
    );
    assert_eq!(
        find(&view, &"x".repeat(MAX_QUERY_BYTES + 1), &options),
        Err(SearchError::QueryTooLong)
    );
    for max_matches in [0, MAX_MATCHES + 1] {
        let bad = FindOptions {
            max_matches,
            ..options
        };
        assert_eq!(find(&view, "x", &bad), Err(SearchError::InvalidOptions));
    }
    assert_eq!(
        SearchDocument::from_v1(&doc, "some texT\n").unwrap_err(),
        SearchError::SourceMismatch
    );
    let empty = parse("");
    let view = SearchDocument::from_v1(&empty, "").unwrap();
    assert!(find(&view, "x", &options).unwrap().matches.is_empty());
}

#[test]
fn budget_truncation_and_navigation() {
    let source = "a a a\n\na a\n";
    let options = |max_matches| FindOptions {
        max_matches,
        ..FindOptions::default()
    };
    let r = find_in(source, "a", options(2));
    assert_eq!((r.matches.len(), r.truncated), (2, true));
    let r = find_in(source, "a", options(5));
    assert_eq!((r.matches.len(), r.truncated), (5, false));
    // Non-overlapping, leftmost first.
    assert_eq!(find_in("aaaa\n", "aa", options(10)).matches.len(), 2);
    let r = find_in(source, "a", options(5));
    let starts: Vec<u64> = r.matches.iter().map(|m| m.source.start).collect();
    assert_eq!(starts, [0, 2, 4, 7, 9]);
    assert_eq!(r.next_from(0), Some(0));
    assert_eq!(r.next_from(3), Some(2));
    assert_eq!(r.next_from(10), None);
    assert_eq!(r.previous_before(7), Some(2));
    assert_eq!(r.previous_before(0), None);
}

#[test]
fn cancellation_returns_no_partial_results() {
    let source = "word ".repeat(200_000);
    let doc = parse(&source);
    let view = SearchDocument::from_v1(&doc, &source).unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let options = FindOptions::default();
    assert_eq!(
        find_cancellable(&view, "word", &options, &token),
        Err(SearchError::Cancelled)
    );
    assert_eq!(
        read_stats_cancellable(&view, &ReadOptions::default(), &token),
        Err(SearchError::Cancelled)
    );
    // Cancelling from another thread while a large search runs.
    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = std::thread::spawn(move || {
        canceller.cancel();
    });
    let outcome = find_cancellable(&view, "word", &options, &token);
    worker.join().unwrap();
    assert!(matches!(outcome, Ok(_) | Err(SearchError::Cancelled)));
    let r = find(&view, "word", &options).unwrap();
    assert_eq!((r.matches.len(), r.truncated), (DEFAULT_MAX_MATCHES, true));
}

#[test]
fn large_documents_are_bounded() {
    let paragraphs: String = (0..20_000)
        .map(|i| format!("Paragraph {i} with *emphasis* and `code`.\n\n"))
        .collect();
    let line = "x".repeat(1 << 20) + "needle\n";
    for source in [paragraphs, line] {
        let doc = parse(&source);
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        let r = find(&view, "needle", &FindOptions::default()).unwrap();
        assert!(r.matches.len() <= 1);
        let r = find(&view, "emphasis and", &FindOptions::default()).unwrap();
        check_invariants(&doc, &source, "emphasis and", &FindOptions::default(), &r);
        read_stats(&view, &ReadOptions::default()).unwrap();
    }
}

#[test]
fn corpus_invariants_for_every_word() {
    for path in CORPUS {
        let source = repo(path);
        let doc = parse(&source);
        let mut queries: Vec<String> = source
            .split(|c: char| c.is_whitespace() || "*_`[]()!#|>".contains(c))
            .filter(|w| !w.is_empty())
            .take(200)
            .map(str::to_owned)
            .collect();
        queries.push("e".into());
        queries.push("the".into());
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        for query in &queries {
            for whole_word in [false, true] {
                for case_sensitive in [false, true] {
                    let options = FindOptions {
                        case_sensitive,
                        whole_word,
                        ..FindOptions::default()
                    };
                    let r = find(&view, query, &options).unwrap();
                    check_invariants(&doc, &source, query, &options, &r);
                }
            }
        }
    }
}

fn session(model: ModelVersion) -> DocumentSession {
    DocumentSession::new(
        DocumentId("doc".into()),
        SessionOptions {
            model,
            ..SessionOptions::default()
        },
    )
    .unwrap()
}
fn publish(s: &DocumentSession, source: &str) -> Arc<Snapshot> {
    match s.submit(source.as_bytes().to_vec(), None).unwrap().run() {
        Outcome::Published(snapshot) => snapshot,
        other => panic!("{other:?}"),
    }
}

#[test]
fn snapshot_results_match_documents_and_anchor() {
    for path in CORPUS {
        let source = repo(path);
        let doc = parse(&source);
        let direct = SearchDocument::from_v1(&doc, &source).unwrap();
        let snapshot = publish(&session(ModelVersion::V1), &source);
        let view = SearchDocument::from_snapshot(&snapshot);
        let options = FindOptions::default();
        assert_eq!(find(&view, "the", &options), find(&direct, "the", &options));
        assert_eq!(
            read_stats(&view, &ReadOptions::default()),
            read_stats(&direct, &ReadOptions::default())
        );
        // Matches convert to session anchors for navigation/restoration.
        for m in find(&view, "e", &options).unwrap().matches.iter().take(5) {
            let anchor = snapshot.anchor_at(m.source.start).unwrap();
            let resolved = snapshot.resolve(&anchor).unwrap().unwrap();
            assert_eq!(resolved.source_offset, m.source.start);
        }
    }
}

#[cfg(feature = "model-v2")]
#[test]
fn v1_and_v2_results_are_identical() {
    // Reference syntax differs by design between models (v1 keeps it as an
    // inert fallback, v2 resolves definitions), so parity skips those inputs.
    let skip = ["specs/semantic-model/v1/fixtures/gfm.source.txt"];
    for source in CORPUS.iter().filter(|p| !skip.contains(p)).map(|p| repo(p)) {
        let v1 = parse(&source);
        let v2 = parse_markdown_v2(
            &source,
            DocumentId("doc".into()),
            RevisionId("r1".into()),
            ParseLimits::default(),
        )
        .unwrap();
        let a = SearchDocument::from_v1(&v1, &source).unwrap();
        let b = SearchDocument::from_v2(&v2, &source).unwrap();
        let snapshot = publish(&session(ModelVersion::V2), &source);
        let c = SearchDocument::from_snapshot(&snapshot);
        for query in ["the", "link", "e", "café"] {
            let options = FindOptions::default();
            let expected = find(&a, query, &options).unwrap();
            assert_eq!(find(&b, query, &options).unwrap(), expected);
            assert_eq!(find(&c, query, &options).unwrap(), expected);
        }
        let expected = read_stats(&a, &ReadOptions::default()).unwrap();
        assert_eq!(read_stats(&b, &ReadOptions::default()).unwrap(), expected);
        assert_eq!(read_stats(&c, &ReadOptions::default()).unwrap(), expected);
        assert_eq!(
            SearchDocument::from_v2(&v2, "x").unwrap_err(),
            SearchError::SourceMismatch
        );
    }
}

#[test]
fn word_segmentation_rules() {
    // Expected values match Intl.Segmenter isWordLike (markdown-viewer) except
    // where noted: CJK ideographs/Hiragana count one per scalar and
    // Thai/Lao/Khmer/Myanmar runs count once (no dictionary segmentation).
    let cases = [
        ("Hello, world!", 2),
        ("don't stop", 2),
        ("e.g. this", 2),
        ("3.14 and 1,000.5", 3),
        ("well-known", 2),
        ("foo_bar _x", 2),
        ("abc123 x2", 2),
        ("café naïve", 2),
        ("cafe\u{301} ok", 2),
        ("한국어 단어", 2),
        ("👍 🇺🇸 👨‍👩‍👧 ok", 1),
        ("a:b c·d", 2),
        ("x.y.z", 1),
        ("it’s", 1),
        ("1️⃣ two", 2),
        ("Ⅻ ½", 1),
        ("foo—bar", 2),
        ("«quoted» text", 2),
        ("mid. Next", 2),
        ("कुछ शब्द", 2),
        ("مرحبا بالعالم", 2),
        ("Привет, мир!", 2),
        ("Γειά σου κόσμε", 3),
        ("שלום עולם", 2),
        ("l’été c’est", 2),
        ("U.S.A. 2024-10-27", 4),
        ("x²", 1),
        ("①②", 0),
        ("", 0),
        ("   \r\n\t", 0),
        // Deviations from ICU dictionary segmentation (3, 2, 5, 2 there).
        ("日本語のテキスト", 5),
        ("中文字符", 4),
        ("カタカナ ひらがな", 5),
        ("สวัสดีครับ", 1),
    ];
    for (text, expected) in cases {
        assert_eq!(word_count(text), expected, "{text:?}");
    }
}

#[test]
fn model_goldens_cover_every_node_kind() {
    // The v1 goldens include footnotes, alerts and fallbacks the basic parser
    // does not yet produce.
    for name in ["containers", "fallback", "gfm", "resources", "unicode"] {
        let dir = "specs/semantic-model/v1/fixtures/";
        let source = repo(&format!("{dir}{name}.source.txt"));
        let json = repo(&format!("{dir}{name}.json"));
        let doc = Document::from_json(json.as_bytes(), &source, ModelLimits::default()).unwrap();
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        for query in source.split_whitespace().chain(["e", "n", "Note"]) {
            for whole_word in [false, true] {
                let options = FindOptions {
                    whole_word,
                    ..FindOptions::default()
                };
                let r = find(&view, query, &options).unwrap();
                check_invariants(&doc, &source, query, &options, &r);
            }
        }
        let s = read_stats(&view, &ReadOptions::default()).unwrap();
        if name == "gfm" {
            // Done, Pending, A, B, x, y, See, footnote "Note"; the reference
            // label is a separator, not a word.
            assert_eq!(s.words, 8);
            assert_eq!(
                find(&view, "See n", &FindOptions::default())
                    .unwrap()
                    .matches
                    .len(),
                0
            );
            assert_eq!(
                find(&view, "Note", &FindOptions::default())
                    .unwrap()
                    .matches
                    .len(),
                1
            );
        }
    }
}

fn stats(source: &str, options: ReadOptions) -> ReadStats {
    let doc = parse(source);
    read_stats(&SearchDocument::from_v1(&doc, source).unwrap(), &options).unwrap()
}

#[test]
fn reading_time_rounding_and_bounds() {
    assert_eq!(estimated_seconds(0, 160), 0);
    assert_eq!(estimated_seconds(1, 160), 0); // 0.375 s
    assert_eq!(estimated_seconds(2, 160), 1); // 0.75 s
    assert_eq!(estimated_seconds(4, 480), 1); // exactly 0.5 s rounds up
    assert_eq!(estimated_seconds(80, 160), 30);
    assert_eq!(estimated_seconds(1234, 160), 463); // markdown-viewer "7 min 43 s"
    assert_eq!(estimated_seconds(u64::MAX, 60), u64::MAX);
    assert_eq!(estimated_seconds(10, 0), 0);
    let doc = parse("x\n");
    let view = SearchDocument::from_v1(&doc, "x\n").unwrap();
    for words_per_minute in [0, 59, 601] {
        let options = ReadOptions {
            words_per_minute,
            ..ReadOptions::default()
        };
        assert_eq!(
            read_stats(&view, &options),
            Err(SearchError::InvalidOptions)
        );
    }
    for words_per_minute in [MIN_WORDS_PER_MINUTE, MAX_WORDS_PER_MINUTE] {
        let options = ReadOptions {
            words_per_minute,
            ..ReadOptions::default()
        };
        assert_eq!(
            read_stats(&view, &options).unwrap().words_per_minute,
            words_per_minute
        );
    }
}

#[test]
fn what_counts_as_words() {
    let source = "---\ntitle: Not counted here\n---\n\nIntro *two* words.\n\n\
# Heading One\n\nA [link text](x) and ![alt words](i.png) and `code span`.\n\n\
```rust\nlet counted = only_with_code;\n```\n\n```mermaid\ngraph never counted\n```\n\n\
| a b | c |\n|---|---|\n| d | e f |\n\n<div>raw html</div>\n\nLast <b>x</b> words.\n";
    let s = stats(source, ReadOptions::default());
    // intro 3; section: "A link text and and" 5 + table 6 + "Last x words" 3
    // (inline tags are separators; the text between them is ordinary text).
    assert_eq!(
        s.sections.iter().map(|s| s.words).collect::<Vec<_>>(),
        [3, 14]
    );
    assert_eq!(s.words, 17);
    let s = stats(
        source,
        ReadOptions {
            include_headings: true,
            include_code: true,
            ..ReadOptions::default()
        },
    );
    // + "Heading One" 2, "code span" 2, code block "let counted only_with_code" 3
    assert_eq!(s.sections[1].words, 21);
    assert_eq!(s.words, 24);
    // A formatting boundary inside a word does not split it.
    assert_eq!(stats("foo**bar** baz\n", ReadOptions::default()).words, 2);
    // Excluded inline content separates words.
    assert_eq!(stats("foo`x`bar\n", ReadOptions::default()).words, 2);
}

#[test]
fn sections_own_and_cumulative() {
    let source = "intro words\n\n# A\n\none two three\n\n## B\n\nfour five\n\n### C\n\nsix\n\n## D\n\nseven\n\n# E\n\neight nine\n\n> ## Quoted\n>\n> ten\n";
    let s = stats(source, ReadOptions::default());
    let rows: Vec<_> = s
        .sections
        .iter()
        .map(|s| (s.level, s.words, s.cumulative_words, s.outline_index))
        .collect();
    assert_eq!(
        rows,
        [
            (0, 2, 2, None),
            (1, 3, 7, Some(0)),
            (2, 2, 3, Some(1)),
            (3, 1, 1, Some(2)),
            (2, 1, 1, Some(3)),
            (1, 2, 3, Some(4)),
            (2, 1, 1, Some(5)),
        ]
    );
    assert_eq!(s.words, 12);
    assert_eq!(s.seconds, estimated_seconds(12, DEFAULT_WORDS_PER_MINUTE));
    let doc = parse(source);
    for section in &s.sections[1..] {
        let heading = section.heading.unwrap();
        assert!(matches!(
            doc.input().nodes[heading].content,
            NodeKind::Heading { .. }
        ));
        assert_eq!(
            doc.input().outline[section.outline_index.unwrap()].node,
            doc.input().nodes[heading].id
        );
        assert_eq!(section.seconds, estimated_seconds(section.words, 160));
        assert_eq!(
            section.cumulative_seconds,
            estimated_seconds(section.cumulative_words, 160)
        );
    }
    let empty = stats("", ReadOptions::default());
    assert_eq!(empty.words, 0);
    assert_eq!(empty.sections.len(), 1);
}

#[test]
fn corpus_reading_goldens() {
    // Default options: 160 wpm, headings and code excluded. These follow the
    // current basic parser: until CORE-003B3 adds alerts and math, alert
    // labels such as `[!NOTE]` and `$...$` math count as ordinary text (the
    // gfm-tour Alerts and Math sections), so B3 is expected to change them.
    let expected: &[(&str, u64, &[u64])] = &[
        (
            "fixtures/markdown/foundation/gfm-tour.md",
            215,
            &[0, 20, 12, 10, 12, 12, 9, 52, 37, 0, 16, 25, 0, 10],
        ),
        ("fixtures/markdown/foundation/unsafe-html.md", 8, &[0, 8]),
        ("fixtures/markdown/foundation/code-fences.md", 15, &[0, 15]),
        (
            "fixtures/markdown/advanced/math-alerts.md",
            35,
            &[0, 25, 10],
        ),
        (
            "fixtures/markdown/advanced/inline-diagrams.md",
            20,
            &[0, 8, 12, 0, 0, 0, 0],
        ),
        (
            "fixtures/markdown/advanced/voice-parity.md",
            146,
            &[0, 136, 10],
        ),
        ("fixtures/markdown/paths/nfd-filename.md", 7, &[0, 7]),
        ("fixtures/markdown/paths/unicode-filename.md", 3, &[0, 3]),
        (
            "specs/semantic-model/v1/fixtures/containers.source.txt",
            7,
            &[6, 1],
        ),
        (
            "specs/semantic-model/v1/fixtures/fallback.source.txt",
            4,
            &[4],
        ),
        ("specs/semantic-model/v1/fixtures/gfm.source.txt", 7, &[7]),
        (
            "specs/semantic-model/v1/fixtures/resources.source.txt",
            3,
            &[3],
        ),
        (
            "specs/semantic-model/v1/fixtures/unicode.source.txt",
            3,
            &[0, 3],
        ),
        ("specs/parsing/basic/input.source.txt", 21, &[0, 21, 0]),
    ];
    assert_eq!(expected.len(), CORPUS.len());
    for (path, words, sections) in expected {
        let s = stats(&repo(path), ReadOptions::default());
        let actual: Vec<u64> = s.sections.iter().map(|s| s.words).collect();
        assert_eq!((s.words, actual.as_slice()), (*words, *sections), "{path}");
        assert_eq!(
            s.seconds,
            estimated_seconds(*words, DEFAULT_WORDS_PER_MINUTE)
        );
    }
    // gfm-tour at 160 wpm: 215 words is 81 s (80.625 rounded).
    assert_eq!(stats(&repo(CORPUS[0]), ReadOptions::default()).seconds, 81);
}

#[test]
fn indented_code_that_looks_fenced_maps_to_its_own_lines() {
    // Review regression: fencing comes from the model, not from the text.
    let source = "    ```\n    ```\n";
    let r = ci(source, "```");
    let starts: Vec<u64> = r.matches.iter().map(|m| m.source.start).collect();
    assert_eq!(starts, [4, 12]);
    assert!(r.matches.iter().all(|m| m.exact));
    let source = "    ~~~a\n    b\n";
    let r = ci(source, "a");
    assert_eq!(slices(source, &r), ["a"]);
    assert_eq!(r.matches[0].source.start, 7);
    // A real fence still skips its opening line.
    let source = "~~~\na\n~~~\n";
    assert_eq!(ci(source, "a").matches[0].source.start, 4);
}

#[test]
fn whitespace_from_derived_text_is_not_exact() {
    // Review regression: the code span's line ending became a space (atomic).
    let source = "`b\n`a\n";
    let r = ci(source, " a");
    assert_eq!(r.matches.len(), 1);
    let m = &r.matches[0];
    assert!(!m.exact);
    assert!(!m.segments[0].exact);
    assert!(m.segments[1].exact);
    // CRLF code line endings are derived; LF ones are exact.
    let crlf = "```\nx\r\ny\n```\n";
    assert!(!ci(crlf, "x y").matches[0].exact);
    let lf = "```\nx\ny\n```\n";
    assert!(ci(lf, "x y").matches[0].exact);
    // Soft breaks map to themselves and stay exact.
    assert!(ci("foo\r\nbar\n", "foo bar").matches[0].exact);
}

#[test]
fn boundaries_use_original_scalars_not_folded_keys() {
    // U+0345 (combining iota subscript) folds to ι but is still a mark.
    let source = "α\u{345} end\n";
    assert!(
        ci(source, "ι").matches.is_empty(),
        "lone mark is not a match"
    );
    assert!(
        ci(source, "α").matches.is_empty(),
        "no match inside the cluster"
    );
    let sensitive = FindOptions {
        case_sensitive: true,
        ..FindOptions::default()
    };
    assert!(find_in(source, "α", sensitive).matches.is_empty());
    assert_eq!(ci(source, "α\u{345}").matches.len(), 1);
}

#[test]
fn whole_word_respects_mid_letter_and_number_context() {
    let options = FindOptions {
        whole_word: true,
        ..FindOptions::default()
    };
    let source = "cat's cat. 3.14 3 1,000 e.g x:y a-b\n";
    assert_eq!(slices(source, &find_in(source, "cat", options)), ["cat"]);
    assert_eq!(find_in(source, "cat", options).matches[0].source.start, 6);
    assert_eq!(slices(source, &find_in(source, "3", options)), ["3"]);
    assert_eq!(find_in(source, "3", options).matches[0].source.start, 16);
    assert!(find_in(source, "000", options).matches.is_empty());
    assert!(find_in(source, "g", options).matches.is_empty());
    assert!(find_in(source, "y", options).matches.is_empty());
    assert_eq!(find_in(source, "b", options).matches.len(), 1);
    // Word count agrees on the same text.
    assert_eq!(word_count(source), 9);
}

#[test]
fn format_characters_are_invisible() {
    for c in ['\u{AD}', '\u{2060}', '\u{FEFF}', '\u{200E}'] {
        let source = format!("foo{c}bar baz\n");
        assert_eq!(word_count(&format!("foo{c}bar")), 1);
        let r = ci(&source, "foobar");
        assert_eq!(slices(&source, &r), [format!("foo{c}bar").as_str()]);
        assert!(r.matches[0].exact);
        assert_eq!(r.matches[0].segments.len(), 1);
        assert_eq!(ci(&source, &format!("foo{c}bar")).matches.len(), 1);
    }
    // ZWSP is a break opportunity, as in Intl.Segmenter.
    assert_eq!(word_count("foo\u{200B}bar"), 2);
}

#[test]
fn matches_in_one_atomic_node_share_a_coarse_source() {
    // Two matches inside `&amp;&amp;` both map to the entity spans; hosts
    // navigate by match index and use `source` as a coarse anchor.
    let source = "x &amp;&amp; y\n";
    let r = ci(source, "&");
    assert_eq!(slices(source, &r), ["&amp;", "&amp;"]);
    let source = "`a\nb a`\n";
    let r = ci(source, "a");
    assert_eq!(r.matches.len(), 2);
    assert_eq!(r.matches[0].source, r.matches[1].source);
    assert_ne!(
        r.matches[0].segments[0].text_start,
        r.matches[1].segments[0].text_start
    );
    assert_eq!(r.next_from(r.matches[0].source.start), Some(0));
}

fn generated_cases(count: u64) {
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
    ];
    const QUERIES: &[&str] = &[
        "a", "b", " a", "a b", "ab", "e", "*", "`", "&", "x", "中", "1",
    ];
    let mut state: u64 = 0x5eed;
    let mut next = |n: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % n
    };
    for _ in 0..count {
        let len = 1 + next(24);
        let source: String = (0..len).map(|_| PARTS[next(PARTS.len())]).collect();
        let Ok(doc) = parse_markdown(
            &source,
            DocumentId("doc".into()),
            RevisionId("r1".into()),
            ParseLimits::default(),
        ) else {
            continue;
        };
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        let query = QUERIES[next(QUERIES.len())];
        let options = FindOptions {
            case_sensitive: next(2) == 0,
            whole_word: next(2) == 0,
            ..FindOptions::default()
        };
        let r = find(&view, query, &options).unwrap();
        check_invariants(&doc, &source, query, &options, &r);
        read_stats(&view, &ReadOptions::default()).unwrap();
    }
}

#[test]
fn generated_inputs_keep_range_invariants() {
    generated_cases(
        std::env::var("SEARCH_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096),
    );
}
