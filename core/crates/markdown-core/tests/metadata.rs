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
        "# comment\ntitle: \"Hello \\u00e9 \\ud83d\\ude42\" # comment\nsingle: 'it''s # literal'\nlist: ['a,b', \"c \\\"quote\\\"\", 'x # y', 'it''s']\nurl: https://example.com/#anchor # comment\nescaped: \"line\\nnext\\tend\"\n# comment",
    ));
    let p = &doc.input().metadata.properties;
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
    assert_eq!(
        p["escaped"],
        MetadataValue::String {
            value: "line\nnext\tend".into()
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
        "x: 2026-10-27",
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
        "\u{feff}---\ntitle: bom\n---\n",
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
