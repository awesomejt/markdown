//! Supported semantic corpus reconciliation (CORE-003B4).
//!
//! `specs/parsing/conformance/corpus.json` is generated from the fixture
//! expectations by `tools/generate_conformance.py`, which maps each expectation
//! to a check token here or records why it is deferred. Each case runs only its
//! listed checks; an unknown token fails so the contract and test stay in step.
//!
//! Every token runs on v1 and, with `model-v2`, on v2 through the same
//! version-neutral [`View`]. Intents differ by version, so v2 is also paired
//! with v1 intent by intent against explicit expectations ([`v2_matches_v1`]).
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
#[cfg(feature = "model-v2")]
fn parse_v2(source: &str) -> v2::Document {
    parse_markdown_v2(
        source,
        DocumentId("conformance".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}

/// A link or resource intent as the fixtures name it: destination and kind.
struct Intent<'a> {
    destination: &'a str,
    kind: &'static str,
}
/// The version-independent fields, plus intents reduced to fixture kinds.
struct View<'a> {
    version: u32,
    nodes: &'a [Node],
    outline: &'a [OutlineEntry],
    metadata: &'a Metadata,
    diagnostics: &'a [Diagnostic],
    links: Vec<Intent<'a>>,
    resources: Vec<Intent<'a>>,
}
impl<'a> View<'a> {
    fn v1(d: &'a Document) -> Self {
        let i = d.input();
        Self {
            version: i.model_version,
            nodes: &i.nodes,
            outline: &i.outline,
            metadata: &i.metadata,
            diagnostics: &i.diagnostics,
            links: i
                .links
                .iter()
                .map(|l| Intent {
                    destination: &l.destination,
                    kind: disposition(&l.disposition),
                })
                .collect(),
            resources: i
                .resources
                .iter()
                .map(|r| Intent {
                    destination: &r.destination,
                    kind: match r.access {
                        ResourceAccess::RequiresCapability {} => "relative",
                        ResourceAccess::Denied { .. } => "blocked",
                    },
                })
                .collect(),
        }
    }
    #[cfg(feature = "model-v2")]
    fn v2(d: &'a v2::Document) -> Self {
        let i = d.input();
        let intent = |destination: &'a String, target: &v2::Target| Intent {
            destination,
            kind: target_kind(target),
        };
        Self {
            version: i.model_version,
            nodes: &i.nodes,
            outline: &i.outline,
            metadata: &i.metadata,
            diagnostics: &i.diagnostics,
            links: i
                .links
                .iter()
                .map(|l| intent(&l.destination, &l.target))
                .collect(),
            resources: i
                .resources
                .iter()
                .map(|r| intent(&r.destination, &r.target))
                .collect(),
        }
    }
}
fn feature(kind: &NodeKind) -> Option<&str> {
    match kind {
        NodeKind::UnsupportedBlock { feature, .. }
        | NodeKind::UnsupportedInline { feature, .. } => Some(feature),
        _ => None,
    }
}
fn count(d: &View, key: &str) -> usize {
    let nodes = d.nodes;
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
        "links" => d.links.len(),
        other => panic!("uncounted feature {other}"),
    }
}
/// Fenced blocks in reading order as (language, semantic). The model stores
/// the raw info string; the fixture language is its first word, or null.
fn fences(d: &View) -> Vec<(Option<String>, &'static str)> {
    d.nodes
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
fn flat_text(d: &View, id: &NodeId) -> String {
    let node = d.nodes.iter().find(|n| &n.id == id).unwrap();
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
#[cfg(feature = "model-v2")]
fn target_kind(t: &v2::Target) -> &'static str {
    match t {
        v2::Target::Fragment { .. } => "fragment",
        v2::Target::RequiresCapability { .. } => "relative",
        v2::Target::External { .. } => "external",
        v2::Target::Denied { .. } => "blocked",
    }
}
fn has_diagnostic(d: &View, node: &NodeId) -> bool {
    d.diagnostics.iter().any(|x| x.node.as_ref() == Some(node))
}

fn check(case: &Value, token: &str, d: &View, source: &str) {
    let id = format!("{} (v{})", case["id"].as_str().unwrap(), d.version);
    let input = d;
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
                .map(|l| serde_json::json!({"target": l.destination, "kind": l.kind}))
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
            for i in input.links.iter().chain(&input.resources) {
                if blocked(i.destination) {
                    assert_eq!(i.kind, "blocked", "{id}: {}", i.destination);
                }
            }
        }
        other => panic!("{id}: unknown check token {other}"),
    }
}

/// V2 must agree with v1 on every version-independent field and pair with it
/// intent by intent: same ID, destination and inline span, and a target that
/// is the typed form of the v1 disposition (documented in the contract).
#[cfg(feature = "model-v2")]
fn v2_matches_v1(id: &str, v1: &Document, v2: &v2::Document, source: &str) {
    let (a, b) = (v1.input(), v2.input());
    assert_eq!(b.model_version, 2, "{id}");
    assert_eq!(b.document_id, a.document_id, "{id}: document id");
    assert_eq!(b.revision_id, a.revision_id, "{id}: revision id");
    assert_eq!(b.source, a.source, "{id}: source");
    assert_eq!(b.capabilities, a.capabilities, "{id}: capabilities");
    assert_eq!(b.roots, a.roots, "{id}: roots");
    assert_eq!(b.nodes, a.nodes, "{id}: nodes");
    assert_eq!(b.outline, a.outline, "{id}: outline");
    assert_eq!(b.metadata, a.metadata, "{id}: metadata");
    assert_eq!(b.diagnostics, a.diagnostics, "{id}: diagnostics");
    assert_eq!(b.links.len(), a.links.len(), "{id}: link count");
    for (x, y) in a.links.iter().zip(&b.links) {
        let what = format!("{id}: link {}", x.destination);
        assert_eq!((&y.id, &y.destination), (&x.id, &x.destination), "{what}");
        assert_eq!(
            y.source,
            v2::IntentSource::Inline { span: x.span },
            "{what}"
        );
        let dest = x.destination.as_str();
        match (&x.disposition, &y.target) {
            (LinkDisposition::Fragment { anchor }, v2::Target::Fragment { anchor: a2 }) => {
                assert_eq!(a2, anchor, "{what}")
            }
            (
                LinkDisposition::RequiresCapability {},
                v2::Target::RequiresCapability { path, fragment },
            ) => {
                let joined = match fragment {
                    Some(f) => format!("{path}#{f}"),
                    None => path.clone(),
                };
                assert_eq!(joined, dest, "{what}: path and fragment")
            }
            (LinkDisposition::External {}, v2::Target::External { uri }) => assert!(
                uri == dest || *uri == format!("{dest}/"),
                "{what}: normalized URI {uri}"
            ),
            (LinkDisposition::Denied { reason }, v2::Target::Denied { reason: r2 }) => {
                assert_eq!(reason, "unsafe_or_unsupported_destination", "{what}");
                assert!(
                    matches!(r2, v2::Denial::Unsafe | v2::Denial::UnsupportedScheme),
                    "{what}: {r2:?}"
                )
            }
            (d, t) => panic!("{what}: v1 {d:?} and v2 {t:?} disagree"),
        }
    }
    assert_eq!(b.resources.len(), a.resources.len(), "{id}: resource count");
    for (x, y) in a.resources.iter().zip(&b.resources) {
        let what = format!("{id}: resource {}", x.destination);
        assert_eq!((&y.id, &y.destination), (&x.id, &x.destination), "{what}");
        assert_eq!(
            y.source,
            v2::IntentSource::Inline { span: x.span },
            "{what}"
        );
        match (&x.access, &y.target) {
            (ResourceAccess::RequiresCapability {}, v2::Target::RequiresCapability { .. })
            | (ResourceAccess::Denied { .. }, v2::Target::Denied { .. }) => {}
            (d, t) => panic!("{what}: v1 {d:?} and v2 {t:?} disagree"),
        }
    }
    // The published v2 snapshot survives its own provenance proof.
    let wire = v2.to_json().unwrap();
    let back = v2::Document::from_json(&wire, source, ModelLimits::default()).unwrap();
    assert_eq!(&back, v2, "{id}: v2 JSON round trip");
}

#[test]
fn supported_corpus_matches_reconciled_fixture_expectations() {
    let contract = contract();
    let cases = contract["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let source = std::fs::read_to_string(root().join(case["input"].as_str().unwrap())).unwrap();
        let d = parse(&source);
        #[cfg(feature = "model-v2")]
        let d2 = parse_v2(&source);
        let views = [
            View::v1(&d),
            #[cfg(feature = "model-v2")]
            View::v2(&d2),
        ];
        for view in &views {
            for token in case["checks"].as_array().unwrap() {
                check(case, token.as_str().unwrap(), view, &source);
            }
        }
        #[cfg(feature = "model-v2")]
        v2_matches_v1(case["id"].as_str().unwrap(), &d, &d2, &source);
        // Every deferral carries a reason for the documentation table.
        for item in case["deferred"].as_array().unwrap() {
            assert!(item["reason"].as_str().is_some_and(|r| !r.is_empty()));
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
            #[cfg(feature = "model-v2")]
            {
                let d2 = parse_v2(&source);
                let i = d2.input();
                assert_eq!(i.links.len() + i.resources.len(), 2, "{source:?}");
                for t in i
                    .links
                    .iter()
                    .map(|l| &l.target)
                    .chain(i.resources.iter().map(|r| &r.target))
                {
                    assert!(matches!(t, v2::Target::Denied { .. }), "{source:?}: {t:?}");
                }
            }
        }
    }
}

/// V1 deliberately leaves reference links and images inert: each occurrence is
/// an `UnsupportedInline` source fallback with a `reference_link_deferred`
/// diagnostic and no intent. V2 turns exactly those occurrences into
/// reference-sourced intents. Uses the v2 reference fixture, because the
/// frozen corpus has no reference syntax.
#[test]
fn references_inert_in_v1_are_reference_intents_in_v2() {
    let path = root().join("specs/semantic-model/v2/fixtures/references.source.txt");
    let source = std::fs::read_to_string(path).unwrap();
    let d = parse(&source);
    let a = d.input();
    let inert: Vec<SourceSpan> = a
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                feature(&n.content),
                Some("reference_link" | "reference_image")
            )
        })
        .map(|n| n.span)
        .collect();
    assert_eq!(inert.len(), 6, "v1 reference fallbacks");
    for n in a.nodes.iter().filter(|n| inert.contains(&n.span)) {
        assert!(
            a.diagnostics
                .iter()
                .any(|x| x.node.as_ref() == Some(&n.id) && x.code == "reference_link_deferred"),
            "v1 {:?} deferred diagnostic",
            n.span
        );
    }
    assert!(a.resources.is_empty(), "v1 reference images have no intent");
    assert_eq!(a.links.len(), 4, "v1 keeps only the inline links");
    let check_tokens = |view: &View| {
        for token in ["fallback_diagnostics", "html_inert"] {
            check(
                &serde_json::json!({"id": "references"}),
                token,
                view,
                &source,
            );
        }
    };
    check_tokens(&View::v1(&d));
    #[cfg(feature = "model-v2")]
    {
        let d2 = parse_v2(&source);
        let b = d2.input();
        check_tokens(&View::v2(&d2));
        assert_eq!(b.capabilities, a.capabilities);
        assert_eq!(b.metadata, a.metadata);
        assert_eq!(b.outline, a.outline);
        assert!(b.nodes.iter().all(|n| !matches!(
            feature(&n.content),
            Some("reference_link" | "reference_image")
        )));
        assert!(
            b.diagnostics
                .iter()
                .all(|x| x.code != "reference_link_deferred")
        );
        // The occurrences v1 left inert are exactly v2's reference intents.
        let reference = |s: &v2::IntentSource| matches!(s, v2::IntentSource::Reference { .. });
        let mut active: Vec<SourceSpan> = b
            .nodes
            .iter()
            .filter(|n| match &n.content {
                NodeKind::Link { target, .. } => b
                    .links
                    .iter()
                    .any(|l| &l.id == target && reference(&l.source)),
                NodeKind::Image { resource, .. } => b
                    .resources
                    .iter()
                    .any(|r| &r.id == resource && reference(&r.source)),
                _ => false,
            })
            .map(|n| n.span)
            .collect();
        active.sort_by_key(|s| s.start);
        assert_eq!(active, inert, "reference occurrences");
        for s in b
            .links
            .iter()
            .map(|l| &l.source)
            .chain(b.resources.iter().map(|r| &r.source))
        {
            if let v2::IntentSource::Reference { definition, span } = s {
                assert!(definition.start <= span.start && span.end <= definition.end);
                assert!(definition.slice(&source).unwrap().starts_with('['));
            }
        }
        // V2 inline intents pair with v1's by destination and span. IDs are
        // snapshot-local (D-021) and v2 also numbers the reference intents, so
        // they differ here. The policies split on purpose: v1 denies non-ASCII
        // and percent-encoded destinations, which v2 decodes into typed targets.
        let inline: Vec<_> = b.links.iter().filter(|l| !reference(&l.source)).collect();
        let fragment = |anchor: &str| v2::Target::Fragment {
            anchor: anchor.into(),
        };
        let external = |uri: &str| v2::Target::External { uri: uri.into() };
        let legacy_denied = LinkDisposition::Denied {
            reason: "unsafe_or_unsupported_destination".into(),
        };
        let expected = [
            (
                legacy_denied.clone(),
                v2::Target::RequiresCapability {
                    path: "docs/café one.md".into(),
                    fragment: Some("café".into()),
                },
            ),
            (
                legacy_denied.clone(),
                external("https://xn--r8jz45g.test/a%20b"),
            ),
            (legacy_denied, fragment("café")),
            (
                LinkDisposition::External {},
                external("mailto:a@example.com"),
            ),
        ];
        assert_eq!(inline.len(), a.links.len());
        assert_eq!(a.links.len(), expected.len());
        for ((l2, l1), (disposition, target)) in inline.iter().zip(&a.links).zip(&expected) {
            assert_eq!(
                (&l2.destination, l2.source.span()),
                (&l1.destination, l1.span)
            );
            assert_eq!(&l1.disposition, disposition, "v1 {}", l1.destination);
            assert_eq!(&l2.target, target, "v2 {}", l2.destination);
        }
        let targets = |dest: &str| -> Vec<&v2::Target> {
            b.links
                .iter()
                .map(|l| (&l.destination, &l.target))
                .chain(b.resources.iter().map(|r| (&r.destination, &r.target)))
                .filter(|(d, _)| d.as_str() == dest)
                .map(|(_, t)| t)
                .collect()
        };
        let denied = |r| v2::Target::Denied { reason: r };
        assert_eq!(
            targets("javascript:alert(1)"),
            [&denied(v2::Denial::UnsupportedScheme)]
        );
        assert_eq!(
            targets("https://example.com/image.png"),
            [&denied(v2::Denial::RemoteResource)]
        );
        assert_eq!(
            targets("images/cafe%CC%81.png"),
            [&v2::Target::RequiresCapability {
                path: "images/cafe\u{301}.png".into(),
                fragment: None,
            }],
            "decoded, normalization preserved"
        );
        assert_eq!(
            targets("docs/café%20one.md").len(),
            3,
            "one intent per occurrence"
        );
        let wire = d2.to_json().unwrap();
        let back = v2::Document::from_json(&wire, &source, ModelLimits::default()).unwrap();
        assert_eq!(back, d2);
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
