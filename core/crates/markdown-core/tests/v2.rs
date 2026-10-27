#![cfg(feature = "model-v2")]
use markdown_core::{
    v2::{self, IntentSource, Target},
    *,
};
fn parse(s: &str) -> v2::Document {
    parse_markdown_v2(
        s,
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap_or_else(|e| panic!("{s:?}: {e:?}"))
}
#[test]
fn reference_forms_images_casefold_and_repeated_uses_have_exact_provenance() {
    let s = "[one][ID] [id][] [ID] ![alt][id]\n\n[id]: <docs/café%20one.md> \"title\"\n";
    let d = parse(s);
    assert_eq!(d.input().links.len(), 3);
    assert_eq!(d.input().resources.len(), 1);
    for l in &d.input().links {
        let IntentSource::Reference { definition, span } = l.source else {
            panic!("missing reference")
        };
        assert_eq!(span.slice(s), Some("docs/café%20one.md"));
        assert!(definition.slice(s).unwrap().contains("[id]:"));
        assert_eq!(
            l.target,
            Target::RequiresCapability {
                path: "docs/café one.md".into(),
                fragment: None
            }
        );
    }
    assert!(matches!(
        d.input().resources[0].source,
        IntentSource::Reference { .. }
    ));
    assert_eq!(
        d.input().resources[0].target,
        Target::RequiresCapability {
            path: "docs/café one.md".into(),
            fragment: None
        }
    );
    let legacy = parse_markdown(
        s,
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap();
    assert!(legacy.input().links.is_empty() && legacy.input().resources.is_empty());
}
#[test]
fn inline_unicode_percent_fragments_and_external_urls_are_typed() {
    let s = "[local](<docs/café one.md#café>) [parent](../other.md) [web](https://例え.test/a%20b) [mail](mailto:a@example.com) [jump](#caf%C3%A9) ![image](<images/a b.png>)";
    let d = parse(s);
    assert_eq!(
        d.input().links[0].target,
        Target::RequiresCapability {
            path: "docs/café one.md".into(),
            fragment: Some("café".into())
        }
    );
    assert_eq!(
        d.input().links[1].target,
        Target::RequiresCapability {
            path: "../other.md".into(),
            fragment: None
        }
    );
    assert!(
        matches!(&d.input().links[2].target,Target::External{uri} if uri=="https://xn--r8jz45g.test/a%20b")
    );
    assert!(
        matches!(&d.input().links[3].target,Target::External{uri} if uri=="mailto:a@example.com")
    );
    assert_eq!(
        d.input().links[4].target,
        Target::Fragment {
            anchor: "café".into()
        }
    );
    assert_eq!(
        d.input().resources[0].target,
        Target::RequiresCapability {
            path: "images/a b.png".into(),
            fragment: None
        }
    );
}
#[test]
fn unsafe_and_obfuscated_uris_never_get_actionable_targets() {
    for s in [
        "javascript:alert(1)",
        "data:text/html,x",
        "file:///tmp/x",
        "//host/x",
        "/etc/passwd",
        "C:/x",
        "x:stream",
        "a\\b",
        "%2fetc/passwd",
        "%2E%2E/x",
        "a%2fb",
        "x%5cy",
        "a%00b",
        "%ff",
        "a%",
        "https://a/%0d%0a",
        "https://x\u{202e}y",
        "docs/a\u{200b}.md",
        "https://x/%E2%80%8B",
        "http:example.com",
        "https://user:pass@example.com",
        "https://[:::1]",
        "mailto:a@example.com?subject=x",
        "CON",
        "a/NUL.txt",
        "a/COM1",
        "a/file. ",
    ] {
        assert!(
            matches!(v2::classify(s, false), Target::Denied { .. }),
            "{s:?}: {:?}",
            v2::classify(s, false)
        );
    }
    assert!(matches!(
        v2::classify("https://example.com/a.png", true),
        Target::Denied { .. }
    ));
    assert!(matches!(
        v2::classify(&"a".repeat(8193), false),
        Target::Denied {
            reason: v2::Denial::Limit
        }
    ));
    assert_eq!(
        v2::classify("literal%252e.txt", false),
        Target::RequiresCapability {
            path: "literal%2e.txt".into(),
            fragment: None
        }
    );
}
#[test]
fn v1_wire_remains_strict_and_explicit_upgrade_keeps_denials() {
    let s = "[ok](a.md) [denied](<a b.md>)";
    let old = parse_markdown(
        s,
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let old_bytes = old.to_json().unwrap();
    assert!(v2::Document::from_json(&old_bytes, s, ModelLimits::default()).is_err());
    let upgraded = v2::Document::from_v1(old, s, ModelLimits::default()).unwrap();
    assert!(matches!(
        upgraded.input().links[1].target,
        Target::Denied {
            reason: v2::Denial::LegacyPolicy
        }
    ));
    let bytes = upgraded.to_json().unwrap();
    assert!(Document::from_json(&bytes, s, ModelLimits::default()).is_err());
    assert_eq!(
        v2::Document::from_json(&bytes, s, ModelLimits::default()).unwrap(),
        upgraded
    );
}
#[test]
fn typed_and_wire_validation_reject_invalid_provenance_and_forged_targets() {
    let s = "[x][id]\n\n[id]: safe.md\n";
    let d = parse(s);
    let mut input = d.clone().into_input();
    input.links[0].source = IntentSource::Inline {
        span: input.links[0].source.span(),
    };
    assert_eq!(
        v2::Document::new(input, s, ModelLimits::default())
            .unwrap_err()
            .code,
        "intent_span"
    );
    let mut input = d.clone().into_input();
    input.links[0].target = Target::External {
        uri: "javascript:x".into(),
    };
    assert_eq!(
        v2::Document::new(input, s, ModelLimits::default())
            .unwrap_err()
            .code,
        "link_policy"
    );
    let mut input = d.clone().into_input();
    input.links[0].source = IntentSource::Reference {
        definition: SourceSpan {
            start: 0,
            end: s.len() as u64,
        },
        span: input.links[0].source.span(),
    };
    assert_eq!(
        v2::Document::new(input, s, ModelLimits::default())
            .unwrap_err()
            .code,
        "reference_span"
    );
    let mut value = serde_json::to_value(d.input()).unwrap();
    value["links"][0]["source"]["junk"] = serde_json::json!(1);
    assert_eq!(
        v2::Document::from_json(
            &serde_json::to_vec(&value).unwrap(),
            s,
            ModelLimits::default()
        )
        .unwrap_err()
        .code,
        "invalid_json"
    );
}
#[test]
fn references_handle_multiline_definitions_duplicates_escaped_labels_and_crlf() {
    for s in [
        "[x][id]\r\n\r\n[id]:\r\n <a%20b.md>\r\n \"title\"\r\n",
        "[x][i\\]d]\n\n[i\\]d]: a.md\n",
        "[x][ID]\n\n[id]: first.md\n[ID]: second.md\n",
        "[x][Straße]\n\n[STRASSE]: a.md\n",
        "[<http://a>](b)",
        "[![i][id]](outer.md)\n\n[id]: i.png\n",
    ] {
        parse(s);
    }
    let d = parse("[missing]\n\n[id]: a.md\n");
    assert!(d.input().links.is_empty());
}
#[test]
fn fallback_keeps_metadata_discards_targets_and_budgets_still_apply() {
    let s = "---\ntitle: Example\n---\n[ref][id]\n\n1. >\t- a\n\n[id]: a.md\n";
    let d = parse(s);
    assert_eq!(d.input().metadata.title.as_deref(), Some("Example"));
    assert!(d.input().links.is_empty());
    assert!(
        d.input()
            .diagnostics
            .iter()
            .any(|d| d.code == "source_mapping_fallback")
    );
    let e = parse_markdown_v2(
        "[x](a)",
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits {
            max_text_bytes: 2,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(e, ParseError::TextLimit);
}
#[test]
fn deterministic_generated_inputs_validate_or_hit_explicit_limits() {
    let tokens = [
        "[x][id]",
        "[id]: a%20b.md\n",
        "[",
        "\\]",
        "]:",
        "<https://x>",
        "\n",
        ">\t- ",
        "é",
        "[id]: <café.md>\n",
        "![x][id]",
        "---",
        "*",
        "[id][]",
    ];
    let mut seed = 19u64;
    for case in 0..8192 {
        let mut s = String::new();
        for _ in 0..case % 16 + 1 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            s.push_str(tokens[(seed >> 32) as usize % tokens.len()]);
        }
        match parse_markdown_v2(
            &s,
            DocumentId("d".into()),
            RevisionId("r".into()),
            ParseLimits::default(),
        ) {
            Ok(d) => {
                v2::Document::new(d.into_input(), &s, ModelLimits::default()).unwrap();
            }
            Err(
                ParseError::SourceLimit
                | ParseError::NodeLimit
                | ParseError::DepthLimit
                | ParseError::EventLimit
                | ParseError::TextLimit,
            ) => {}
            Err(e) => panic!("{case} {s:?}: {e:?}"),
        }
    }
}

#[test]
fn authored_v2_golden_roundtrips_and_v1_goldens_upgrade_explicitly() {
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../specs/semantic-model");
    let s = std::fs::read_to_string(root.join("v2/fixtures/references.source.txt")).unwrap();
    let d = parse_markdown_v2(
        &s,
        DocumentId("example".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("v2/fixtures/references.json")).unwrap())
            .unwrap();
    assert_eq!(serde_json::to_value(d.input()).unwrap(), expected);
    assert_eq!(
        v2::Document::from_json(&d.to_json().unwrap(), &s, ModelLimits::default()).unwrap(),
        d
    );
    for name in ["unicode", "gfm", "resources", "containers", "fallback"] {
        let s =
            std::fs::read_to_string(root.join(format!("v1/fixtures/{name}.source.txt"))).unwrap();
        let b = std::fs::read(root.join(format!("v1/fixtures/{name}.json"))).unwrap();
        let old = Document::from_json(&b, &s, ModelLimits::default()).unwrap();
        let new = v2::Document::from_v1(old.clone(), &s, ModelLimits::default()).unwrap();
        assert_eq!(new.input().nodes, old.input().nodes);
        assert_eq!(new.input().source, old.input().source);
        assert_eq!(new.input().metadata, old.input().metadata);
    }
}
#[cfg(feature = "schema")]
#[test]
fn frozen_v2_schema_matches_types() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../specs/semantic-model/v2/document.schema.json");
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(v2::document_schema()).unwrap(),
        expected
    );
}

#[test]
fn wire_cannot_forge_destinations_or_reference_definition_bindings() {
    let s = "[x](safe.md)";
    let d = parse(s);
    let mut input = d.into_input();
    input.links[0].destination = "other.md".into();
    input.links[0].target = Target::RequiresCapability {
        path: "other.md".into(),
        fragment: None,
    };
    assert_eq!(
        v2::Document::new(input.clone(), s, ModelLimits::default())
            .unwrap_err()
            .code,
        "source_provenance"
    );
    assert_eq!(
        v2::Document::from_json(
            &serde_json::to_vec(&input).unwrap(),
            s,
            ModelLimits::default()
        )
        .unwrap_err()
        .code,
        "source_provenance"
    );
    let s = "[x][one]\n\n[one]: same.md\n[two]: same.md\n";
    let mut input = parse(s).into_input();
    let start = s.find("[two]:").unwrap();
    let dest = s[start..].find("same.md").unwrap() + start;
    input.links[0].source = IntentSource::Reference {
        definition: SourceSpan {
            start: start as u64,
            end: (s.len() - 1) as u64,
        },
        span: SourceSpan {
            start: dest as u64,
            end: (dest + 7) as u64,
        },
    };
    assert_eq!(
        v2::Document::new(input, s, ModelLimits::default())
            .unwrap_err()
            .code,
        "source_provenance"
    );
}
#[test]
fn quoted_multiline_destinations_have_exact_source_lexemes() {
    for s in [
        "> [id]:\n>   <docs/a.md>\n>\n> [x][id]\n",
        "> > [id]:\n> >   <docs/a&amp;b.md>\n> >\n> > [x][id]\n",
        "> [x](\n>   <docs/a.md>\n> )\n",
        "> ![x](\n>   <docs/a.md>\n> )\n",
        "> > [id]:\n> >   <docs/a.md>\n> >\n> > ![x][id]\n",
        "1. > [id]:\n   >   <docs/a.md>\n   >\n   > [x][id]\n",
    ] {
        let d = parse(s);
        for l in &d.input().links {
            assert!(
                matches!(
                    l.source.span().slice(s),
                    Some("docs/a.md" | "docs/a&amp;b.md")
                ),
                "{s:?}"
            );
        }
        for r in &d.input().resources {
            assert_eq!(r.source.span().slice(s), Some("docs/a.md"));
        }
    }
    let s = "> [x](\n>   <docs/a.md>\n> )\n";
    let old = parse_markdown(
        s,
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap();
    assert_eq!(old.input().links[0].span.slice(s), Some("docs/a.md"));
}
#[test]
fn strict_scheme_and_windows_device_guards_preserve_legitimate_fragments() {
    assert_eq!(
        v2::classify("doc.md#café:one", false),
        Target::RequiresCapability {
            path: "doc.md".into(),
            fragment: Some("café:one".into())
        }
    );
    for raw in [
        "https:///example.com",
        "https:////example.com",
        "COM¹.txt",
        "LPT²",
        "a/COM³",
        "COM%C2%B9.txt",
        "CON .txt",
        "CONIN$",
        "CONOUT$",
    ] {
        assert!(
            matches!(v2::classify(raw, false), Target::Denied { .. }),
            "{raw}"
        );
    }
}
#[test]
fn captured_event_budget_is_independent_of_published_node_count() {
    let s = format!("![{}](a.png)", "*x* ".repeat(1200));
    let limits = ParseLimits {
        model: ModelLimits {
            max_nodes: 2,
            ..ModelLimits::default()
        },
        ..ParseLimits::default()
    };
    let d = parse_markdown_v2(&s, DocumentId("d".into()), RevisionId("r".into()), limits).unwrap();
    assert_eq!(d.input().nodes.len(), 2);
    assert_eq!(
        parse_markdown_v2(
            &s,
            DocumentId("d".into()),
            RevisionId("r".into()),
            ParseLimits {
                max_events: 100,
                ..limits
            }
        )
        .unwrap_err(),
        ParseError::EventLimit
    );
}
