//! Bare GFM autolinks (CORE-003D): forms, exclusions, exact spans, policy and
//! v2 provenance. The golden fixture is `specs/parsing/autolinks`.
use markdown_core::*;
use std::path::PathBuf;

fn ids() -> (DocumentId, RevisionId) {
    (DocumentId("doc".into()), RevisionId("r1".into()))
}
fn parse(source: &str) -> Document {
    let (d, r) = ids();
    parse_markdown(source, d, r, ParseLimits::default()).unwrap_or_else(|e| panic!("{e:?}"))
}
/// (lexeme, destination) for every link, in order.
fn links(source: &str) -> Vec<(String, String)> {
    parse(source)
        .input()
        .links
        .iter()
        .map(|l| (l.span.slice(source).unwrap().into(), l.destination.clone()))
        .collect()
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../specs/parsing/autolinks")
        .join(name)
}

#[test]
fn fixture_matches_its_v1_golden() {
    let source = std::fs::read_to_string(fixture("input.source.txt")).unwrap();
    let doc = parse_markdown(
        &source,
        DocumentId("example".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("expected.json")).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&doc).unwrap(), expected);
    assert_eq!(
        Document::from_json(&doc.to_json().unwrap(), &source, ModelLimits::default()).unwrap(),
        doc
    );
}

#[cfg(feature = "model-v2")]
#[test]
fn fixture_matches_its_v2_golden_and_proves_its_provenance() {
    let source = std::fs::read_to_string(fixture("input.source.txt")).unwrap();
    let doc = parse_markdown_v2(
        &source,
        DocumentId("example".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("expected.v2.json")).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&doc).unwrap(), expected);
    let back = v2::Document::from_json(&doc.to_json().unwrap(), &source, ModelLimits::default());
    assert_eq!(back.unwrap(), doc);
}

#[test]
fn each_form_links_its_exact_lexeme() {
    assert_eq!(
        links("a https://x.example/p b"),
        [("https://x.example/p".into(), "https://x.example/p".into())]
    );
    assert_eq!(
        links("a www.x.example b"),
        [("www.x.example".into(), "http://www.x.example".into())]
    );
    assert_eq!(
        links("a me@x.example b"),
        [("me@x.example".into(), "mailto:me@x.example".into())]
    );
    let source = "see www.a.example, then";
    let d = parse(source);
    let link = &d.input().links[0];
    assert_eq!(link.span.slice(source), Some("www.a.example"));
    assert_eq!(link.disposition, LinkDisposition::External {});
    // The link node wraps one text child over the same span; text around it stays.
    let node = d
        .input()
        .nodes
        .iter()
        .find(|n| matches!(n.content, NodeKind::Link { .. }))
        .unwrap();
    assert_eq!(node.span, link.span);
    let texts: Vec<&str> = d
        .input()
        .nodes
        .iter()
        .filter_map(|n| match &n.content {
            NodeKind::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["see ", "www.a.example", ", then"]);
}

#[test]
fn spans_are_exact_across_split_text_events() {
    // `_`, `*` and `$` split pulldown-cmark text events; the link spans them.
    for source in [
        "x https://a.example/a_b_c y",
        "x https://a.example/a*b y",
        "x https://a.example/$5/x y",
        "x www.a.example/a_x_b y",
    ] {
        let d = parse(source);
        assert_eq!(d.input().links.len(), 1, "{source}");
        let lexeme = d.input().links[0].span.slice(source).unwrap();
        assert!(
            source.contains(&format!(" {lexeme} ")),
            "{source}: {lexeme}"
        );
    }
}

#[test]
fn documents_without_bare_links_keep_their_shape() {
    for source in ["plain _text_ with $5 and a*b\n", "a.b c@d e_f www\n"] {
        let d = parse(source);
        assert!(d.input().links.is_empty(), "{source}");
    }
}

#[test]
fn exclusions_never_link() {
    for source in [
        "`https://code.example`",
        "```\nhttps://block.example\n```",
        "    https://indented.example",
        "<div>\nhttps://html.example\n</div>",
        "![www.alt.example](i.png)",
        "xhttp://a.example awww.a.example www.a_b.c_d ftp://a.example",
    ] {
        let d = parse(source);
        assert!(
            d.input()
                .links
                .iter()
                .all(|l| !l.destination.contains("example")),
            "{source}: {:?}",
            d.input().links
        );
    }
    // Inside an existing link the label stays text and only the outer target links.
    for source in [
        "[www.label.example](https://real.example)",
        "<https://angle.example>",
    ] {
        let d = parse(source);
        assert_eq!(d.input().links.len(), 1, "{source}");
        let lexeme = d.input().links[0].span.slice(source).unwrap();
        assert!(!lexeme.starts_with("www.label"), "{source}");
    }
    // A decoded entity ends a run: spans stay exact source.
    let source = "https://a.example/&amp;b";
    assert_eq!(
        parse(source).input().links[0].destination,
        "https://a.example/"
    );
}

#[test]
fn links_work_in_inline_and_block_containers() {
    for source in [
        "# Title www.h.example\n",
        "> www.q.example\n",
        "- www.l.example\n",
        "| a |\n| - |\n| www.t.example |\n",
        "Text[^n]\n\n[^n]: www.f.example\n",
        "*www.e.example* **https://s.example** ~~me@x.example~~\n",
    ] {
        let d = parse(source);
        assert!(!d.input().links.is_empty(), "{source}");
        for l in &d.input().links {
            assert!(l.span.slice(source).is_some_and(|s| !s.is_empty()));
        }
    }
    // A heading keeps the link text in its outline title.
    let d = parse("# Visit www.h.example\n");
    assert_eq!(d.input().outline[0].title, "Visit www.h.example");
}

#[test]
fn targets_use_the_normal_uri_policy() {
    let d = parse("https://例え.example www.ü.example https://a.example\n");
    let kinds: Vec<bool> = d
        .input()
        .links
        .iter()
        .map(|l| matches!(l.disposition, LinkDisposition::Denied { .. }))
        .collect();
    // V1 is ASCII-only; the plain ASCII URL is external.
    assert_eq!(kinds, [true, true, false]);
    #[cfg(feature = "model-v2")]
    {
        let (di, ri) = ids();
        let d = parse_markdown_v2(
            "https://例え.example www.ü.example\n",
            di,
            ri,
            ParseLimits::default(),
        )
        .unwrap();
        for l in &d.input().links {
            assert!(
                matches!(&l.target, v2::Target::External { uri } if uri.contains("xn--")),
                "{l:?}"
            );
        }
    }
}

#[cfg(feature = "model-v2")]
#[test]
fn v2_proof_rejects_forged_autolink_intents() {
    let source = "see www.a.example and https://b.example now\n";
    let (di, ri) = ids();
    let doc = parse_markdown_v2(source, di, ri, ParseLimits::default()).unwrap();
    let decode = |input: &v2::DocumentInput| {
        v2::Document::from_json(
            &serde_json::to_vec(input).unwrap(),
            source,
            ModelLimits::default(),
        )
    };
    assert!(decode(doc.input()).is_ok());
    // A different destination with a self-consistent target.
    let mut forged = doc.input().clone();
    forged.links[0].destination = "http://www.evil.example".into();
    forged.links[0].target = v2::classify("http://www.evil.example", false);
    assert!(decode(&forged).is_err(), "destination");
    // A shifted span inside the same text.
    let mut forged = doc.input().clone();
    let v2::IntentSource::Inline { span } = &mut forged.links[1].source else {
        panic!()
    };
    span.start += 1;
    assert!(decode(&forged).is_err(), "span");
    // A link node over text that is not an autolink.
    let mut forged = doc.input().clone();
    let link = forged
        .nodes
        .iter()
        .position(|n| matches!(n.content, NodeKind::Link { .. }))
        .unwrap();
    forged.nodes[link].span.start -= 4;
    assert!(decode(&forged).is_err(), "occurrence");
}

#[test]
fn bounded_generated_autolink_markup_never_publishes_an_invalid_model() {
    let tokens = [
        "www.", "a.b", "http://", "https://", "@", "c.de", "_", "*", "(", ")", "&a;", "&amp;", "`",
        "[", "](", "<", ">", " ", "\n", "> ", "- ", "é", "$", ".", ";", "~",
    ];
    let mut seed = 0x2545f4914f6cdd1du64;
    for case in 0..20_000 {
        let mut source = String::new();
        for _ in 0..(case % 24 + 1) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            source.push_str(tokens[(seed % tokens.len() as u64) as usize]);
        }
        let (di, ri) = ids();
        let doc = parse_markdown(&source, di, ri, ParseLimits::default())
            .unwrap_or_else(|e| panic!("case {case}: {source:?}: {e:?}"));
        Document::from_json(&doc.to_json().unwrap(), &source, ModelLimits::default())
            .unwrap_or_else(|e| panic!("case {case}: {source:?}: {e:?}"));
        #[cfg(feature = "model-v2")]
        {
            let (di, ri) = ids();
            let doc = parse_markdown_v2(&source, di, ri, ParseLimits::default())
                .unwrap_or_else(|e| panic!("v2 case {case}: {source:?}: {e:?}"));
            v2::Document::from_json(&doc.to_json().unwrap(), &source, ModelLimits::default())
                .unwrap_or_else(|e| panic!("v2 case {case}: {source:?}: {e:?}"));
        }
    }
}

#[test]
fn link_counts_stay_within_the_node_and_text_limits() {
    let source = "a@b.co ".repeat(400_000);
    let (di, ri) = ids();
    match parse_markdown(&source, di, ri, ParseLimits::default()) {
        Ok(d) => assert_eq!(d.input().links.len(), 400_000),
        Err(e) => assert!(
            matches!(e, ParseError::NodeLimit | ParseError::TextLimit),
            "{e:?}"
        ),
    }
    let (di, ri) = ids();
    let tight = ParseLimits {
        model: ModelLimits {
            max_nodes: 100,
            ..ModelLimits::default()
        },
        ..ParseLimits::default()
    };
    assert!(matches!(
        parse_markdown(&"www.a.b ".repeat(100), di, ri, tight),
        Err(ParseError::NodeLimit)
    ));
}

#[test]
fn dollar_refunds_apply_while_text_is_buffered() {
    // One long paragraph: every `$` splits a text event, and the splits are
    // refunded as before CORE-003D even though the run is held (review 3).
    let units = 20_000;
    let source = "Costs $5 and $10, or $7.50. ".repeat(units);
    let limits = ParseLimits {
        max_events: units * 2,
        ..ParseLimits::default()
    };
    let (di, ri) = ids();
    let d = parse_markdown(&source, di, ri, limits).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(d.input().links.is_empty());
}
