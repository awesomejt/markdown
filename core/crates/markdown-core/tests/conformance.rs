//! Supported semantic corpus reconciliation (CORE-003B4).
//!
//! `specs/parsing/conformance/corpus.json` is generated from the fixture
//! expectations by `tools/generate_conformance.py`, which maps each expectation
//! to a check token here or records why it is deferred. Each case runs only its
//! listed checks; an unknown token fails so the contract and test stay in step.
mod support;

use markdown_core::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use support::profiles;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn contract() -> Value {
    let text = std::fs::read_to_string(root().join("specs/parsing/conformance/corpus.json"))
        .expect("conformance contract");
    serde_json::from_str(&text).expect("contract JSON")
}
fn parse(source: &str) -> Document {
    parse_markdown(
        source,
        DocumentId("conformance".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}
fn feature(kind: &NodeKind) -> Option<&str> {
    match kind {
        NodeKind::UnsupportedBlock { feature, .. }
        | NodeKind::UnsupportedInline { feature, .. } => Some(feature),
        _ => None,
    }
}
fn count(d: &Document, key: &str) -> usize {
    let nodes = &d.input().nodes;
    let n = |p: &dyn Fn(&NodeKind) -> bool| nodes.iter().filter(|n| p(&n.content)).count();
    let f = |names: &[&str]| n(&|k| feature(k).is_some_and(|f| names.contains(&f)));
    match key {
        "headings" => n(&|k| matches!(k, NodeKind::Heading { .. })),
        "tables" => n(&|k| matches!(k, NodeKind::Table { .. })),
        "task_items" => n(&|k| {
            matches!(
                k,
                NodeKind::ListItem {
                    checked: Some(_),
                    ..
                }
            )
        }),
        "alerts" => n(&|k| matches!(k, NodeKind::Alert { .. })),
        "blockquotes" => n(&|k| matches!(k, NodeKind::BlockQuote { .. })),
        "code_fences" => fences(d).len(),
        "footnote_definitions" => n(&|k| matches!(k, NodeKind::FootnoteDefinition { .. })),
        "math_inline" => f(&["inline_math"]),
        "math_display" => f(&["display_math"]),
        "math_fences" => f(&["math", "latex"]),
        "diagram_fences" => f(&["mermaid", "d2"]),
        "raw_html_constructs" => f(&["raw_html"]),
        "links" => d.input().links.len(),
        other => panic!("uncounted feature {other}"),
    }
}
/// Fenced blocks in reading order as (language, semantic). The model stores
/// the raw info string; the fixture language is its first word, or null.
fn fences(d: &Document) -> Vec<(Option<String>, &'static str)> {
    d.input()
        .nodes
        .iter()
        .filter_map(|n| match &n.content {
            NodeKind::CodeBlock {
                info: Some(info), ..
            } => Some((info.split_whitespace().next().map(str::to_owned), "code")),
            k => match feature(k)? {
                f @ ("math" | "latex") => Some((Some(f.to_owned()), "math")),
                f @ ("mermaid" | "d2") => Some((Some(f.to_owned()), "diagram")),
                _ => None,
            },
        })
        .collect()
}
fn flat_text(d: &Document, id: &NodeId) -> String {
    let node = d.input().nodes.iter().find(|n| &n.id == id).unwrap();
    match &node.content {
        NodeKind::Text { text } => text.clone(),
        k => k.children().iter().map(|c| flat_text(d, c)).collect(),
    }
}
fn metadata_json(v: &MetadataValue) -> Value {
    match v {
        MetadataValue::Null {} => Value::Null,
        MetadataValue::Boolean { value } => Value::from(*value),
        MetadataValue::Integer { value } => Value::from(*value),
        MetadataValue::String { value } => Value::from(value.clone()),
        MetadataValue::Strings { value } => Value::from(value.clone()),
    }
}
fn disposition(l: &LinkDisposition) -> &'static str {
    match l {
        LinkDisposition::Fragment { .. } => "fragment",
        LinkDisposition::RequiresCapability {} => "relative",
        LinkDisposition::External {} => "external",
        LinkDisposition::Denied { .. } => "blocked",
    }
}
fn has_diagnostic(d: &Document, node: &NodeId) -> bool {
    d.input()
        .diagnostics
        .iter()
        .any(|x| x.node.as_ref() == Some(node))
}

fn check(case: &Value, token: &str, d: &Document, source: &str) {
    let id = case["id"].as_str().unwrap();
    let input = d.input();
    match token {
        "outline" => {
            let got: Vec<Value> = input
                .outline
                .iter()
                .map(|e| serde_json::json!({"level": e.level, "text": e.title, "slug": e.anchor}))
                .collect();
            assert_eq!(&Value::from(got), &case["outline"], "{id}: outline");
        }
        "front_matter" => {
            let fm = &case["front_matter"];
            let present = input.metadata.front_matter_span.is_some();
            assert_eq!(Value::from(present), fm["present"], "{id}: front matter");
            let got: serde_json::Map<String, Value> = input
                .metadata
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), metadata_json(v)))
                .collect();
            assert_eq!(Value::Object(got), fm["properties"], "{id}: properties");
        }
        "counts" => {
            for (key, expected) in case["counts"].as_object().unwrap() {
                assert_eq!(
                    count(d, key) as u64,
                    expected.as_u64().unwrap(),
                    "{id}: count {key}"
                );
            }
        }
        "code_fences" => {
            let got: Vec<Value> = fences(d)
                .into_iter()
                .map(|(language, semantic)| {
                    serde_json::json!({"language": language, "semantic": semantic})
                })
                .collect();
            assert_eq!(&Value::from(got), &case["code_fences"], "{id}: fences");
        }
        "links" => {
            let got: Vec<Value> = input
                .links
                .iter()
                .map(|l| serde_json::json!({"target": l.destination, "kind": disposition(&l.disposition)}))
                .collect();
            assert_eq!(&Value::from(got), &case["links"], "{id}: links");
        }
        "alerts" => {
            let got: Vec<String> = input
                .nodes
                .iter()
                .filter_map(|n| match &n.content {
                    NodeKind::Alert { category, .. } => Some(
                        serde_json::to_value(category)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_uppercase(),
                    ),
                    _ => None,
                })
                .collect();
            assert_eq!(Value::from(got), case["alerts"], "{id}: alerts");
        }
        "non_alert_marker" => {
            let marker = case["non_alert_marker"].as_str().unwrap();
            assert!(
                input
                    .nodes
                    .iter()
                    .any(|n| matches!(n.content, NodeKind::BlockQuote { .. })
                        && flat_text(d, &n.id).contains(marker)),
                "{id}: look-alike stays a quote"
            );
        }
        "math" => {
            let m = &case["math"];
            for (key, field) in [
                ("math_inline", "inline"),
                ("math_display", "display"),
                ("math_fences", "fences"),
            ] {
                assert_eq!(
                    count(d, key) as u64,
                    m[field].as_u64().unwrap(),
                    "{id}: math {field}"
                );
            }
        }
        "voice_mode" => {
            assert_eq!(case["voice_mode"], "script");
            assert!(
                input
                    .diagnostics
                    .iter()
                    .any(|x| x.code == "voice_script_unavailable"),
                "{id}: script opt-in diagnostic"
            );
        }
        "no_links" => assert!(input.links.is_empty() && input.resources.is_empty(), "{id}"),
        "fallback_diagnostics" => {
            for n in input.nodes.iter().filter(|n| feature(&n.content).is_some()) {
                assert!(
                    has_diagnostic(d, &n.id),
                    "{id}: fallback {} has no diagnostic",
                    n.id.0
                );
                assert!(n.span.slice(source).is_some_and(|s| !s.is_empty()), "{id}");
            }
        }
        "html_inert" => {
            // Raw HTML is only ever an inert source node with a diagnostic;
            // its exact source stays visible and nothing in the model executes.
            for n in input
                .nodes
                .iter()
                .filter(|n| feature(&n.content) == Some("raw_html"))
            {
                assert!(
                    has_diagnostic(d, &n.id),
                    "{id}: raw HTML {} diagnostic",
                    n.id.0
                );
                assert!(
                    n.span.slice(source).unwrap().trim_start().starts_with('<'),
                    "{id}"
                );
            }
        }
        "blocked_links" => {
            let schemes: Vec<String> = case["blocked_url_schemes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| format!("{}:", s.as_str().unwrap()))
                .collect();
            let blocked = |dest: &str| {
                let dest = dest.trim().to_ascii_lowercase();
                schemes.iter().any(|s| dest.starts_with(s))
            };
            for l in input.links.iter().filter(|l| blocked(&l.destination)) {
                assert!(
                    matches!(l.disposition, LinkDisposition::Denied { .. }),
                    "{id}"
                );
            }
            for r in input.resources.iter().filter(|r| blocked(&r.destination)) {
                assert!(matches!(r.access, ResourceAccess::Denied { .. }), "{id}");
            }
        }
        other => panic!("{id}: unknown check token {other}"),
    }
}

#[test]
fn supported_corpus_matches_reconciled_fixture_expectations() {
    let contract = contract();
    let cases = contract["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let source = std::fs::read_to_string(root().join(case["input"].as_str().unwrap())).unwrap();
        let d = parse(&source);
        for token in case["checks"].as_array().unwrap() {
            check(case, token.as_str().unwrap(), &d, &source);
        }
        // Every deferral carries a reason for the documentation table.
        for item in case["deferred"].as_array().unwrap() {
            assert!(item["reason"].as_str().is_some_and(|r| !r.is_empty()));
        }
        #[cfg(feature = "model-v2")]
        {
            let v2 = parse_markdown_v2(
                &source,
                DocumentId("conformance".into()),
                RevisionId("r1".into()),
                ParseLimits::default(),
            )
            .unwrap();
            assert_eq!(v2.input().outline, d.input().outline);
            assert_eq!(v2.input().links.len(), d.input().links.len());
        }
    }
}

#[test]
fn blocked_schemes_are_denied_in_every_spelling() {
    for scheme in [
        "javascript",
        "vbscript",
        "data",
        "JavaScript",
        "DATA",
        "file",
    ] {
        for payload in [
            ":alert(1)",
            ":text/html;base64,PHNjcmlwdD4=",
            "&#58;x",
            "%3Ax",
        ] {
            let source = format!("[x]({scheme}{payload}) ![y]({scheme}{payload})\n");
            let d = parse(&source);
            for l in &d.input().links {
                assert!(
                    matches!(l.disposition, LinkDisposition::Denied { .. }),
                    "{source:?}"
                );
            }
            for r in &d.input().resources {
                assert!(
                    matches!(r.access, ResourceAccess::Denied { .. }),
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn rust_profile_generators_match_the_fixture_profiles() {
    let contract = contract();
    let expected = contract["profiles"].as_array().unwrap();
    assert_eq!(expected.len(), profiles::PROFILES.len());
    for (p, e) in profiles::PROFILES.iter().zip(expected) {
        assert_eq!(e["id"], p.id);
        assert_eq!(e["generator"], p.generator);
        assert_eq!(e["target_bytes"].as_u64().unwrap() as usize, p.target_bytes);
        assert_eq!(e["line_bytes"].as_u64().unwrap() as usize, p.line_bytes);
        let bytes = profiles::generate(p.generator, p.target_bytes, p.line_bytes);
        assert_eq!(bytes.len(), p.target_bytes);
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(e["sha256"], digest.as_str(), "{}", p.id);
    }
}
