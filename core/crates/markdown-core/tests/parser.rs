use markdown_core::*;
use std::path::PathBuf;

fn parse(source: &str) -> Document {
    parse_markdown(
        source,
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap_or_else(|error| panic!("{error:?}"))
}
fn kinds(d: &Document, predicate: impl Fn(&NodeKind) -> bool) -> usize {
    d.input()
        .nodes
        .iter()
        .filter(|n| predicate(&n.content))
        .count()
}
fn corpus(path: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/markdown")
            .join(path),
    )
    .unwrap()
}
#[test]
fn basic_blocks_inlines_lists_and_outline_are_semantic() {
    let source = "# Hello **world**\n\nProse *em* **strong** ~~gone~~ `x` &amp; \\*escaped*.\nnext  \nline\n\n> quote\n\n- one\n- two\n  - nested\n\n0. zero\n1. next\n\n---\n\n```rs title\nlet x = 1;\n```\n";
    let d = parse(source);
    assert_eq!(d.input().outline[0].title, "Hello world");
    assert_eq!(d.input().outline[0].anchor, "hello-world");
    assert_eq!(
        kinds(&d, |k| matches!(k, NodeKind::List { tight: true, .. })),
        3
    );
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::Emphasis { .. })), 1);
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::HardBreak {})), 1);
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::SoftBreak {})), 1);
    assert_eq!(
        kinds(
            &d,
            |k| matches!(k, NodeKind::CodeBlock {info:Some(i),text} if i=="rs title" && text=="let x = 1;\n")
        ),
        1
    );
    assert!(
        d.input()
            .nodes
            .iter()
            .any(|n| matches!(&n.content, NodeKind::Text{text} if text.contains("&")))
    );
    assert_eq!(
        d.input().capabilities.iter().copied().collect::<Vec<_>>(),
        vec![Capability::Strikethrough]
    );
}
#[test]
fn loose_and_tight_items_have_explicit_paragraphs_without_overlap() {
    for source in [
        "- a\n\n- b\n",
        "- a\n\n  b\n",
        "- a\n  > b\n",
        "- a\n\n  ---\n",
        "- [x] a *b*\n- [ ] c\n",
    ] {
        let d = parse(source);
        for n in &d.input().nodes {
            if let NodeKind::ListItem { children, .. } = &n.content {
                assert!(!children.is_empty());
                assert!(children.iter().all(|id| {
                    d.input()
                        .nodes
                        .iter()
                        .find(|n| &n.id == id)
                        .unwrap()
                        .content
                        .is_block()
                }));
            }
        }
    }
    assert_eq!(
        kinds(&parse("- a\n\n- b\n"), |k| matches!(
            k,
            NodeKind::List { tight: false, .. }
        )),
        1
    );
}
#[test]
fn unicode_crlf_entities_and_exact_destination_lexemes_survive() {
    let source = "# Café\r\n\r\n[é](<local file.md> \"title\") [x](a\\(b\\).md) [y](a&amp;b.md) <https://example.com> <a@example.com> ![**bold** `alt`](img.png)\r\n";
    let d = parse(source);
    let slices: Vec<_> = d
        .input()
        .links
        .iter()
        .map(|l| l.span.slice(source).unwrap())
        .collect();
    assert_eq!(
        slices,
        vec![
            "local file.md",
            "a\\(b\\).md",
            "a&amp;b.md",
            "https://example.com",
            "a@example.com"
        ]
    );
    assert_eq!(d.input().links[1].destination, "a(b).md");
    assert_eq!(d.input().links[2].destination, "a&b.md");
    assert_eq!(d.input().resources[0].span.slice(source), Some("img.png"));
    assert_eq!(
        kinds(
            &d,
            |k| matches!(k, NodeKind::Image {alt,..} if alt=="bold alt")
        ),
        1
    );
    assert!(
        d.input()
            .nodes
            .iter()
            .all(|n| n.span.slice(source).is_some())
    );
    assert_eq!(d.input().source, SourceInfo::of(source));
}
#[test]
fn malformed_and_empty_inputs_return_valid_documents() {
    for source in [
        "",
        " ",
        "[broken](",
        "***unclosed",
        "```rs\nunclosed",
        "![a](b",
        "[x](\"title\")",
        "[]()",
        "#\n",
        "-\n",
        "\0\r\n",
        "| a |\n|---|\n|b|\n",
        "[![i](i.png)](file.md)",
        "[a `](` b](file.md)",
        "[a [b] c](file.md)",
    ] {
        let d = parse(source);
        assert!(Document::new(d.into_input(), source, ModelLimits::default()).is_ok());
    }
}
#[test]
fn unsafe_html_and_links_are_inert_and_explicit() {
    let source = corpus("foundation/unsafe-html.md");
    let d = parse(&source);
    assert_eq!(d.input().outline[0].anchor, "unsafe");
    assert!(matches!(
        d.input().links[0].disposition,
        LinkDisposition::Denied { .. }
    ));
    assert!(
        kinds(
            &d,
            |k| matches!(k,NodeKind::UnsupportedBlock{feature,..}|NodeKind::UnsupportedInline{feature,..} if feature=="raw_html")
        ) >= 6
    );
    assert!(
        d.input()
            .diagnostics
            .iter()
            .any(|d| d.code == "raw_html_disabled")
    );
    assert!(
        d.input()
            .nodes
            .iter()
            .filter(|n| matches!(
                n.content,
                NodeKind::UnsupportedBlock { .. } | NodeKind::UnsupportedInline { .. }
            ))
            .all(|n| n.span.slice(&source).is_some())
    );
}
#[test]
fn conservative_uri_policy_denies_obfuscation_remote_images_and_traversal() {
    for destination in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "jav&#x61;script:alert(1)",
        "data:text/html,hello",
        "vbscript:x",
        "file:///tmp/x",
        "//server/x",
        "/etc/passwd",
        "C:/x",
        "../secret.md",
        "a/../secret.md",
        "%2e%2e/secret.md",
        "a%20b.md",
        "https://example.com/%0a",
    ] {
        let source = format!("[x]({destination}) ![x]({destination})");
        let d = parse(&source);
        assert_eq!(d.input().links.len(), 1, "{destination}");
        assert!(
            matches!(
                d.input().links[0].disposition,
                LinkDisposition::Denied { .. }
            ),
            "{destination}"
        );
        assert!(
            matches!(d.input().resources[0].access, ResourceAccess::Denied { .. }),
            "{destination}"
        );
    }
    let d = parse(
        "[ok](https://example.com) [mail](mailto:a@example.com) [local](docs/a.md#hello) [jump](#hello) ![local](img/a.png) ![remote](https://example.com/a.png)",
    );
    assert!(matches!(
        d.input().links[0].disposition,
        LinkDisposition::External {}
    ));
    assert!(matches!(
        d.input().links[1].disposition,
        LinkDisposition::External {}
    ));
    assert!(matches!(
        d.input().links[2].disposition,
        LinkDisposition::RequiresCapability {}
    ));
    assert!(matches!(
        d.input().links[3].disposition,
        LinkDisposition::Fragment { .. }
    ));
    assert!(matches!(
        d.input().resources[0].access,
        ResourceAccess::RequiresCapability {}
    ));
    assert!(matches!(
        d.input().resources[1].access,
        ResourceAccess::Denied { .. }
    ));
}
#[test]
fn front_matter_reference_links_and_engines_are_explicitly_deferred() {
    let source = "---\r\ntitle: Example\r\n---\r\n# Body\r\n\r\n[ref][id] ![alt][id]\r\n\r\n[id]: local.md\r\n\r\n```mermaid\r\ngraph TD; A-->B\r\n```\r\n";
    let d = parse(source);
    assert_eq!(d.input().outline.len(), 1);
    assert_eq!(
        d.input().metadata.front_matter_span.unwrap().slice(source),
        Some("---\r\ntitle: Example\r\n---\r\n")
    );
    assert!(d.input().metadata.properties.is_empty());
    assert!(d.input().links.is_empty() && d.input().resources.is_empty());
    assert_eq!(
        kinds(&d, |k| matches!(k, NodeKind::UnsupportedInline { .. })),
        2
    );
    assert_eq!(
        kinds(
            &d,
            |k| matches!(k,NodeKind::UnsupportedBlock{feature,..} if feature=="mermaid")
        ),
        1
    );
    assert!(
        parse("# Body\n\n---\ntitle: body\n---\n")
            .input()
            .metadata
            .front_matter_span
            .is_none()
    );
    assert!(
        parse("---\nunclosed\n")
            .input()
            .metadata
            .front_matter_span
            .is_none()
    );
}
#[test]
fn frozen_foundation_corpus_has_expected_basic_semantics() {
    let d = parse(&corpus("foundation/gfm-tour.md"));
    assert_eq!(d.input().outline.len(), 13);
    assert_eq!(
        d.input().outline.first().unwrap().anchor,
        "gfm-feature-tour"
    );
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::Table { .. })), 1);
    assert_eq!(
        kinds(&d, |k| matches!(
            k,
            NodeKind::ListItem {
                checked: Some(_),
                ..
            }
        )),
        3
    );
    assert_eq!(d.input().links.len(), 6);
    let d = parse(&corpus("foundation/code-fences.md"));
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::CodeBlock { .. })), 4);
    for path in [
        "paths/unicode-filename.md",
        "paths/nfd-filename.md",
        "advanced/math-alerts.md",
        "advanced/inline-diagrams.md",
        "advanced/voice-parity.md",
    ] {
        parse(&corpus(path));
    }
}
#[test]
fn anchors_are_deterministic_unique_and_not_snapshot_identity() {
    let source = "# A\n# A\n# A-1\n# A\n# 💡\n# 💡\n";
    let d = parse(source);
    assert_eq!(
        d.input()
            .outline
            .iter()
            .map(|e| e.anchor.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "a-1", "a-1-1", "a-2", "section", "section-1"]
    );
    assert_eq!(d.to_json().unwrap(), parse(source).to_json().unwrap());
    parse(&"# A\n".repeat(10_000));
}
#[test]
fn every_budget_is_enforced_and_errors_keep_source_private() {
    let run = |s: &str, l: ParseLimits| {
        parse_markdown(s, DocumentId("doc".into()), RevisionId("r1".into()), l).unwrap_err()
    };
    let mut l = ParseLimits::default();
    l.model.max_source_bytes = 0;
    assert_eq!(run("private", l), ParseError::SourceLimit);
    let l = ParseLimits {
        max_events: 0,
        ..ParseLimits::default()
    };
    assert_eq!(run("private", l), ParseError::EventLimit);
    let mut l = ParseLimits::default();
    l.model.max_nodes = 0;
    assert_eq!(run("private", l), ParseError::NodeLimit);
    let mut l = ParseLimits::default();
    l.model.max_depth = 1;
    assert_eq!(run("*private*", l), ParseError::DepthLimit);
    let l = ParseLimits {
        max_text_bytes: 0,
        ..ParseLimits::default()
    };
    let e = run("private", l);
    assert_eq!(e, ParseError::TextLimit);
    assert!(!format!("{e:?} {e}").contains("private"));
    let e = parse_markdown(
        "text",
        DocumentId("bad/id".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap_err();
    assert!(matches!(
        e,
        ParseError::Model(ModelError {
            code: "invalid_id",
            ..
        })
    ));
    parse_markdown(
        "",
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits {
            max_events: 0,
            max_text_bytes: 0,
            model: ModelLimits {
                max_nodes: 0,
                max_depth: 0,
                ..ModelLimits::default()
            },
        },
    )
    .unwrap();
}

#[test]
fn original_basic_snapshot_matches_frozen_semantic_v1() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../specs/parsing/basic");
    let source = std::fs::read_to_string(base.join("input.source.txt")).unwrap();
    let doc = parse_markdown(
        &source,
        DocumentId("example".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(base.join("expected.json")).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&doc).unwrap(), expected);
    assert_eq!(
        Document::from_json(&doc.to_json().unwrap(), &source, ModelLimits::default()).unwrap(),
        doc
    );
}

#[test]
fn heading_html_tags_are_inert_source_but_not_outline_labels() {
    let d = parse("# Safe <em>visible</em> text\n");
    assert_eq!(d.input().outline[0].title, "Safe visible text");
    assert_eq!(d.input().outline[0].anchor, "safe-visible-text");
    assert_eq!(
        kinds(
            &d,
            |k| matches!(k,NodeKind::UnsupportedInline{feature,..} if feature=="raw_html")
        ),
        2
    );
    // D-015 delimiter precedence is intentional even when the body is not a mapping.
    let source = "---\n# Actual heading\n---\n";
    let d = parse(source);
    assert!(d.input().outline.is_empty());
    assert_eq!(
        d.input().metadata.front_matter_span.unwrap().slice(source),
        Some(source)
    );
    assert!(
        d.input()
            .diagnostics
            .iter()
            .any(|d| d.code == "front_matter_deferred")
    );
}

#[test]
fn grok_email_autolinks_are_external_mailto_intents() {
    let source = "<user@example.com>";
    let d = parse(source);
    assert_eq!(d.input().links[0].destination, "mailto:user@example.com");
    assert!(matches!(
        d.input().links[0].disposition,
        LinkDisposition::External {}
    ));
    assert_eq!(
        d.input().links[0].span.slice(source),
        Some("user@example.com")
    );
}

#[test]
fn grok_valid_markdown_must_not_fail_model_validation() {
    for source in [
        "[<http://a>](b)",
        "[x <a@b.c>](b)",
        "1. >\t- a",
        "- > <div>\n\t - x",
    ] {
        let d = parse(source);
        assert!(Document::new(d.into_input(), source, ModelLimits::default()).is_ok());
    }
}

#[test]
fn grok_unicode_format_and_bidi_destinations_are_denied() {
    for control in [
        '\u{200b}', '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}',
        '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        '\u{feff}', '\u{ad}',
    ] {
        for destination in [
            format!("https://exa{control}mple.com"),
            format!("docs/a{control}b.md"),
            format!("#a{control}b"),
        ] {
            let source = format!("[x]({destination}) ![alt]({destination})");
            let d = parse(&source);
            assert!(
                matches!(
                    d.input().links[0].disposition,
                    LinkDisposition::Denied { .. }
                ),
                "U+{:04X}",
                control as u32
            );
            assert!(matches!(
                d.input().resources[0].access,
                ResourceAccess::Denied { .. }
            ));
        }
    }
}

#[test]
fn nested_autolinks_preserve_source_without_nested_activation() {
    for (source, inner) in [
        ("[<http://a>](b)", "<http://a>"),
        ("[x <a@b.c>](b)", "<a@b.c>"),
    ] {
        let d = parse(source);
        assert_eq!(d.input().links.len(), 1);
        assert_eq!(d.input().links[0].destination, "b");
        assert_eq!(d.input().links[0].span.slice(source), Some("b"));
        let fallback=d.input().nodes.iter().find(|n|matches!(&n.content,NodeKind::UnsupportedInline{feature,..} if feature=="nested_link")).unwrap();
        assert_eq!(fallback.span.slice(source), Some(inner));
        assert!(
            d.input()
                .diagnostics
                .iter()
                .any(|d| d.code == "nested_link_disabled")
        );
    }
}

#[test]
fn incompatible_tab_container_offsets_fall_back_to_exact_inert_body() {
    for body in ["1. >\t- a", "- > <div>\n\t - x"] {
        for prefix in ["", "---\r\ntitle: private\r\n---\r\n"] {
            let source = format!("{prefix}{body}");
            let d = parse(&source);
            assert_eq!(d.input().nodes.len(), 1);
            assert!(
                matches!(&d.input().nodes[0].content,NodeKind::UnsupportedBlock{feature,..} if feature=="unrepresentable_source_mapping")
            );
            assert_eq!(d.input().nodes[0].span.slice(&source), Some(body));
            assert!(
                d.input().links.is_empty()
                    && d.input().resources.is_empty()
                    && d.input().outline.is_empty()
            );
            assert!(
                d.input()
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "source_mapping_fallback")
            );
            assert_eq!(d.input().source, SourceInfo::of(&source));
            assert_eq!(d.input().document_id.0, "doc");
            assert_eq!(d.input().revision_id.0, "r1");
            assert_eq!(
                d.input()
                    .metadata
                    .front_matter_span
                    .map(|s| s.slice(&source).unwrap()),
                if prefix.is_empty() {
                    None
                } else {
                    Some(prefix)
                }
            );
            if !prefix.is_empty() {
                assert!(d.input().capabilities.contains(&Capability::FrontMatter));
                assert!(
                    d.input()
                        .diagnostics
                        .iter()
                        .any(|d| d.code == "front_matter_deferred")
                );
            }
        }
    }
    // Semantic data/activation outside the conflicting container is discarded too.
    let source = "# Heading\n\n[link](a.md) ![image](i.png)\n\n1. >\t- a";
    let d = parse(source);
    assert_eq!(d.input().nodes[0].span.slice(source), Some(source));
    assert!(
        d.input().links.is_empty()
            && d.input().resources.is_empty()
            && d.input().outline.is_empty()
    );
}

#[test]
fn ascii_only_activation_keeps_denied_unicode_values_visible() {
    for destination in [
        "café.md",
        "cafe\u{301}.md",
        "https://例え.test",
        "#café",
        "🙂.png",
    ] {
        let source = format!("[label]({destination}) ![alt]({destination})");
        let d = parse(&source);
        assert_eq!(d.input().links[0].destination, destination);
        assert!(matches!(
            d.input().links[0].disposition,
            LinkDisposition::Denied { .. }
        ));
        assert_eq!(d.input().resources[0].destination, destination);
        assert!(matches!(
            d.input().resources[0].access,
            ResourceAccess::Denied { .. }
        ));
        assert_eq!(
            kinds(&d, |k| matches!(k,NodeKind::Image{alt,..} if alt=="alt")),
            1
        );
    }
}

#[test]
fn bounded_generated_markup_never_publishes_an_invalid_model() {
    let tokens = [
        "[",
        "]",
        "(",
        ")",
        "<http://a>",
        "<a@b.c>",
        "1. ",
        "- ",
        ">",
        "\t",
        "\n",
        "<div>",
        "a",
        "é",
        " ",
        "*",
        "`",
        "!",
        "|",
        "\\",
        "&amp;",
        "#",
        "---",
        "\r\n",
    ];
    let mut seed = 0x9e3779b97f4a7c15u64;
    for case in 0..20_000 {
        let mut source = String::new();
        for _ in 0..(case % 20 + 1) {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            source.push_str(tokens[(seed >> 32) as usize % tokens.len()]);
        }
        match parse_markdown(
            &source,
            DocumentId("d".into()),
            RevisionId("r".into()),
            ParseLimits::default(),
        ) {
            Ok(doc) => {
                Document::new(doc.into_input(), &source, ModelLimits::default()).unwrap();
            }
            Err(
                ParseError::SourceLimit
                | ParseError::EventLimit
                | ParseError::NodeLimit
                | ParseError::DepthLimit
                | ParseError::TextLimit,
            ) => {}
            Err(error) => panic!("case {case}: {source:?}: {error:?}"),
        }
    }
}
