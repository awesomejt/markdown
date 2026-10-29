//! Hostile-input and property coverage (CORE-003B4).
//!
//! Three deterministic layers complement the token generators in parser.rs:
//! corpus-seeded structural mutations with model properties, a catalog of
//! pathological constructs that must stop with a validated model or a typed
//! limit, and the generated performance profiles under default limits.
//! `MD_FUZZ_CASES` raises the mutation count for longer local runs. None of
//! this is coverage-guided fuzzing.
mod support;

use markdown_core::search::{FindOptions, ReadOptions, SearchDocument, find, read_stats};
use markdown_core::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use support::profiles;

fn ids() -> (DocumentId, RevisionId) {
    (DocumentId("hostile".into()), RevisionId("r1".into()))
}
fn v1(source: &str) -> Result<Document, ParseError> {
    let (d, r) = ids();
    parse_markdown(source, d, r, ParseLimits::default())
}
/// Typed defensive stops; anything else (or a panic) is a defect.
fn limit(error: &ParseError) -> bool {
    matches!(
        error,
        ParseError::SourceLimit
            | ParseError::EventLimit
            | ParseError::NodeLimit
            | ParseError::DepthLimit
            | ParseError::TextLimit
    )
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
    fn below(&mut self, n: usize) -> usize {
        self.next() % n.max(1)
    }
}
fn boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

const TOKENS: &[&str] = &[
    "\t",
    "\n",
    "\r\n",
    "\r",
    "> ",
    "- ",
    "1. ",
    "* ",
    "[",
    "]",
    "(",
    ")",
    "![",
    "](",
    "<",
    ">",
    "`",
    "```",
    "~~~",
    "$",
    "$$",
    "*",
    "_",
    "~~",
    "|",
    "| --- |",
    "\\",
    "&",
    "&#0;",
    "&#x110000;",
    "&amp;",
    "[^n]",
    "[^n]: ",
    "[r]: u.md",
    "[r]",
    "> [!NOTE]",
    "<div>",
    "<!--",
    "-->",
    "---",
    "===",
    "#",
    "    ",
    "\u{0}",
    "\u{feff}",
    "\u{202e}",
    "\u{200d}",
    "e\u{301}",
    "🙂",
    "javascript:",
    "www.",
    "www.a.b",
    "https://",
    "http://a.b/(",
    "a@b.co",
    "&a;",
    "<http://a>",
    "<a@b.c>",
    "[x](#a)",
    "[x](../a.md)",
];

fn corpus() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/markdown");
    let mut seeds: Vec<String> = [
        "foundation/gfm-tour.md",
        "foundation/unsafe-html.md",
        "foundation/code-fences.md",
        "advanced/math-alerts.md",
        "advanced/inline-diagrams.md",
        "advanced/voice-parity.md",
        "paths/unicode-filename.md",
        "paths/nfd-filename.md",
    ]
    .iter()
    .map(|p| std::fs::read_to_string(root.join(p)).unwrap())
    .collect();
    seeds.push(
        "---\ntitle: x\ntags: [a, b]\n---\n# T\n\n[a][r] [^n]\n\n[r]: a.md\n[^n]: n\n".into(),
    );
    seeds
}

/// One structural edit at char boundaries: insert, delete, duplicate, line
/// ending rewrite or container prefixing of a line range.
fn mutate(rng: &mut Rng, s: &mut String) {
    let at = boundary(s, rng.below(s.len() + 1));
    match rng.below(6) {
        0 | 1 => s.insert_str(at, TOKENS[rng.below(TOKENS.len())]),
        2 => {
            let end = boundary(s, at + rng.below(64));
            s.replace_range(at..end, "");
        }
        3 => {
            let end = boundary(s, at + rng.below(256));
            let copy = s[at..end].to_owned();
            s.insert_str(end, &copy);
        }
        4 => {
            let ending = ["\r\n", "\r", "\n"][rng.below(3)];
            *s = s.replace("\r\n", "\n").replace('\n', ending);
        }
        _ => {
            let prefix = ["> ", "\t", "- ", "1. ", "   ", "> - "][rng.below(6)];
            let lines: Vec<&str> = s.split_inclusive('\n').collect();
            let (from, n) = (rng.below(lines.len()), rng.below(8) + 1);
            *s = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    if (from..from + n).contains(&i) {
                        format!("{prefix}{l}")
                    } else {
                        (*l).to_owned()
                    }
                })
                .collect();
        }
    }
}

/// Returns whether the case produced a localized source-mapping fallback.
fn properties(case: usize, source: &str) -> bool {
    let d = match v1(source) {
        Ok(d) => d,
        Err(e) if limit(&e) => return false,
        Err(e) => panic!("case {case}: {source:?}: {e:?}"),
    };
    // Wire round trip revalidates and reproduces the exact model.
    let json = d.to_json().unwrap();
    let back = Document::from_json(&json, source, ModelLimits::default()).unwrap();
    assert_eq!(back.input(), d.input(), "case {case}: round trip");
    // Deterministic, and the cancellable entry point is identical.
    assert_eq!(v1(source).unwrap().to_json().unwrap(), json, "case {case}");
    let (di, ri) = ids();
    let token = CancellationToken::new();
    let c = parse_markdown_cancellable(source, di, ri, ParseLimits::default(), &token).unwrap();
    assert_eq!(c.to_json().unwrap(), json, "case {case}: cancellable");
    // Each source-mapping fallback is one inert root over its exact span, and
    // the add-time conflict check catches every case before validation does:
    // the whole-body safety net never fires.
    for n in &d.input().nodes {
        if matches!(&n.content, NodeKind::UnsupportedBlock { feature, .. } if feature == "unrepresentable_source_mapping")
        {
            assert!(
                d.input().roots.contains(&n.id),
                "case {case}: fallback root"
            );
        }
    }
    assert!(
        d.input()
            .diagnostics
            .iter()
            .all(|x| !x.message.contains("the body is preserved")),
        "case {case}: whole-body fallback: {source:?}"
    );
    let fallback = d
        .input()
        .diagnostics
        .iter()
        .any(|x| x.code == "source_mapping_fallback");
    let view = SearchDocument::from_v1(&d, source).unwrap();
    find(&view, "a", &FindOptions::default()).unwrap();
    read_stats(&view, &ReadOptions::default()).unwrap();
    #[cfg(feature = "model-v2")]
    {
        let (di, ri) = ids();
        let v2 = parse_markdown_v2(source, di, ri, ParseLimits::default())
            .unwrap_or_else(|e| panic!("v2 case {case}: {source:?}: {e:?}"));
        let back = v2::Document::from_json(&v2.to_json().unwrap(), source, ModelLimits::default())
            .unwrap();
        assert_eq!(back.input(), v2.input(), "v2 case {case}: round trip");
        // V2 differs from v1 only in reference intents, never in tree size
        // for documents without reference syntax.
        if d.input()
            .diagnostics
            .iter()
            .all(|x| x.code != "reference_link_deferred")
        {
            assert_eq!(v2.input().nodes.len(), d.input().nodes.len(), "case {case}");
        }
    }
    fallback
}

#[test]
fn corpus_seeded_mutations_keep_model_properties() {
    let cases: usize = std::env::var("MD_FUZZ_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4_000);
    let seeds = corpus();
    let mut rng = Rng(0x5eed_c0de_0b4f_u64);
    let mut fallbacks = 0;
    for case in 0..cases {
        let mut source = seeds[case % seeds.len()].clone();
        for _ in 0..rng.below(8) + 1 {
            mutate(&mut rng, &mut source);
        }
        fallbacks += usize::from(properties(case, &source));
    }
    eprintln!("{cases} mutated cases, {fallbacks} with a localized fallback");
    // The mutations reach the upstream tab-container conflict.
    assert!(fallbacks > 0);
}

/// Pathological constructs. Each must yield a validated model or a typed
/// limit; release builds also bound the time to catch superlinear behavior.
fn catalog() -> Vec<(&'static str, String)> {
    let n = 50_000;
    let rep = |s: &str, k: usize| s.repeat(k);
    vec![
        ("deep quotes", format!("{} x", rep(">", n))),
        ("deep quote lines", rep("> ", 200) + "x\n"),
        (
            "deep lists",
            (0..200).map(|i| format!("{}- x\n", rep("  ", i))).collect(),
        ),
        ("deep emphasis", format!("{}a{}", rep("*", n), rep("*", n))),
        ("open brackets", rep("[", n)),
        ("nested link openers", rep("[a](", n)),
        ("image openers", rep("![", n)),
        (
            "backtick runs",
            (1..400).map(|i| rep("`", i) + " ").collect(),
        ),
        ("unclosed comment", format!("<!--{}", rep("x -- ", n))),
        ("html block storm", rep("<div>\n", n)),
        ("entity storm", rep("&amp;&#x10FFFF;&#0;&bogus;", n / 4)),
        (
            "control characters",
            rep("a\u{0}\u{1}\u{7f}\u{85}\u{2028}", n / 4),
        ),
        (
            "bom and bidi",
            rep("\u{feff}\u{202e}\u{2066}x\u{200d}", n / 4),
        ),
        ("combining storm", format!("e{}", rep("\u{301}", n))),
        ("cr only", rep("# h\r- a\r> q\r", n / 4)),
        (
            "wide table",
            format!(
                "{}|\n{}|\n{}|\n",
                rep("|a", 4_000),
                rep("|-", 4_000),
                rep("|b", 4_000)
            ),
        ),
        (
            "long table",
            format!("|a|b|\n|-|-|\n{}", rep("|x|y|\n", n / 2)),
        ),
        (
            "footnote storm",
            format!("{}\n\n[^a]: x\n", rep("[^a]", n / 2)),
        ),
        (
            "footnote definitions",
            (0..5_000).map(|i| format!("[^{i}]: x\n")).collect(),
        ),
        ("duplicate footnotes", rep("[^a]: x\n\n", 5_000) + "[^a]"),
        (
            "reference definitions",
            rep("[a]: b.md\n", n / 4) + &rep("[a] ", n / 4),
        ),
        ("alert nesting", rep("> [!NOTE]\n", 100) + "x\n"),
        ("dollar storm", rep("$", n)),
        ("display math storm", rep("$$a$$ ", n / 4)),
        ("inline math storm", rep("$a$ ", n / 4)),
        ("heading storm", rep("#", n) + "\n" + &rep("# h\n", n / 4)),
        ("setext storm", rep("a\n=\n", n / 4)),
        ("thematic breaks", rep("***\n", n / 4)),
        ("autolink storm", rep("<http://a> <a@b.c> www.x.y ", n / 8)),
        // Bare GFM autolinks (CORE-003D): long URL-like runs, prefix storms and
        // trailing-delimiter scans must stay linear.
        (
            "bare autolink storm",
            rep("https://a.b/c www.d.e f@g.hi ", n / 4),
        ),
        (
            "long url run",
            format!("https://a.b/{}", rep("x_y*z", n * 2)),
        ),
        ("www prefixes", rep("www.", n * 2)),
        ("www underscores", rep("www._", n * 2)),
        ("at-sign storm", rep("a@", n * 2)),
        ("closing parens", format!("www.a.b/{}", rep(")", n * 4))),
        ("entity-like suffixes", format!("www.a.b/{}", rep("&a;", n))),
        ("tab containers", rep("1. >\t- a\n", n / 8)),
        (
            "front matter only",
            format!("---\n{}---\n", rep("k: v\n", n / 8)),
        ),
        ("fence storm", rep("```\n", n / 4)),
        ("indented code", rep("    x\n", n / 4)),
    ]
}

#[test]
fn hostile_constructs_stop_with_a_model_or_a_typed_limit() {
    let mut fallbacks = Vec::new();
    for (name, source) in catalog() {
        let start = Instant::now();
        match v1(&source) {
            Ok(d) => {
                if d.input()
                    .diagnostics
                    .iter()
                    .any(|x| x.code == "source_mapping_fallback")
                {
                    fallbacks.push(name);
                }
            }
            Err(e) => assert!(limit(&e), "{name}: {e:?}"),
        }
        #[cfg(feature = "model-v2")]
        {
            let (d, r) = ids();
            if let Err(e) = parse_markdown_v2(&source, d, r, ParseLimits::default()) {
                assert!(limit(&e), "v2 {name}: {e:?}");
            }
        }
        let elapsed = start.elapsed();
        eprintln!("{name}: {} bytes in {elapsed:?}", source.len());
        // Release builds: generous regression guard against superlinear cost,
        // not a performance budget (see docs/Parser-Budgets.md).
        if !cfg!(debug_assertions) {
            assert!(elapsed < Duration::from_secs(5), "{name}: {elapsed:?}");
        }
    }
    // Repeated tab containers are many top-level blocks; each one that hits
    // the upstream offset conflict falls back alone.
    assert_eq!(fallbacks, ["tab containers"]);
}

#[test]
fn performance_profiles_parse_under_default_limits() {
    for p in &profiles::PROFILES {
        let source = profiles::profile_source(p);
        let d = v1(&source).unwrap_or_else(|e| panic!("{}: {e:?}", p.id));
        assert!(
            d.input()
                .diagnostics
                .iter()
                .all(|x| x.code != "source_mapping_fallback")
        );
        #[cfg(feature = "model-v2")]
        {
            let (di, ri) = ids();
            let v2 = parse_markdown_v2(&source, di, ri, ParseLimits::default()).unwrap();
            assert_eq!(v2.input().nodes.len(), d.input().nodes.len(), "{}", p.id);
        }
        // A pre-cancelled token stops before any work.
        let (di, ri) = ids();
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            parse_markdown_cancellable(&source, di, ri, ParseLimits::default(), &token)
                .unwrap_err(),
            CancellableParseError::Cancelled
        );
    }
}

#[test]
fn source_ceiling_and_dense_markup_stop_with_typed_limits() {
    let limits = ParseLimits::default();
    let over = "a".repeat(limits.model.max_source_bytes + 1);
    assert_eq!(v1(&over).unwrap_err(), ParseError::SourceLimit);
    // Representative prose at the source ceiling fits the default limits.
    let prose = profiles::generate("prose-sections-v1", limits.model.max_source_bytes - 1, 0);
    let prose = String::from_utf8(prose).unwrap();
    assert!(v1(&prose).is_ok());
    // Dollar-dense prose costs no extra events: `$` splits are rejoined first.
    let line = "Costs $5 and $10, or $7.50.\n";
    let dollars = line.repeat(limits.model.max_source_bytes / line.len());
    assert!(v1(&dollars).is_ok());
    // Dense hostile markup at the ceiling stops with a typed node limit.
    let storm = profiles::generate("delimiter-storm-v1", limits.model.max_source_bytes, 0);
    let storm = String::from_utf8(storm).unwrap();
    assert_eq!(v1(&storm).unwrap_err(), ParseError::NodeLimit);
}
