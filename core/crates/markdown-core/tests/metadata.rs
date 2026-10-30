use markdown_core::*;
use std::path::PathBuf;

fn parse(source: &str) -> Document {
    parse_markdown(
        source,
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap()
}
fn wrapped(body: &str) -> String {
    format!("---\n{body}\n---\n# Body\n")
}
fn rejected(body: &str, code: &str) {
    let source = wrapped(body);
    let doc = parse(&source);
    assert!(doc.input().metadata.properties.is_empty(), "{body:?}");
    assert!(doc.input().metadata.title.is_none());
    assert_eq!(doc.input().outline[0].title, "Body");
    assert!(
        doc.input().diagnostics.iter().any(|d| d.code == code),
        "{body:?}: {:?}",
        doc.input().diagnostics
    );
    assert!(
        doc.input()
            .metadata
            .front_matter_span
            .unwrap()
            .slice(&source)
            .unwrap()
            .contains(body)
    );
}
#[test]
fn supported_values_have_exact_model_types_and_title() {
    let source = wrapped(
        "title: 'Café é'\nnull1: null\nnull2: ~\nempty:\nflag: false\nyes: TRUE\nmin: -9223372036854775808\nmax: 9223372036854775807\nzero: 0\ntext: ordinary text\nquoted: \"42\"\ntags: [gfm, 'demo']\nblock:\n  - first\n  - \"second\"\nnone: []",
    );
    let doc = parse(&source);
    let m = &doc.input().metadata;
    assert_eq!(m.title.as_deref(), Some("Café é"));
    for key in ["null1", "null2", "empty"] {
        assert_eq!(m.properties[key], MetadataValue::Null {});
    }
    assert_eq!(
        m.properties["flag"],
        MetadataValue::Boolean { value: false }
    );
    assert_eq!(m.properties["yes"], MetadataValue::Boolean { value: true });
    assert_eq!(
        m.properties["min"],
        MetadataValue::Integer { value: i64::MIN }
    );
    assert_eq!(
        m.properties["max"],
        MetadataValue::Integer { value: i64::MAX }
    );
    assert_eq!(m.properties["zero"], MetadataValue::Integer { value: 0 });
    assert_eq!(
        m.properties["quoted"],
        MetadataValue::String { value: "42".into() }
    );
    assert_eq!(
        m.properties["tags"],
        MetadataValue::Strings {
            value: vec!["gfm".into(), "demo".into()]
        }
    );
    assert_eq!(
        m.properties["block"],
        MetadataValue::Strings {
            value: vec!["first".into(), "second".into()]
        }
    );
    assert_eq!(
        m.properties["none"],
        MetadataValue::Strings { value: vec![] }
    );
    assert!(doc.input().diagnostics.is_empty());
    assert!(doc.input().capabilities.contains(&Capability::FrontMatter));
    assert_eq!(
        Document::from_json(&doc.to_json().unwrap(), &source, ModelLimits::default()).unwrap(),
        doc
    );
}
#[test]
fn quotes_comments_escapes_and_unicode_are_unambiguous() {
    let doc = parse(&wrapped(
        "# comment\ntitle: \"Hello \\u00e9 \\ud83d\\ude42\" # comment\nsingle: 'it''s # literal'\nlist: ['a,b', \"c \\\"quote\\\"\", 'x # y', 'it''s']\nurl: https://example.com/#anchor # comment\nescaped: \"literal\\\\n\"\n# comment",
    ));
    let p = &doc.input().metadata.properties;
    assert_eq!(
        p["escaped"],
        MetadataValue::String {
            value: r"literal\n".into()
        }
    );
    assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello é 🙂"));
    assert_eq!(
        p["single"],
        MetadataValue::String {
            value: "it's # literal".into()
        }
    );
    assert_eq!(
        p["list"],
        MetadataValue::Strings {
            value: vec![
                "a,b".into(),
                "c \"quote\"".into(),
                "x # y".into(),
                "it's".into()
            ]
        }
    );
    assert_eq!(
        p["url"],
        MetadataValue::String {
            value: "https://example.com/#anchor".into()
        }
    );

    assert!(doc.input().diagnostics.is_empty());
}
#[test]
fn unsupported_or_hostile_metadata_never_publishes_partial_properties() {
    for bad in [
        "x: &anchor value",
        "x: *alias",
        "x: !!str text",
        "x: {a: b}",
        "x:\n  nested: value",
        "x: [a, [b]]",
        "x: [true]",
        "x: [1]",
        "x: [null]",
        "x:\n  - false",
        "x: |\n  text",
        "x: >\n  text",
        "<<: *base",
        "x: 1.2",
        "x: .5",
        "x: -.5",
        "x: .NaN",
        "x: 01",
        "x: +2",
        "x: 9223372036854775808",
        "x: -9223372036854775809",
        "x: - item",
        "x: ? item",
        "x: a: b",
        "x: [a,]",
        "x: [a,,b]",
        "x: [a] junk",
        "x: [a",
        "x: \"unclosed",
        "x: 'unclosed",
        "x: \"a\" junk",
        "x: \"\\q\"",
        "x: \"\\ud800\"",
        "x:value",
        "'x': value",
        " x: value",
        "x: a\n\t- b",
        "x: \"literal\ttab\"",
        "%YAML 1.2\nx: y",
    ] {
        rejected(
            &format!("title: should-not-publish\n{bad}"),
            "front_matter_unsupported",
        );
    }
}
#[test]
fn duplicate_keys_are_atomic_and_diagnostics_do_not_echo_private_values() {
    for body in [
        "secret: value\nsecret: changed",
        "tags:\n  - private\ntags: [changed]",
        "title: private\ntitle: changed",
    ] {
        rejected(body, "front_matter_duplicate");
        let doc = parse(&wrapped(body));
        for d in &doc.input().diagnostics {
            assert!(!format!("{d:?}").contains("private"));
            assert!(!format!("{d:?}").contains("secret"));
        }
    }
}
#[test]
fn metadata_hard_limits_accept_the_boundary_and_reject_one_more() {
    let properties = |count| {
        (0..count)
            .map(|i| format!("key{i}: x"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        parse(&wrapped(&properties(256)))
            .input()
            .metadata
            .properties
            .len(),
        256
    );
    rejected(&properties(257), "front_matter_limit");
    assert_eq!(
        parse(&wrapped(&format!("{}: x", "a".repeat(128))))
            .input()
            .metadata
            .properties
            .len(),
        1
    );
    rejected(&format!("{}: x", "a".repeat(129)), "front_matter_limit");
    assert_eq!(
        parse(&wrapped(&format!("text: {}", "a".repeat(16 * 1024))))
            .input()
            .metadata
            .properties
            .len(),
        1
    );
    rejected(
        &format!("text: {}", "a".repeat(16 * 1024 + 1)),
        "front_matter_limit",
    );
    let list = |count| format!("tags: [{}]", vec!["x"; count].join(","));
    assert!(parse(&wrapped(&list(1024))).input().diagnostics.is_empty());
    rejected(&list(1025), "front_matter_limit");
    rejected(
        &format!("a: [{}]\nb: [x]", vec!["x"; 1024].join(",")),
        "front_matter_limit",
    );
    for extra in [0, 1] {
        let source = format!("---\n#{}\n---\n", "x".repeat(64 * 1024 - 10 + extra));
        assert_eq!(source.len(), 64 * 1024 + extra);
        let doc = parse(&source);
        assert_eq!(
            doc.input()
                .diagnostics
                .iter()
                .any(|d| d.code == "front_matter_limit"),
            extra == 1
        );
    }
}
#[test]
fn metadata_charges_the_shared_copy_budget_including_failed_attempts_and_title() {
    let run = |source: &str, bytes| {
        parse_markdown(
            source,
            DocumentId("d".into()),
            RevisionId("r".into()),
            ParseLimits {
                max_text_bytes: bytes,
                ..ParseLimits::default()
            },
        )
    };
    let source = "---\ntitle: x\n---\n";
    assert!(run(source, 7).is_ok());
    assert_eq!(run(source, 6).unwrap_err(), ParseError::TextLimit);
    assert_eq!(
        run("---\nx: *alias\n---\n", 0).unwrap_err(),
        ParseError::TextLimit
    );
    // Failed metadata consumed 1 key + 1 value; subsequent body still shares that counter.
    assert_eq!(
        run("---\nx: a\nx: b\n---\nc", 2).unwrap_err(),
        ParseError::TextLimit
    );
    assert!(run("---\n# empty mapping\n---\n", 0).is_ok());
}
#[test]
fn original_source_boundaries_and_body_offsets_are_preserved() {
    for newline in ["\n", "\r\n"] {
        let front = format!("---{newline}title: Café é{newline}...{newline}");
        let source = format!("{front}# Body{newline}");
        let doc = parse(&source);
        assert_eq!(
            doc.input()
                .metadata
                .front_matter_span
                .unwrap()
                .slice(&source),
            Some(front.as_str())
        );
        assert_eq!(doc.input().metadata.title.as_deref(), Some("Café é"));
        assert_eq!(doc.input().nodes[0].span.start, front.len() as u64);
        assert_eq!(doc.input().source, SourceInfo::of(&source));
    }
    for source in [
        "# Body\n\n---\ntitle: body\n---\n",
        "---\ntitle: unclosed\n",
    ] {
        let doc = parse(source);
        assert!(doc.input().metadata.front_matter_span.is_none());
        assert!(doc.input().metadata.properties.is_empty());
    }
    let doc = parse("---\ntitle: 42\n---\n");
    assert_eq!(
        doc.input().metadata.properties["title"],
        MetadataValue::Integer { value: 42 }
    );
    assert!(doc.input().metadata.title.is_none());
}
#[test]
fn tab_offset_fallback_preserves_typed_properties_and_metadata_diagnostics() {
    for body in [
        "title: retained\ntags: [a,b]",
        "title: private\ntitle: duplicate",
    ] {
        let source = format!("---\n{body}\n---\n1. >\t- a");
        let doc = parse(&source);
        assert_eq!(doc.input().nodes.len(), 1);
        assert!(
            doc.input()
                .diagnostics
                .iter()
                .any(|d| d.code == "source_mapping_fallback")
        );
        if body.contains("duplicate") {
            assert!(doc.input().metadata.properties.is_empty());
            assert!(
                doc.input()
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "front_matter_duplicate")
            );
        } else {
            assert_eq!(doc.input().metadata.title.as_deref(), Some("retained"));
            assert_eq!(doc.input().metadata.properties.len(), 2);
            assert!(
                !doc.input()
                    .diagnostics
                    .iter()
                    .any(|d| d.code.starts_with("front_matter_"))
            );
        }
    }
}
#[test]
fn frozen_gfm_fixture_properties_match_the_supported_expectations() {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/markdown/foundation/gfm-tour.md"),
    )
    .unwrap();
    let doc = parse(&source);
    let p = &doc.input().metadata.properties;
    assert_eq!(p.len(), 4);
    assert_eq!(
        p["description"],
        MetadataValue::String {
            value: "A tour of GFM features".into()
        }
    );
    assert_eq!(
        p["author"],
        MetadataValue::String {
            value: "mdv examples".into()
        }
    );
    assert_eq!(
        p["tags"],
        MetadataValue::Strings {
            value: vec!["gfm".into(), "demo".into()]
        }
    );
    assert_eq!(p["draft"], MetadataValue::Boolean { value: false });
    assert_eq!(doc.input().outline.len(), 13);
}
#[test]
fn bounded_generated_metadata_returns_valid_snapshots_or_copy_limits() {
    let tokens = [
        "key: ", "x", "é", "\n", "  - ", "'", "\"", "[", "]", ",", "*a", "&b", "null", "42",
        "\\u00e9", "#", "\t", ": ", "{", "}",
    ];
    let mut seed = 0x12345678u64;
    for case in 0..4096 {
        let mut body = String::new();
        for _ in 0..(case % 24 + 1) {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            body.push_str(tokens[(seed >> 32) as usize % tokens.len()]);
        }
        let source = wrapped(&body);
        let doc = parse(&source);
        Document::new(doc.into_input(), &source, ModelLimits::default()).unwrap();
    }
}

#[test]
fn grok_review_reproductions() {
    let doc = parse(&wrapped("title: \"\\u0000x\""));
    assert!(doc.input().metadata.properties.is_empty());
    for title in ["\"\"", "\"   \""] {
        assert!(
            parse(&wrapped(&format!("title: {title}")))
                .input()
                .metadata
                .title
                .is_none()
        );
    }
    let doc = parse(&wrapped("title: Hello\ndate: 2024-01-01\ntags:\n- a\n- b"));
    assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello"));
    assert_eq!(
        doc.input().metadata.properties["date"],
        MetadataValue::String {
            value: "2024-01-01".into()
        }
    );
    let doc = parse("\u{feff}---\ntitle: Hello\n---\nBody");
    assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello"));
}

#[test]
fn decoded_and_literal_control_format_values_are_rejected_atomically() {
    for code in [
        0, 9, 10, 13, 0x7f, 0x85, 0xad, 0x600, 0x61c, 0x200b, 0x200e, 0x200f, 0x202a, 0x202b,
        0x202c, 0x202d, 0x202e, 0x2060, 0x2066, 0x2067, 0x2068, 0x2069, 0xfeff, 0xe0001, 0xe0020,
        0xe0067, 0xe007f,
    ] {
        let c = char::from_u32(code).unwrap();
        let encoded = serde_json::to_string(&format!("before{c}after")).unwrap();
        for value in [encoded.clone(), format!("[{encoded}]")] {
            rejected(
                &format!("title: legitimate\nunsafe: {value}"),
                "front_matter_unsupported",
            );
        }
        if !c.is_control() {
            rejected(
                &format!("title: before{c}after"),
                "front_matter_unsupported",
            );
            rejected(
                &format!("title: 'before{c}after'"),
                "front_matter_unsupported",
            );
        }
    }
    for title in ["", "   ", "\u{a0}"] {
        let doc = parse(&wrapped(&format!(
            "title: {}",
            serde_json::to_string(title).unwrap()
        )));
        assert!(doc.input().metadata.title.is_none());
        assert_eq!(
            doc.input().metadata.properties["title"],
            MetadataValue::String {
                value: title.into()
            }
        );
    }
}

// Non-Cf default-ignorable values and U+2800 (CORE-003E, D-032): the Hangul
// fillers, Khmer inherent vowels, unassigned default-ignorables and tags-block
// gaps. The display modifiers stay allowed.
const BLANK_INVISIBLE: [u32; 14] = [
    0x115f, 0x1160, 0x17b4, 0x17b5, 0x2065, 0x2800, 0x3164, 0xffa0, 0xfff0, 0xfff8, 0xe0000,
    0xe0002, 0xe0080, 0xe0fff,
];
const DISPLAY_MODIFIERS: [u32; 8] = [
    0x34f, 0x180b, 0x180d, 0x180f, 0xfe00, 0xfe0f, 0xe0100, 0xe01ef,
];

#[test]
fn blank_invisible_values_are_rejected_atomically() {
    for code in BLANK_INVISIBLE {
        let c = char::from_u32(code).unwrap();
        let encoded = serde_json::to_string(&format!("before{c}after")).unwrap();
        let escaped = format!("\"before\\u{code:04x}after\"");
        for value in [
            encoded.clone(),
            format!("[{encoded}]"),
            format!("before{c}after"),
            format!("'before{c}after'"),
            format!("'{c}'"),
        ] {
            rejected(
                &format!("title: legitimate\nunsafe: {value}"),
                "front_matter_unsupported",
            );
            rejected(&format!("title: {value}"), "front_matter_unsupported");
        }
        if code <= 0xffff {
            rejected(&format!("title: {escaped}"), "front_matter_unsupported");
        }
        rejected(
            &format!("tags:\n  - fine\n  - before{c}after"),
            "front_matter_unsupported",
        );
    }
}

#[test]
fn display_modifiers_stay_allowed_in_metadata_text() {
    for code in DISPLAY_MODIFIERS {
        let c = char::from_u32(code).unwrap();
        let value = format!("a{c}b");
        let doc = parse(&wrapped(&format!(
            "title: {value}\ntags: [{}]",
            serde_json::to_string(&value).unwrap()
        )));
        assert!(doc.input().diagnostics.is_empty(), "{code:x}");
        assert_eq!(doc.input().metadata.title.as_deref(), Some(value.as_str()));
        assert_eq!(
            doc.input().metadata.properties["tags"],
            MetadataValue::Strings { value: vec![value] }
        );
    }
}

// Joiners and subdivision flags (CORE-003F, D-034): allowed in titles and
// string/list values, never in keys; other tag characters stay rejected.
const JOINED: [&str; 5] = [
    "\u{645}\u{6cc}\u{200c}\u{62e}\u{648}\u{627}\u{647}\u{645}",
    "\u{915}\u{94d}\u{200c}\u{937}",
    "\u{915}\u{94d}\u{200d}\u{937}",
    "\u{1f469}\u{200d}\u{1f4bb} dev",
    "\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f} Scotland",
];
const MALFORMED_TAGS: [&str; 8] = [
    "a\u{e0067}\u{e0062}\u{e007f}",
    "\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}",
    "\u{1f3f4}\u{e0067}\u{e0062}\u{e007f}",
    "\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e0061}\u{e0062}\u{e0063}\u{e007f}",
    "\u{1f3f4}\u{e0047}\u{e0042}\u{e0053}\u{e007f}",
    "\u{1f3f4}\u{e0067}\u{e0020}\u{e0073}\u{e007f}",
    "\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e007f}\u{e0074}",
    "\u{1f3f4}\u{e007f}",
];

#[test]
fn joiners_and_subdivision_flags_are_allowed_in_metadata_text() {
    for value in JOINED {
        let json = serde_json::to_string(value).unwrap();
        for body in [
            format!("title: {value}\ntags: [{json}]"),
            format!("title: {json}\ntags:\n  - fine\n  - {value}"),
            format!("title: '{value}'\nnote: {value}"),
        ] {
            let doc = parse(&wrapped(&body));
            assert!(doc.input().diagnostics.is_empty(), "{body:?}");
            assert_eq!(doc.input().metadata.title.as_deref(), Some(value));
        }
    }
    for value in MALFORMED_TAGS {
        let json = serde_json::to_string(value).unwrap();
        rejected(&format!("title: {value}"), "front_matter_unsupported");
        rejected(
            &format!("title: legitimate\ntags: [{json}]"),
            "front_matter_unsupported",
        );
    }
    // Keys are ASCII identifiers, and bidi controls, LRM and RLM stay rejected.
    rejected("ti\u{200d}tle: Hello", "front_matter_unsupported");
    for c in ['\u{200e}', '\u{200f}', '\u{202b}', '\u{2067}'] {
        rejected(&format!("title: a\u{200c}{c}b"), "front_matter_unsupported");
    }
}

#[test]
fn iso_dates_and_datetimes_remain_literal_strings() {
    for value in [
        "2024-01-01",
        "2024-02-29",
        "2000-02-29",
        "2024-01-01T12:34",
        "2024-01-01 12:34:56",
        "2024-01-01T12:34Z",
        "2024-01-01T12:34:56+05:30",
        "2024-01-01T00:00-07:00",
    ] {
        let doc = parse(&wrapped(&format!("title: Hello\ndate: {value} # comment")));
        assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello"));
        assert_eq!(
            doc.input().metadata.properties["date"],
            MetadataValue::String {
                value: value.into()
            }
        );
    }
    for value in [
        "2023-02-29",
        "1900-02-29",
        "2024-00-01",
        "2024-13-01",
        "2024-01-00",
        "2024-04-31",
        "0000-01-01",
        "2024-01-01T24:00",
        "2024-01-01T00:60",
        "2024-01-01T00:00:60",
        "2024-01-01T00:00+24:00",
        "2024-01-01T00:00+00:60",
        "2024-01-01junk",
        "2024-01-01T00:00:",
        "2024-01-01T00:00.5",
        "2024-01-01T00:00+05",
    ] {
        rejected(&format!("date: {value}"), "front_matter_unsupported");
    }
}

#[test]
fn block_lists_allow_indentless_or_consistent_positive_indentation() {
    for indent in [0, 1, 2, 4, 8] {
        let pad = " ".repeat(indent);
        let source = wrapped(&format!(
            "title: Hello\ntags:\n{pad}- a\n# comment\n{pad}- b\ndraft: false"
        ));
        let doc = parse(&source);
        assert_eq!(
            doc.input().metadata.properties["tags"],
            MetadataValue::Strings {
                value: vec!["a".into(), "b".into()]
            }
        );
        assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello"));
    }
    for body in [
        "tags:\n- a\n  - b",
        "tags:\n    - a\n- b",
        "tags: scalar\n- a",
        "- orphan",
        "tags:\n  - a\n  - - nested",
    ] {
        rejected(body, "front_matter_unsupported");
    }
}

#[test]
fn bom_and_blank_line_detection_preserve_exact_offsets() {
    for nl in ["\n", "\r\n"] {
        let front = format!("\u{feff}---{nl}title: Hello{nl}---{nl}");
        let source = format!("{front}Body{nl}");
        let doc = parse(&source);
        assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello"));
        assert_eq!(
            doc.input()
                .metadata
                .front_matter_span
                .unwrap()
                .slice(&source),
            Some(front.as_str())
        );
        assert_eq!(doc.input().nodes[0].span.start, front.len() as u64);
        let source = format!("\u{feff}---{nl}{nl}Intro paragraph{nl}{nl}Section{nl}---{nl}more");
        let doc = parse(&source);
        assert!(doc.input().metadata.front_matter_span.is_none());
        assert_eq!(doc.input().outline[0].title, "Section");
        assert!(
            doc.input()
                .nodes
                .iter()
                .any(|n| matches!(&n.content,NodeKind::Text{text} if text=="Intro paragraph"))
        );
    }
}

#[test]
fn legacy_yaml_booleans_are_documented_literal_strings_and_comma_is_reserved() {
    for value in ["yes", "NO", "on", "Off", "y", "N"] {
        assert_eq!(
            parse(&wrapped(&format!("draft: {value}")))
                .input()
                .metadata
                .properties["draft"],
            MetadataValue::String {
                value: value.into()
            }
        );
    }
    rejected("x: ,indicator", "front_matter_unsupported");
}

#[test]
fn static_site_style_header_matches_typed_metadata_golden() {
    let source = "\u{feff}---\ntitle: Hello\ndate: 2024-02-29T12:34:56+05:30\ntags:\n- rust\n- 'offline'\nauthors:\n    - Ada\n    - Grace\ndraft: false\n---\n# Article\n";
    let doc = parse(source);
    assert_eq!(
        serde_json::to_value(&doc.input().metadata.properties).unwrap(),
        serde_json::json!({
            "title":{"kind":"string","value":"Hello"},
            "date":{"kind":"string","value":"2024-02-29T12:34:56+05:30"},
            "tags":{"kind":"strings","value":["rust","offline"]},
            "authors":{"kind":"strings","value":["Ada","Grace"]},
            "draft":{"kind":"boolean","value":false}
        })
    );
    assert_eq!(doc.input().metadata.title.as_deref(), Some("Hello"));
    assert_eq!(doc.input().outline[0].title, "Article");
    assert!(doc.input().diagnostics.is_empty());
}
