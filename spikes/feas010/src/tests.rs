//! Misparse and hostile-input checks, run against both engines.

use crate::common::{self, Semantics};
use crate::{cm, pd};

fn both(source: &str) -> [Semantics; 2] {
    [
        common::finish(source, pd::extract(source), false),
        common::finish(source, cm::extract(source), false),
    ]
}

const SCRIPT: &str = "---\nmdv: script\n---\n\n";

fn script(body: &str) -> [Semantics; 2] {
    let source: &'static str = Box::leak(format!("{SCRIPT}{body}").into_boxed_str());
    both(source)
}

fn directions(s: &Semantics) -> Vec<String> {
    s.voice
        .as_ref()
        .unwrap()
        .directions
        .iter()
        .map(|d| d.text.clone())
        .collect()
}

#[test]
fn lowercase_bold_label_is_not_a_speaker() {
    for s in script("**Note:** this is prose.\n\n**ANNA:** Hello.\n") {
        assert_eq!(s.voice.unwrap().speakers, ["ANNA"]);
    }
}

#[test]
fn listed_speaker_may_be_mixed_case() {
    let source = "---\nmdv: script\nspeakers: [Anna]\n---\n\n**Anna:** Hello.\n";
    for s in both(source) {
        assert_eq!(s.voice.unwrap().speakers, ["Anna"]);
    }
}

#[test]
fn digit_and_sic_brackets_are_not_directions() {
    for s in script("See [1] and [sic] but [beat] counts.\n") {
        assert_eq!(directions(&s), ["beat"]);
    }
}

#[test]
fn task_markers_are_not_directions() {
    for s in script("- [x] done [pause]\n- [ ] open\n") {
        assert_eq!(directions(&s), ["pause"]);
        assert_eq!(s.feature_counts["task_items"], 2);
    }
}

#[test]
fn defined_reference_becomes_a_link_not_a_direction() {
    for s in script("Say [beat] now.\n\n[beat]: https://example.com\n") {
        assert!(directions(&s).is_empty());
        assert_eq!(s.links.len(), 1);
    }
}

#[test]
fn links_code_math_and_headings_are_excluded() {
    let body = "# Title [beat]\n\nA [link [x]](a.md), `[code]`, $[m]$ and [real] {a|b}.\n";
    for s in script(body) {
        assert_eq!(directions(&s), ["real"]);
        assert_eq!(s.voice.as_ref().unwrap().pronunciations.len(), 1);
    }
}

#[test]
fn notes_match_before_directions() {
    for s in script("Text [[a note]] and [cue].\n") {
        let v = s.voice.as_ref().unwrap();
        assert_eq!(v.notes.len(), 1);
        assert_eq!(directions(&s), ["cue"]);
    }
}

#[test]
fn syntax_is_inert_without_opt_in() {
    for s in both("**ANNA:** Hello [beat] {a|b} [[note]].\n") {
        assert!(s.voice.is_none());
    }
    for s in both("---\nmdv: notes\n---\n\n**ANNA:** Hello [beat].\n") {
        assert!(s.voice.is_none());
    }
}

#[test]
fn hostile_front_matter_is_rejected_but_opt_in_survives() {
    let bombs = [
        "---\na: &x [1]\nb: *x\nmdv: script\n---\n\nText [beat].\n",
        "---\nmdv: script\nnested:\n  key: value\n---\n\nText [beat].\n",
        "---\nmdv: script\nmdv: script\n---\n\nText [beat].\n",
    ];
    for source in bombs {
        for s in both(source) {
            assert!(s.front_matter.present);
            assert!(s.front_matter.properties.is_none(), "{source}");
            // E1: the line-scan fallback still opts in.
            assert_eq!(directions(&s), ["beat"], "{source}");
        }
    }
}

#[test]
fn unclosed_front_matter_is_not_front_matter() {
    for s in both("---\ntitle: x\n\n# Heading\n") {
        assert!(!s.front_matter.present);
    }
}

#[test]
fn alert_look_alike_and_empty_alert_are_plain_quotes() {
    for s in both("> [!NOTE] trailing text\n\n> [!TIP]\n\n> [!WARNING]\n> Real.\n") {
        assert_eq!(s.alerts, ["WARNING"]);
    }
}

#[test]
fn engines_agree_on_spans_for_the_corpus_shapes() {
    let body = "**ANNA:** (softly) One {a\\|b} [cue].\n**BOB:** Two.\n\n| Visual | Audio |\n| --- | --- |\n| shot [x1] | **ANNA:** said [beat] |\n";
    let [p, c] = script(body);
    let (pv, cv) = (p.voice.as_ref().unwrap(), c.voice.as_ref().unwrap());
    let spans = |v: &common::Voice| -> Vec<[usize; 2]> {
        v.directions
            .iter()
            .chain(&v.notes)
            .map(|d| d.span)
            .chain(v.pronunciations.iter().map(|x| x.span))
            .collect()
    };
    assert_eq!(spans(pv), spans(cv));
    assert_eq!(pv.speakers, ["ANNA", "BOB"]);
    assert_eq!(pv.av_tables, 1);
    // The Visual cell's bracket is not a direction.
    assert!(!directions(&p).contains(&"x1".to_string()));
    assert!(p.span_checks.failures.is_empty() && c.span_checks.failures.is_empty());
}

#[test]
#[ignore = "diagnostic: prints raw parser behavior before shared passes"]
fn raw_parser_behavior() {
    let cases = [
        ("alert with trailing text", "> [!NOTE] trailing text\n"),
        ("empty alert", "> [!TIP]\n"),
        ("lowercase alert", "> [!note]\n> body\n"),
        ("unbalanced inline math", "an error $\\frac{1}{$ here\n"),
        ("dollar amount", "costs $5 and $6 today\n"),
        ("front matter ... close", "---\na: b\n...\n\n# H\n"),
        ("front matter not first line", "\n---\na: b\n---\n"),
    ];
    for (name, src) in cases {
        let p = pd::extract(src);
        let c = cm::extract(src);
        println!(
            "{name:28} pulldown alerts={:?} quotes={} math={} fm={} | comrak alerts={:?} quotes={} math={} fm={}",
            p.alerts.iter().map(|a| &a.0).collect::<Vec<_>>(),
            p.blockquotes.len(),
            p.math.len(),
            p.front_matter.is_some(),
            c.alerts.iter().map(|a| &a.0).collect::<Vec<_>>(),
            c.blockquotes.len(),
            c.math.len(),
            c.front_matter.is_some()
        );
    }
}

#[test]
#[ignore = "diagnostic: mid-document metadata acceptance"]
fn raw_mid_document_metadata() {
    let cases = [
        (
            "after paragraph",
            "Intro text.\n\n---\ntitle: x\n---\n\nMore.\n",
        ),
        (
            "rule then text then rule",
            "# H\n\n---\nVisible words here\n---\n\nEnd.\n",
        ),
        (
            "two rules around prose",
            "Para.\n\n---\n\nVisible.\n\n---\n",
        ),
    ];
    for (name, src) in cases {
        let text: String = pulldown_cmark::Parser::new_ext(src, pd::options())
            .filter_map(|e| match e {
                pulldown_cmark::Event::Text(t) => Some(t.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("|");
        println!(
            "{name:28} metadata={} text events={text:?}",
            pd::extract(src).front_matter.is_some()
        );
    }
}

#[test]
fn mid_document_rules_are_never_front_matter() {
    for s in both("# H\n\n---\nVisible words here\n---\n\nEnd.\n") {
        assert!(!s.front_matter.present);
        let texts: Vec<_> = s.outline.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, ["H", "Visible words here"]);
    }
    for s in both("---\r\ntitle: x\r\n---\r\n\r\n# H\r\n") {
        assert!(s.front_matter.present);
        assert_eq!(s.outline.len(), 1);
    }
}
