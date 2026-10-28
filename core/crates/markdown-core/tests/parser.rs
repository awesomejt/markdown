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
fn front_matter_is_typed_while_reference_links_and_engines_are_deferred() {
    let source = "---\r\ntitle: Example\r\n---\r\n# Body\r\n\r\n[ref][id] ![alt][id]\r\n\r\n[id]: local.md\r\n\r\n```mermaid\r\ngraph TD; A-->B\r\n```\r\n";
    let d = parse(source);
    assert_eq!(d.input().outline.len(), 1);
    assert_eq!(
        d.input().metadata.front_matter_span.unwrap().slice(source),
        Some("---\r\ntitle: Example\r\n---\r\n")
    );
    assert_eq!(d.input().metadata.title.as_deref(), Some("Example"));
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
    assert!(d.input().metadata.properties.is_empty());
    assert!(d.input().diagnostics.is_empty());
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
                assert_eq!(d.input().metadata.title.as_deref(), Some("private"));
            }
        }
    }
    // Only the conflicting top-level block falls back (CORE-003B4). Semantics and
    // activation outside it survive; intents, notes and headings inside it do not.
    let bad = "1. >\t- a [in](b.md) ![in](j.png)\n   # Inner\n\n   [^n]: note";
    let source = format!("# Heading\n\n[link](a.md) ![image](i.png)[^n]\n\n{bad}\n\n> after\n");
    let d = parse(&source);
    let fallback: Vec<_> = d
        .input()
        .nodes
        .iter()
        .filter(|n| matches!(&n.content, NodeKind::UnsupportedBlock { feature, .. } if feature == "unrepresentable_source_mapping"))
        .collect();
    assert_eq!(fallback.len(), 1);
    // Upstream list spans include their trailing blank lines.
    let exact = format!("{bad}\n\n");
    assert_eq!(fallback[0].span.slice(&source), Some(&exact[..]));
    assert!(d.input().roots.contains(&fallback[0].id));
    let destinations = |v: Vec<&String>| v.into_iter().cloned().collect::<Vec<_>>();
    assert_eq!(
        destinations(d.input().links.iter().map(|l| &l.destination).collect()),
        ["a.md"]
    );
    assert_eq!(
        destinations(d.input().resources.iter().map(|r| &r.destination).collect()),
        ["i.png"]
    );
    assert_eq!(d.input().outline.len(), 1);
    assert_eq!(d.input().outline[0].title, "Heading");
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::BlockQuote { .. })), 1);
    assert_eq!(
        kinds(&d, |k| matches!(k, NodeKind::FootnoteDefinition { .. })),
        0
    );
    // The reference outside lost its definition, so it reads as source.
    assert!(d.input().nodes.iter().any(|n| matches!(&n.content, NodeKind::UnsupportedInline { feature, .. } if feature == "footnote_reference")));
    assert!(!d.input().capabilities.contains(&Capability::Footnotes));
    let codes: Vec<_> = d
        .input()
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(
        codes,
        ["source_mapping_fallback", "footnote_reference_unresolved"]
    );
    assert_eq!(
        d.input().diagnostics[0].span.and_then(|s| s.slice(&source)),
        Some(&exact[..])
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
#[test]
fn v1_quote_continuation_destination_spans_point_at_the_lexeme() {
    // Corrected in CORE-003B2: these spans previously covered the `>` marker.
    let source = "> [a](\n> b)\n\n> ![i](\n> c.png)\n\n> [t](\n> d.md \"x\")\n";
    let d = parse(source);
    let links: Vec<_> = d
        .input()
        .links
        .iter()
        .map(|l| (l.span.start, l.span.end, l.span.slice(source).unwrap()))
        .collect();
    assert_eq!(links, vec![(9, 10, "b"), (40, 44, "d.md")]);
    assert_eq!(d.input().links[0].destination, "b");
    assert_eq!(d.input().resources[0].span.slice(source), Some("c.png"));
    assert_eq!(d.input().resources[0].destination, "c.png");
}

fn nodes(d: &Document, predicate: impl Fn(&NodeKind) -> bool) -> Vec<&Node> {
    d.input()
        .nodes
        .iter()
        .filter(|n| predicate(&n.content))
        .collect()
}
fn diagnostics<'a>(d: &'a Document, code: &str) -> Vec<&'a Diagnostic> {
    d.input()
        .diagnostics
        .iter()
        .filter(|x| x.code == code)
        .collect()
}
fn child<'a>(d: &'a Document, id: &NodeId) -> &'a Node {
    d.input().nodes.iter().find(|n| &n.id == id).unwrap()
}

#[test]
fn b3_alerts_are_semantic_and_look_alikes_stay_quotes() {
    let source = corpus("advanced/math-alerts.md");
    let d = parse(&source);
    let alerts = nodes(&d, |k| matches!(k, NodeKind::Alert { .. }));
    let categories: Vec<_> = alerts
        .iter()
        .map(|n| match n.content {
            NodeKind::Alert { category, .. } => category,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(categories, [AlertKind::Note, AlertKind::Warning]);
    assert_eq!(
        alerts[0].span.slice(&source),
        Some("> [!NOTE]\n> Useful information.\n")
    );
    let body = child(&d, &alerts[0].content.children()[0]);
    assert_eq!(body.span.slice(&source), Some("Useful information.\n"));
    let quote = nodes(&d, |k| matches!(k, NodeKind::BlockQuote { .. }));
    assert_eq!(quote.len(), 1);
    assert_eq!(
        quote[0].span.slice(&source),
        Some("> [!NOTE] not an alert\n")
    );
    assert!(d.input().capabilities.contains(&Capability::Alerts));

    // Case-insensitive markers, empty, nested, list-contained and unknown kinds.
    let source = "> [!note]\n> lower\n\n> [!TIP]\n\n> [!CAUTION]\n> > [!IMPORTANT]\n> > [x](\n> > a.md)\n\n- > [!WARNING]\n  > in list\n\n> [!UNKNOWN]\n> x\n";
    let d = parse(source);
    let alerts = nodes(&d, |k| matches!(k, NodeKind::Alert { .. }));
    assert_eq!(alerts.len(), 5);
    assert!(
        alerts
            .iter()
            .any(|n| matches!(&n.content, NodeKind::Alert { category: AlertKind::Tip, children } if children.is_empty()))
    );
    assert_eq!(
        nodes(&d, |k| matches!(k, NodeKind::BlockQuote { .. })).len(),
        1
    );
    // Destinations continued inside nested alert markers still map to the lexeme.
    assert_eq!(d.input().links[0].span.slice(source), Some("a.md"));
}

#[test]
fn b3_footnotes_resolve_case_insensitively_and_fail_closed() {
    let source = "A[^a] B[^A] C[^missing] D[^b] E[^c].\n\n[^a]: First.\n[^A]: Duplicate [x](dup.md).\n\n> [^b]: In quote\n\n- [^c]: In list\n";
    let d = parse(source);
    let definitions = nodes(&d, |k| matches!(k, NodeKind::FootnoteDefinition { .. }));
    let labels: Vec<_> = definitions
        .iter()
        .map(|n| match &n.content {
            NodeKind::FootnoteDefinition { label, .. } => label.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(labels, ["a", "b", "c"]);
    assert_eq!(definitions[0].span.slice(source), Some("[^a]: First.\n"));
    // Both references to `a` name the first definition's exact label.
    let references: Vec<_> = nodes(&d, |k| matches!(k, NodeKind::FootnoteReference { .. }))
        .iter()
        .map(|n| match &n.content {
            NodeKind::FootnoteReference { label } => (label.as_str(), n.span.slice(source)),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        references,
        [
            ("a", Some("[^a]")),
            ("a", Some("[^A]")),
            ("b", Some("[^b]")),
            ("c", Some("[^c]"))
        ]
    );
    // Undefined references stay ordinary text (GFM), never a dangling node.
    assert!(
        nodes(
            &d,
            |k| matches!(k, NodeKind::Text { text } if text == "^missing")
        )
        .len()
            == 1
    );
    // Later case-folded duplicates are inert source; their links are discarded.
    let duplicate = nodes(
        &d,
        |k| matches!(k, NodeKind::UnsupportedBlock { feature, reason: UnsupportedReason::InvalidInput } if feature == "footnote_definition"),
    );
    assert_eq!(duplicate.len(), 1);
    assert_eq!(
        duplicate[0].span.slice(source),
        Some("[^A]: Duplicate [x](dup.md).\n\n")
    );
    assert_eq!(diagnostics(&d, "footnote_definition_duplicate").len(), 1);
    assert!(d.input().links.is_empty());
    assert!(d.input().capabilities.contains(&Capability::Footnotes));

    // Only undefined references: no footnote capability or node.
    let d = parse("Text[^x] and [^y].\n");
    assert!(!d.input().capabilities.contains(&Capability::Footnotes));
    assert!(nodes(&d, |k| matches!(k, NodeKind::FootnoteReference { .. })).is_empty());

    // Headings drop the marker from titles; image alt keeps math readable. A
    // reference inside image brackets prevents the image upstream (like a link).
    let source = "# Title[^n]\n\n![alt $x$](i.png) ![b [^n]](j.png)\n\n[^n]: Note\n";
    let d = parse(source);
    assert_eq!(d.input().outline[0].title, "Title");
    assert_eq!(d.input().outline[0].anchor, "title");
    let images: Vec<_> = nodes(&d, |k| matches!(k, NodeKind::Image { .. }))
        .iter()
        .map(|n| match &n.content {
            NodeKind::Image { alt, .. } => alt.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(images, ["alt $x$"]);
    assert_eq!(d.input().resources.len(), 1);
    assert_eq!(
        nodes(&d, |k| matches!(k, NodeKind::FootnoteReference { .. })).len(),
        2
    );
}

#[test]
fn b3_math_is_explicit_source_fallback_with_exact_spans() {
    let source = corpus("advanced/math-alerts.md");
    let d = parse(&source);
    let math = |feature: &str| {
        nodes(&d, |k| matches!(k, NodeKind::UnsupportedInline { feature: f, reason: UnsupportedReason::Unavailable } if f == feature))
            .iter()
            .map(|n| n.span.slice(&source).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(math("inline_math"), ["$E=mc^2$", "$a^2 + b^2 = c^2$"]);
    assert_eq!(
        math("display_math"),
        ["$$\n\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}\n$$"]
    );
    assert_eq!(diagnostics(&d, "math_unavailable").len(), 3);
    // The frozen case: headings keep math source; unbalanced braces stay text.
    assert_eq!(d.input().outline[0].title, "Math $E=mc^2$");
    assert_eq!(d.input().outline[0].anchor, "math-emc2");
    assert!(
        nodes(
            &d,
            |k| matches!(k, NodeKind::Text { text } if text == " and an error $\\frac{1}{$ in one paragraph.")
        )
        .len()
            == 1
    );
    assert_eq!(
        nodes(
            &d,
            |k| matches!(k, NodeKind::UnsupportedBlock { feature, .. } if feature == "math")
        )
        .len(),
        1
    );

    // Prices, spaced and escaped dollars and code spans are not math; math is
    // opaque, so a link inside it is no activation intent.
    let source = "Costs $5 and $10. $ x $ \\$y$ `$c$` $[a](b.md)$ and $$d$$.\n\n| a | b |\n| - | - |\n| $p$ | q |\n";
    let d = parse(source);
    let spans: Vec<_> = nodes(&d, |k| matches!(k, NodeKind::UnsupportedInline { .. }))
        .iter()
        .map(|n| n.span.slice(source).unwrap())
        .collect();
    assert_eq!(spans, ["$[a](b.md)$", "$$d$$", "$p$"]);
    assert!(d.input().links.is_empty());
}

#[test]
fn b3_non_math_dollars_cost_no_extra_nodes() {
    // ENABLE_MATH splits text at each `$`; contiguous pieces are rejoined.
    let d = parse("Costs $5 and $10, or $7.50.\n");
    let text = nodes(&d, |k| matches!(k, NodeKind::Text { .. }));
    assert_eq!(text.len(), 1);
    assert!(
        matches!(&text[0].content, NodeKind::Text { text } if text == "Costs $5 and $10, or $7.50.")
    );
    assert_eq!(text[0].span, SourceSpan { start: 0, end: 27 });
    // An escaped dollar is not contiguous with its neighbour and keeps its split.
    let escaped = parse("a \\$x$ b\n");
    assert_eq!(
        kinds(&escaped, |k| matches!(
            k,
            NodeKind::UnsupportedInline { .. }
        )),
        0
    );
    // ~1.3 MiB of prices or shell variables stays at base node counts.
    for (line, lines, expected) in [
        ("Costs $5 and $10, or $7.50.\n", 50_000, 100_000),
        ("Run `x` with $HOME and $PATH set.\n", 40_000, 160_000),
    ] {
        assert_eq!(
            parse(&line.repeat(lines)).input().nodes.len(),
            expected,
            "{line:?}"
        );
    }
    // `$$$$` is empty display math, real syntax that spends nodes like any span.
    let runs = parse(&"$".repeat(11));
    assert_eq!(
        kinds(&runs, |k| matches!(k, NodeKind::UnsupportedInline { .. })),
        2
    );
    assert!(matches!(&runs.input().nodes[3].content, NodeKind::Text { text } if text == "$$$"));
}

#[test]
fn b3_diagram_fences_are_source_fallbacks_and_slugs_survive() {
    let source = corpus("advanced/inline-diagrams.md");
    let d = parse(&source);
    let fences: Vec<_> = nodes(&d, |k| matches!(k, NodeKind::UnsupportedBlock { .. }))
        .iter()
        .map(|n| match &n.content {
            NodeKind::UnsupportedBlock { feature, reason } => {
                assert_eq!(*reason, UnsupportedReason::Unavailable);
                assert!(n.span.slice(&source).unwrap().starts_with("```"));
                assert!(n.span.slice(&source).unwrap().ends_with("```"));
                feature.as_str()
            }
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(fences, ["mermaid", "mermaid", "d2", "d2"]);
    assert_eq!(diagnostics(&d, "advanced_fence_disabled").len(), 4);
    let anchors: Vec<_> = d
        .input()
        .outline
        .iter()
        .map(|o| o.anchor.as_str())
        .collect();
    assert_eq!(
        anchors,
        [
            "inline-diagrams",
            "slug-look-alikes",
            "mdv-mermaid-1",
            "dmdv-mermaid-2",
            "mdvmermaid-1",
            "dmdvmermaid-2"
        ]
    );
    // Fence names are exact, as in markdown-viewer; other spellings are code.
    let d = parse("```Mermaid\nx\n```\n\n~~~d2 extra\ny\n~~~\n");
    assert_eq!(kinds(&d, |k| matches!(k, NodeKind::CodeBlock { .. })), 1);
    assert_eq!(
        kinds(
            &d,
            |k| matches!(k, NodeKind::UnsupportedBlock { feature, .. } if feature == "d2")
        ),
        1
    );
}

#[test]
fn b3_voice_script_opt_in_is_diagnosed_and_syntax_stays_text() {
    let source = corpus("advanced/voice-parity.md");
    let d = parse(&source);
    let voice = diagnostics(&d, "voice_script_unavailable");
    assert_eq!(voice.len(), 1);
    assert_eq!(voice[0].severity, Severity::Info);
    assert_eq!(voice[0].node, None);
    assert_eq!(voice[0].span, d.input().metadata.front_matter_span);
    assert!(
        voice[0]
            .span
            .unwrap()
            .slice(&source)
            .unwrap()
            .contains("mdv: script")
    );
    // Directions, notes and pronunciation guides remain readable text.
    assert!(
        nodes(
            &d,
            |k| matches!(k, NodeKind::Text { text } if text.contains("pecan|PEE-can"))
        )
        .len()
            == 1
    );
    assert!(d.input().links.is_empty());

    for (source, expected) in [
        ("---\nmdv: notes\n---\n\n**ANNA:** Hi [beat].\n", 0),
        ("**ANNA:** Hi [beat].\n\nmdv: script\n", 0),
        // Typed properties omitted: the opt-in line scan still applies.
        (
            "---\nmdv: script\nnested:\n  key: value\n---\n\nText [beat].\n",
            1,
        ),
        ("---\nmdv: script\nmdv: script\n---\n\nText.\n", 1),
        // The diagnostic survives the inert-body source-mapping fallback.
        ("---\nmdv: script\n---\n1. >\t- a", 1),
    ] {
        let d = parse(source);
        assert_eq!(
            diagnostics(&d, "voice_script_unavailable").len(),
            expected,
            "{source:?}"
        );
    }
}

#[test]
fn b3_bounded_generated_extension_markup_never_publishes_an_invalid_model() {
    let tokens = [
        "$",
        "$$",
        "[^a]",
        "[^A]",
        "[^a]: ",
        "> [!NOTE]\n",
        "> ",
        "[!TIP]",
        "\n",
        "\n\n",
        "- ",
        "    ",
        "\t",
        "x",
        "{",
        "}",
        "\\",
        "`",
        "[l](u.md)",
        "|",
        "| - |\n",
        "*",
        "é",
        "```math\n",
        "```\n",
    ];
    let mut seed = 0x2545f4914f6cdd1du64;
    for case in 0..20_000 {
        let mut source = String::new();
        for _ in 0..(case % 24 + 1) {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            source.push_str(tokens[(seed >> 32) as usize % tokens.len()]);
        }
        let ids = || (DocumentId("d".into()), RevisionId("r".into()));
        let (doc, rev) = ids();
        match parse_markdown(&source, doc, rev, ParseLimits::default()) {
            Ok(d) => {
                Document::new(d.into_input(), &source, ModelLimits::default()).unwrap();
            }
            Err(error) => panic!("case {case}: {source:?}: {error:?}"),
        }
        #[cfg(feature = "model-v2")]
        {
            let (doc, rev) = ids();
            if let Err(error) = parse_markdown_v2(&source, doc, rev, ParseLimits::default()) {
                panic!("v2 case {case}: {source:?}: {error:?}");
            }
        }
    }
}
