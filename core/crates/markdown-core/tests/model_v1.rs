use markdown_core::*;
use serde_json::{Value, json};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../specs/semantic-model/v1/fixtures")
}
fn input(name: &str) -> (DocumentInput, String) {
    let base = fixtures();
    let source = std::fs::read_to_string(base.join(format!("{name}.source.txt"))).unwrap();
    let input =
        serde_json::from_slice(&std::fs::read(base.join(format!("{name}.json"))).unwrap()).unwrap();
    (input, source)
}
fn validate(input: DocumentInput, source: &str) -> Result<Document, ModelError> {
    Document::new(input, source, ModelLimits::default())
}
fn rejected(input: DocumentInput, source: &str, code: &str) {
    assert_eq!(validate(input, source).unwrap_err().code, code);
}

#[test]
fn all_goldens_roundtrip_owned_values_and_preserve_source_spans() {
    for name in ["unicode", "gfm", "resources", "containers", "fallback"] {
        let (input, source) = input(name);
        let snapshot = validate(input.clone(), &source).unwrap_or_else(|e| panic!("{name}: {e}"));
        let bytes = snapshot.to_json().unwrap();
        let roundtrip = Document::from_json(&bytes, &source, ModelLimits::default()).unwrap();
        assert_eq!(roundtrip, snapshot);
        let expected: Value = serde_json::from_slice(
            &std::fs::read(fixtures().join(format!("{name}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
        for node in snapshot.input().nodes.iter() {
            assert!(node.span.slice(&source).is_some());
        }
    }
}

#[test]
fn utf8_nfc_nfd_crlf_and_half_open_spans_are_exact() {
    let (d, source) = input("unicode");
    assert!(source.contains("\r\n"));
    let nfc = &d.nodes.iter().find(|n| n.id.0 == "ht").unwrap().span;
    let nfd = &d.nodes.iter().find(|n| n.id.0 == "bt").unwrap().span;
    assert_eq!(nfc.slice(&source), Some("Café"));
    assert_eq!(nfd.slice(&source), Some("cafe\u{301}"));
    assert_eq!(nfc.end - nfc.start, 5);
    assert_eq!(nfd.end - nfd.start, 6);
    assert_eq!(SourceSpan { start: 0, end: 0 }.slice(&source), Some(""));
    assert_eq!(
        SourceSpan {
            start: u64::MAX,
            end: u64::MAX
        }
        .slice(&source),
        None
    );
    let mut bad = d.clone();
    bad.nodes[1].span.end -= 1; // Middle of the two-byte NFC character.
    rejected(bad, &source, "invalid_span");
    let mut bad = d;
    bad.nodes[0].span.end = u64::MAX;
    rejected(bad, &source, "invalid_span");
}

#[test]
fn source_revision_binding_and_versions_are_enforced() {
    let (d, source) = input("unicode");
    rejected(
        d.clone(),
        &source.replace("Hello", "Other"),
        "source_mismatch",
    );
    let mut bad = d.clone();
    bad.source.bytes += 1;
    rejected(bad, &source, "source_mismatch");
    let mut bad = d;
    bad.model_version = 2;
    rejected(bad, &source, "model_version");
}

#[test]
fn stable_ids_are_not_offsets_and_are_scoped_to_document_lineage() {
    let (first, source) = input("unicode");
    let mut second = first.clone();
    let changed = format!("\n{source}");
    second.revision_id = RevisionId("rev-2".into());
    second.source = SourceInfo::of(&changed);
    for node in &mut second.nodes {
        node.span.start += 1;
        node.span.end += 1;
    }
    let before = validate(first, &source).unwrap();
    let after = validate(second, &changed).unwrap();
    assert_eq!(before.input().document_id, after.input().document_id);
    assert_eq!(before.input().nodes[0].id, after.input().nodes[0].id);
    assert_ne!(before.input().nodes[0].span, after.input().nodes[0].span);
    // Identity reconciliation itself belongs to CORE-004A; this tests the contract.
}

#[test]
fn duplicate_invalid_and_dangling_ids_are_rejected() {
    let (d, s) = input("unicode");
    let mut bad = d.clone();
    bad.nodes[1].id = bad.nodes[0].id.clone();
    rejected(bad, &s, "duplicate_id");
    let mut bad = d.clone();
    bad.document_id.0 = "/private/file.md".into();
    rejected(bad, &s, "invalid_id");
    let mut bad = d;
    bad.roots[0] = NodeId("missing".into());
    rejected(bad, &s, "unknown_node");
}

#[test]
fn invalid_tree_shapes_order_cycles_and_orphans_are_rejected() {
    let (d, s) = input("unicode");
    let mut bad = d.clone();
    bad.roots.reverse();
    rejected(bad, &s, "sibling_order");
    let mut bad = d.clone();
    bad.roots = vec![NodeId("ht".into())];
    rejected(bad, &s, "root_kind");
    let mut bad = d.clone();
    bad.nodes[0].content = NodeKind::Paragraph {
        children: vec![NodeId("p".into())],
    };
    assert!(validate(bad, &s).is_err());
    let mut bad = d.clone();
    bad.roots.pop();
    rejected(bad, &s, "parent_count");
    let mut bad = d;
    bad.nodes[0].content = NodeKind::BlockQuote {
        children: vec![NodeId("h".into())],
    };
    assert!(validate(bad, &s).is_err());
}

#[test]
fn outline_and_capabilities_cannot_lie_about_the_tree() {
    let (d, s) = input("unicode");
    let mut bad = d.clone();
    bad.outline.clear();
    rejected(bad, &s, "outline_coverage");
    let mut bad = d.clone();
    bad.outline[0].level = 6;
    rejected(bad, &s, "outline_mismatch");
    let mut bad = d;
    bad.capabilities.insert(Capability::Tables);
    rejected(bad, &s, "capabilities");
    let (mut bad, s) = input("gfm");
    bad.capabilities.remove(&Capability::Tables);
    rejected(bad, &s, "capabilities");
}

#[test]
fn table_shape_and_footnote_targets_are_checked() {
    let (d, s) = input("gfm");
    let mut bad = d.clone();
    if let NodeKind::Table { alignments, .. } = &mut bad
        .nodes
        .iter_mut()
        .find(|n| n.id.0 == "table")
        .unwrap()
        .content
    {
        alignments.pop();
    }
    rejected(bad, &s, "child_kind");
    let mut bad = d;
    if let NodeKind::FootnoteReference { label } = &mut bad
        .nodes
        .iter_mut()
        .find(|n| n.id.0 == "ref")
        .unwrap()
        .content
    {
        *label = "missing".into();
    }
    rejected(bad, &s, "unknown_footnote");
}

#[test]
fn typed_intents_do_not_grant_filesystem_or_remote_image_access() {
    let (d, s) = input("resources");
    // The v1 producer denies parent traversal and non-ASCII paths (CORE-002B).
    assert_eq!(d.links[0].destination, "../café.md#part");
    assert!(matches!(
        d.links[0].disposition,
        LinkDisposition::Denied { .. }
    ));
    let mut bad = d.clone();
    bad.links[0].disposition = LinkDisposition::RequiresCapability {};
    rejected(bad, &s, "link_policy");
    let mut bad = d.clone();
    bad.links[2].disposition = LinkDisposition::External {};
    rejected(bad, &s, "link_policy");
    let mut bad = d.clone();
    bad.resources[0].destination = "https://example.invalid/a.png".into();
    rejected(bad, &s, "resource_policy");
    for destination in [
        "/etc/file",
        "C:\\private",
        "file:private",
        "//server/file",
        "x\nsecret",
    ] {
        let mut bad = d.clone();
        bad.resources[0].destination = destination.into();
        rejected(bad, &s, "resource_policy");
    }
    let mut bad = d;
    bad.resources[0].id = ResourceId("missing".into());
    rejected(bad, &s, "unknown_resource");
}

#[test]
fn metadata_diagnostics_and_unsupported_fallbacks_preserve_contracts() {
    let (d, s) = input("resources");
    let diagram = d.nodes.iter().find(|n| n.id.0 == "diagram").unwrap();
    assert_eq!(
        diagram.span.slice(&s),
        Some("```mermaid\ngraph TD\n A-->B\n```")
    );
    let mut bad = d.clone();
    bad.metadata.front_matter_span.as_mut().unwrap().start = 1;
    rejected(bad, &s, "front_matter_span");
    let mut bad = d.clone();
    bad.diagnostics[0].node = Some(NodeId("missing".into()));
    rejected(bad, &s, "unknown_node");
    let mut bad = d;
    bad.metadata.front_matter_span = None;
    rejected(bad, &s, "metadata_without_front_matter");
}

#[test]
fn untrusted_wire_rejects_unknown_fields_kinds_and_duplicate_capabilities() {
    let (d, s) = input("containers");
    let original = serde_json::to_value(d).unwrap();
    let mut unknown_root = original.clone();
    unknown_root["unknown"] = json!(true);
    assert_eq!(
        Document::from_json(
            &serde_json::to_vec(&unknown_root).unwrap(),
            &s,
            ModelLimits::default()
        )
        .unwrap_err()
        .code,
        "invalid_json"
    );
    for mutate in 0..7 {
        let mut value = original.clone();
        match mutate {
            0 => value["nodes"][0]["content"]["kind"] = json!("future_kind"),
            1 => value["nodes"][0]["content"]["unknown"] = json!(true),
            2 => value["capabilities"] = json!(["alerts", "alerts", "strikethrough"]),
            3 => {
                let node = value["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|n| n["id"] == "rule")
                    .unwrap();
                node["content"]["unknown"] = json!(true);
            }
            4 => value["links"][0]["disposition"] = json!({"kind": "external", "junk": 1}),
            5 => {
                value["links"][0]["disposition"] = json!({"kind": "requires_capability", "junk": 1})
            }
            _ => {
                value["resources"] = json!([{
                    "id": "extra", "span": {"start": 0, "end": 0},
                    "destination": "image.png",
                    "access": {"kind": "requires_capability", "junk": 1}
                }]);
            }
        }
        assert_eq!(
            Document::from_json(
                &serde_json::to_vec(&value).unwrap(),
                &s,
                ModelLimits::default()
            )
            .unwrap_err()
            .code,
            "invalid_json",
            "case {mutate}"
        );
    }
}

#[test]
fn bounds_are_explicit_and_errors_do_not_echo_document_contents() {
    let (d, s) = input("unicode");
    let limits = ModelLimits {
        max_nodes: 1,
        ..ModelLimits::default()
    };
    assert_eq!(
        Document::new(d.clone(), &s, limits).unwrap_err().code,
        "node_limit"
    );
    let limits = ModelLimits {
        max_depth: 1,
        ..ModelLimits::default()
    };
    assert_eq!(
        Document::new(d.clone(), &s, limits).unwrap_err().code,
        "depth_limit"
    );
    let limits = ModelLimits {
        max_wire_bytes: 1,
        ..ModelLimits::default()
    };
    assert_eq!(
        Document::from_json(b"{}", &s, limits).unwrap_err().code,
        "wire_limit"
    );
    let error = Document::from_json(b"{SECRET DOCUMENT}", &s, ModelLimits::default()).unwrap_err();
    assert!(!error.to_string().contains("SECRET"));
}

#[cfg(feature = "schema")]
#[test]
fn checked_in_schema_matches_the_rust_wire_types() {
    let generated = serde_json::to_value(markdown_core::document_schema()).unwrap();
    let path = fixtures().join("../document.schema.json");
    let checked: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(generated, checked);
}

#[test]
fn duplicate_metadata_keys_and_nested_link_nodes_are_rejected() {
    let (d, s) = input("resources");
    let bytes = serde_json::to_string(&d).unwrap();
    let duplicate = bytes.replacen(
        "\"properties\":{",
        "\"properties\":{\"count\":{\"kind\":\"integer\",\"value\":99},",
        1,
    );
    assert_eq!(
        Document::from_json(duplicate.as_bytes(), &s, ModelLimits::default())
            .unwrap_err()
            .code,
        "invalid_json"
    );
    let mut bad = d;
    let text = bad.nodes.iter_mut().find(|n| n.id.0 == "localt").unwrap();
    text.content = NodeKind::Link {
        target: LinkId("link-local".into()),
        title: None,
        children: vec![],
    };
    rejected(bad, &s, "nested_link");
}

#[test]
fn disconnected_cycles_and_deep_flat_graphs_fail_without_recursive_walks() {
    let (mut d, s) = input("unicode");
    d.outline.clear();
    d.roots.clear();
    let span = SourceSpan { start: 0, end: 1 };
    d.nodes = vec![
        Node {
            id: NodeId("a".into()),
            span,
            content: NodeKind::BlockQuote {
                children: vec![NodeId("b".into())],
            },
        },
        Node {
            id: NodeId("b".into()),
            span,
            content: NodeKind::BlockQuote {
                children: vec![NodeId("a".into())],
            },
        },
    ];
    rejected(d.clone(), &s, "unreachable_node");
    d.roots = vec![NodeId("n0".into())];
    d.nodes = (0..2000)
        .map(|i| Node {
            id: NodeId(format!("n{i}")),
            span,
            content: if i == 1999 {
                NodeKind::Paragraph { children: vec![] }
            } else {
                NodeKind::BlockQuote {
                    children: vec![NodeId(format!("n{}", i + 1))],
                }
            },
        })
        .collect();
    let limits = ModelLimits {
        max_depth: 64,
        ..ModelLimits::default()
    };
    assert_eq!(
        Document::new(d, &s, limits).unwrap_err().code,
        "depth_limit"
    );
}

#[test]
fn empty_document_and_an_unknown_feature_fallback_are_valid() {
    let (mut d, _) = input("unicode");
    d.source = SourceInfo::of("");
    d.roots.clear();
    d.nodes.clear();
    d.outline.clear();
    assert!(validate(d, "").is_ok());
    let (mut d, s) = input("fallback");
    if let NodeKind::UnsupportedInline { feature, .. } = &mut d
        .nodes
        .iter_mut()
        .find(|n| n.id.0 == "math")
        .unwrap()
        .content
    {
        *feature = "future-widget".into();
    }
    assert!(validate(d, &s).is_ok());
}

#[test]
fn source_budget_is_checked_before_source_fingerprint_work() {
    let (d, s) = input("unicode");
    let limits = ModelLimits {
        max_source_bytes: 1,
        ..ModelLimits::default()
    };
    assert_eq!(
        Document::new(d, &s, limits).unwrap_err().code,
        "source_limit"
    );
}

#[test]
fn null_metadata_is_strict_and_errors_keep_untrusted_fields_private() {
    let (d, s) = input("resources");
    let original = serde_json::to_value(d).unwrap();
    for field in ["value", "SECRET_FIELD"] {
        let mut value = original.clone();
        value["metadata"]["properties"]["missing"][field] = serde_json::Value::Null;
        let error = Document::from_json(
            &serde_json::to_vec(&value).unwrap(),
            &s,
            ModelLimits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_json");
        assert!(!error.to_string().contains("SECRET"));
    }
}

fn parsed(source: &str) -> DocumentInput {
    parse_markdown(
        source,
        DocumentId("d".into()),
        RevisionId("r".into()),
        ParseLimits::default(),
    )
    .unwrap()
    .into_input()
}
fn imported(input: &DocumentInput, source: &str) -> Result<Document, ModelError> {
    Document::from_json(
        &serde_json::to_vec(input).unwrap(),
        source,
        ModelLimits::default(),
    )
}

#[test]
fn imported_intents_must_match_the_parser_policy() {
    let s = "[go](safe.md) ![i](img.png)\n";
    let d = parsed(s);
    assert!(imported(&d, s).is_ok());
    for destination in [
        "../../../etc/passwd",
        "%2e%2e/%2e%2e/x",
        "a b.md",
        "\u{202e}gpj.md",
        "https://example.invalid/",
        "#top",
    ] {
        let mut bad = d.clone();
        bad.links[0].destination = destination.into();
        assert_eq!(
            imported(&bad, s).unwrap_err().code,
            "link_policy",
            "{destination:?}"
        );
        let mut bad = d.clone();
        bad.resources[0].destination = destination.into();
        assert_eq!(
            imported(&bad, s).unwrap_err().code,
            "resource_policy",
            "{destination:?}"
        );
    }
    let mut bad = d.clone();
    bad.links[0].destination = "https://example.invalid/".into();
    bad.links[0].disposition = LinkDisposition::External {};
    assert!(imported(&bad, s).is_ok());
    // Narrowing to Denied stays allowed, as in v2.
    let mut narrowed = d;
    narrowed.links[0].disposition = LinkDisposition::Denied {
        reason: "host_policy".into(),
    };
    assert!(imported(&narrowed, s).is_ok());
}

#[test]
fn v1_userinfo_urls_round_trip_as_the_parser_emits_them() {
    // The v1 producer classifies credential URLs as External; v2 denies them (D-025).
    let s = "[x](https://user:pw@evil.example/)\n";
    let d = parsed(s);
    assert_eq!(d.links[0].disposition, LinkDisposition::External {});
    assert_eq!(imported(&d, s).unwrap().into_input(), d);
}

#[test]
fn imported_metadata_text_keeps_the_front_matter_display_guard() {
    let s = "---\ntitle: Safe\ntags:\n  - a\n---\n[go](safe.md)\n";
    let d = parsed(s);
    assert!(imported(&d, s).is_ok());
    let rlo = "abc\u{202e}txt".to_string();
    let mut bad = d.clone();
    bad.metadata.title = Some(rlo.clone());
    assert_eq!(imported(&bad, s).unwrap_err().code, "metadata_text");
    let mut bad = d.clone();
    bad.metadata
        .properties
        .insert("title".into(), MetadataValue::String { value: rlo.clone() });
    assert_eq!(imported(&bad, s).unwrap_err().code, "metadata_text");
    let mut bad = d.clone();
    bad.metadata.properties.insert(
        "tags".into(),
        MetadataValue::Strings {
            value: vec!["a".into(), rlo],
        },
    );
    assert_eq!(imported(&bad, s).unwrap_err().code, "metadata_text");
    let mut bad = d.clone();
    bad.metadata
        .properties
        .insert("x\u{200b}".into(), MetadataValue::Null {});
    assert_eq!(imported(&bad, s).unwrap_err().code, "metadata_key");
    // Blank default-ignorable values that are not Cf (CORE-003E, D-032).
    for c in [
        '\u{115f}',
        '\u{1160}',
        '\u{17b4}',
        '\u{2065}',
        '\u{2800}',
        '\u{3164}',
        '\u{ffa0}',
        '\u{e0002}',
    ] {
        let blank = format!("a{c}b");
        let mut bad = d.clone();
        bad.metadata.title = Some(blank.clone());
        assert_eq!(imported(&bad, s).unwrap_err().code, "metadata_text");
        let mut bad = d.clone();
        bad.metadata.properties.insert(
            "tags".into(),
            MetadataValue::Strings {
                value: vec!["a".into(), blank],
            },
        );
        assert_eq!(imported(&bad, s).unwrap_err().code, "metadata_text");
    }
    // Variation selectors, Mongolian FVS and CGJ stay allowed in display text.
    let mut allowed = d.clone();
    allowed.metadata.properties.insert(
        "tags".into(),
        MetadataValue::Strings {
            value: vec!["\u{1820}\u{180b}".into(), "\u{845b}\u{e0100}".into()],
        },
    );
    assert!(imported(&allowed, s).is_ok());
    let mut allowed = d;
    allowed.metadata.title = Some("\u{2764}\u{fe0f} a\u{34f}b".into());
    assert!(imported(&allowed, s).is_ok());
}
