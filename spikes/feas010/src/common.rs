//! Engine-independent semantic passes over a small intermediate form.
//!
//! Each parser adapter fills a `Raw` record with byte spans into the original
//! source. Everything product-specific (front matter values, slugs, link
//! policy, alert validation, voice-over script syntax and span checks) lives
//! here, so the two adapters differ only in how they walk their parser output.

use serde::Serialize;
use std::collections::BTreeMap;
use std::ops::Range;

pub type Span = Range<usize>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathForm {
    Inline,
    Display,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvColumn {
    Visual,
    Audio,
}

/// One inline item of a block, in source order.
#[derive(Clone, Debug)]
pub enum Inl {
    /// Literal text. Adjacent pieces are merged when only escapes separate them.
    Text(Span),
    /// A strong run with its plain text, used for speaker labels.
    Strong { span: Span, text: String },
    /// A soft or hard line break.
    Break,
    /// Code, math, links, HTML and other inline content voice syntax ignores.
    Opaque,
}

#[derive(Clone, Debug)]
pub enum BlockKind {
    Paragraph,
    Heading,
    /// A table body cell; `av` names its column in a Visual/Audio table.
    Cell {
        av: Option<AvColumn>,
    },
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub span: Span,
    pub inl: Vec<Inl>,
}

impl Block {
    pub fn new(kind: BlockKind, span: Span) -> Self {
        Block {
            kind,
            span,
            inl: Vec::new(),
        }
    }

    /// Appends text, merging with the previous piece across escape-only gaps.
    pub fn push_text(&mut self, source: &str, span: Span) {
        if span.is_empty() {
            return;
        }
        if let Some(Inl::Text(prev)) = self.inl.last_mut() {
            let gap = &source.as_bytes()[prev.end.min(span.start)..span.start];
            if span.start >= prev.end && gap.iter().all(|&b| b == b'\\') {
                prev.end = span.end;
                return;
            }
        }
        self.inl.push(Inl::Text(span));
    }
}

/// What a parser adapter extracts. All spans are byte ranges into the source.
#[derive(Default, Debug)]
pub struct Raw {
    pub front_matter: Option<Span>,
    pub headings: Vec<(u8, String, Span)>,
    pub fences: Vec<(Option<String>, Span)>,
    pub links: Vec<(String, Span)>,
    pub alerts: Vec<(String, Span)>,
    pub blockquotes: Vec<Span>,
    pub math: Vec<(MathForm, Span)>,
    pub tables: usize,
    pub task_items: usize,
    pub footnote_definitions: Vec<Span>,
    pub html_constructs: usize,
    /// Tables whose header names Visual and Audio columns.
    pub av_tables: usize,
    pub blocks: Vec<Block>,
}

#[derive(Serialize, Default)]
pub struct Semantics {
    pub front_matter: FrontMatter,
    pub outline: Vec<Heading>,
    pub feature_counts: BTreeMap<&'static str, usize>,
    pub code_fences: Vec<Fence>,
    pub links: Vec<Link>,
    pub alerts: Vec<String>,
    pub math: Vec<Math>,
    pub voice: Option<Voice>,
    pub span_checks: SpanChecks,
}

#[derive(Serialize, Default)]
pub struct FrontMatter {
    pub present: bool,
    /// `null` when the block is not the simple subset (shown raw by the app).
    pub properties: Option<BTreeMap<String, serde_json::Value>>,
    pub span: Option<[usize; 2]>,
}

#[derive(Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub slug: String,
    pub span: [usize; 2],
}

#[derive(Serialize)]
pub struct Fence {
    pub language: Option<String>,
    pub semantic: &'static str,
    pub span: [usize; 2],
}

#[derive(Serialize)]
pub struct Link {
    pub target: String,
    pub kind: &'static str,
    pub span: [usize; 2],
}

#[derive(Serialize)]
pub struct Math {
    pub form: &'static str,
    pub span: [usize; 2],
}

#[derive(Serialize, Default)]
pub struct Voice {
    pub speakers: Vec<String>,
    pub lines: Vec<VoiceLine>,
    pub directions: Vec<Mark>,
    pub notes: Vec<Mark>,
    pub pronunciations: Vec<Pronunciation>,
    pub av_tables: usize,
}

#[derive(Serialize)]
pub struct VoiceLine {
    pub speaker: Option<String>,
    pub line: usize,
    pub span: [usize; 2],
    pub labelled: bool,
}

#[derive(Serialize)]
pub struct Mark {
    pub form: &'static str,
    pub text: String,
    pub span: [usize; 2],
}

#[derive(Serialize)]
pub struct Pronunciation {
    pub word: String,
    pub reading: String,
    pub span: [usize; 2],
}

#[derive(Serialize, Default)]
pub struct SpanChecks {
    pub checked: usize,
    pub failures: Vec<String>,
}

fn arr(s: &Span) -> [usize; 2] {
    [s.start, s.end]
}

/// Byte offsets where each line starts, built once per document.
pub fn line_starts(source: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// 1-based line of a byte offset.
pub fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|&s| s <= offset)
}

/// Longest direction, note or pronunciation the scanner looks for. Bounding
/// the search for a closing marker keeps scanning linear on hostile input;
/// anything longer stays plain text.
const MAX_MARK: usize = 256;

/// `s` cut to at most `MAX_MARK` bytes on a character boundary.
fn window(s: &str) -> &str {
    let mut end = s.len().min(MAX_MARK);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ---------------------------------------------------------------------------
// Front matter: the markdown-viewer subset, with typed plain scalars.

/// The front matter block, fences included: `---` on the first line and a
/// closing `---` line (trailing spaces or tabs allowed), as remark-frontmatter
/// and markdown-viewer require. Anywhere else, `---` is ordinary Markdown.
pub fn front_matter_span(source: &str) -> Option<Span> {
    let first = source.lines().next()?;
    if first.trim_end_matches([' ', '\t', '\r']) != "---" {
        return None;
    }
    let mut offset = source.find('\n')? + 1;
    for line in source[offset..].split_inclusive('\n') {
        let end = offset + line.len();
        if line
            .trim_end_matches(['\r', '\n'])
            .trim_end_matches([' ', '\t'])
            == "---"
        {
            return Some(0..end);
        }
        offset = end;
    }
    None
}

/// Parses the body between `---` fences, or returns `None` for anything
/// outside the simple subset: top-level keys with plain or quoted scalars and
/// flow or block lists. Anchors, tags, block scalars, nested maps, duplicate
/// keys and tabs are rejected, so aliases can't expand.
pub fn parse_front_matter(body: &str) -> Option<BTreeMap<String, serde_json::Value>> {
    let lines: Vec<&str> = body.lines().collect();
    if lines.iter().any(|l| l.len() > 10_000 || l.contains('\t')) {
        return None;
    }
    let mut props = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with(' ') {
            return None;
        }
        let colon = line
            .find(": ")
            .or_else(|| line.strip_suffix(':').map(|s| s.len()))?;
        let key = line[..colon].trim_end();
        if key.is_empty() || "-?:,[]{}#&*!|>'\"%@`".contains(&key[..1]) {
            return None;
        }
        let rest = strip_comment(line[colon + 1..].trim());
        let value = if rest.is_empty() {
            let mut items = Vec::new();
            while i < lines.len() {
                let next = lines[i].trim_start();
                if lines[i].trim().is_empty() {
                    i += 1;
                    continue;
                }
                let Some(item) =
                    next.strip_prefix("- ")
                        .or(if next == "-" { Some("") } else { None })
                else {
                    break;
                };
                if !lines[i].starts_with(' ') && !lines[i].starts_with('-') {
                    break;
                }
                items.push(typed_scalar(item)?);
                i += 1;
            }
            if i < lines.len() && lines[i].starts_with(' ') {
                return None;
            }
            if items.is_empty() {
                serde_json::Value::String(String::new())
            } else {
                serde_json::Value::Array(items)
            }
        } else if let Some(inner) = rest.strip_prefix('[') {
            let inner = inner.strip_suffix(']')?;
            if inner.contains(['[', ']', '{', '}']) {
                return None;
            }
            let items: Option<Vec<_>> = inner
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(typed_scalar)
                .collect();
            serde_json::Value::Array(items?)
        } else {
            typed_scalar(rest)?
        };
        if props.insert(key.to_string(), value).is_some() {
            return None;
        }
    }
    (!props.is_empty()).then_some(props)
}

fn strip_comment(s: &str) -> &str {
    match s.find(" #") {
        Some(h) if !s.starts_with(['"', '\'']) => s[..h].trim_end(),
        _ => s,
    }
}

/// A YAML core-schema scalar: booleans, null and integers are typed; quoted
/// and other plain values stay strings. Indicators that start anchors, tags,
/// block scalars and flow maps are rejected.
fn typed_scalar(s: &str) -> Option<serde_json::Value> {
    use serde_json::Value;
    let s = s.trim();
    if let Some(body) = s.strip_prefix('"').and_then(|b| b.strip_suffix('"')) {
        return Some(Value::String(body.replace("\\\"", "\"")));
    }
    if let Some(body) = s.strip_prefix('\'').and_then(|b| b.strip_suffix('\'')) {
        return Some(Value::String(body.replace("''", "'")));
    }
    if s.starts_with(['&', '*', '!', '|', '>', '{', '%', '@', '`']) {
        return None;
    }
    Some(match s {
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        "null" | "~" | "" => Value::Null,
        _ => match s.parse::<i64>() {
            Ok(n) => Value::from(n),
            Err(_) => Value::String(s.to_string()),
        },
    })
}

// ---------------------------------------------------------------------------
// Headings, links, fences, alerts.

/// GitHub-style heading slug: lowercase, keep letters, digits, `-`, `_` and
/// spaces, then spaces become hyphens. Duplicates get `-1`, `-2`, ...
pub fn slug(text: &str, seen: &mut BTreeMap<String, usize>) -> String {
    let base: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect();
    let n = seen.entry(base.clone()).or_insert(0);
    let out = if *n == 0 {
        base.clone()
    } else {
        format!("{base}-{n}")
    };
    *n += 1;
    out
}

const BLOCKED_SCHEMES: [&str; 3] = ["javascript", "vbscript", "data"];

pub fn link_kind(target: &str) -> &'static str {
    let t: String = target
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    if t.starts_with('#') {
        return "fragment";
    }
    let scheme_end = t.find(':').filter(|&i| {
        i > 0
            && t[..i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
            && t.as_bytes()[0].is_ascii_alphabetic()
    });
    match scheme_end.map(|i| t[..i].to_ascii_lowercase()) {
        None => "relative",
        Some(s) if BLOCKED_SCHEMES.contains(&s.as_str()) => "blocked",
        Some(s) if matches!(s.as_str(), "http" | "https" | "mailto") => "external",
        // Unknown schemes are not activated by default.
        Some(_) => "blocked",
    }
}

fn fence_semantic(lang: Option<&str>) -> &'static str {
    match lang {
        Some("math") => "math",
        Some("mermaid") | Some("d2") => "diagram",
        _ => "code",
    }
}

/// GitHub's rule, as in markdown-viewer: the marker must be alone on the
/// quote's first line and followed by content.
fn valid_alert(source: &str, span: &Span) -> Option<String> {
    let text = &source[span.clone()];
    let mut lines = text.lines();
    let first = lines.next()?.trim_start().strip_prefix('>')?.trim();
    let kind = first
        .strip_prefix("[!")?
        .strip_suffix(']')?
        .to_ascii_uppercase();
    let known = ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];
    let has_body = lines.any(|l| !l.trim_start().trim_start_matches('>').trim().is_empty());
    (known.contains(&kind.as_str()) && has_body).then_some(kind)
}

// ---------------------------------------------------------------------------
// Voice-over script syntax (markdown-viewer VO0 section 12).

fn script_mode(fm: &Option<BTreeMap<String, serde_json::Value>>, raw_body: Option<&str>) -> bool {
    if let Some(v) = fm.as_ref().and_then(|p| p.get("mdv")) {
        return v.as_str() == Some("script");
    }
    // E1 fallback: a line scan when the block isn't the simple subset.
    raw_body.is_some_and(|b| {
        b.lines()
            .any(|l| l.trim_end() == "mdv: script" || l.starts_with("mdv: script "))
    })
}

fn speaker_name(label: &str, listed: &[String]) -> Option<String> {
    let name = label.trim().strip_suffix(':')?.trim();
    if name.is_empty() || name.chars().count() > 32 {
        return None;
    }
    (!name.chars().any(char::is_lowercase) || listed.iter().any(|l| l == name))
        .then(|| name.to_string())
}

/// `[[note]]` at the start of `rest`: the inner text and the matched length.
fn note_at(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix("[[")?;
    let end = body.find("]]")?;
    Some((&body[..end], end + 4))
}

/// `[direction]` at the start of `rest`, excluding empty, digit-only and
/// `[sic]` brackets (VO0 E3).
fn direction_at(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix('[')?;
    let end = body.find([']', '['])?;
    let inner = &body[..end];
    let excluded = body.as_bytes()[end] != b']'
        || inner.trim().is_empty()
        || inner.chars().all(|c| c.is_ascii_digit())
        || inner.eq_ignore_ascii_case("sic");
    (!excluded).then_some((inner, end + 2))
}

/// `{word|reading}` (or `{word\\|reading}` inside tables) at the start of `rest`.
fn pronunciation_at(rest: &str) -> Option<(&str, &str, usize)> {
    let body = rest.strip_prefix('{')?;
    let end = body.find('}')?;
    let inner = &body[..end];
    let (p, w) = inner
        .find("\\|")
        .map(|p| (p, 2))
        .or_else(|| inner.find('|').map(|p| (p, 1)))?;
    let (word, reading) = (&inner[..p], &inner[p + w..]);
    (!word.is_empty() && !reading.is_empty() && !inner.contains('{')).then_some((
        word,
        reading,
        end + 2,
    ))
}

/// Scans one text span for notes, directions and pronunciations. `line_start`
/// is true when the span begins a line, so a leading `(text)` counts.
fn scan_voice(source: &str, span: &Span, line_start: bool, v: &mut Voice) {
    let mut i = span.start;
    if line_start {
        let text = &source[span.clone()];
        let lead = i + text.len() - text.trim_start().len();
        if let Some(close) = window(&source[lead..span.end])
            .strip_prefix('(')
            .and_then(|r| r.find(')'))
        {
            let s = lead..lead + close + 2;
            v.directions.push(Mark {
                form: "leading-parenthetical",
                text: source[lead + 1..s.end - 1].into(),
                span: arr(&s),
            });
            i = s.end;
        }
    }
    while i < span.end {
        let rest = window(&source[i..span.end]);
        if let Some((text, len)) = note_at(rest) {
            v.notes.push(Mark {
                form: "double-bracket",
                text: text.into(),
                span: [i, i + len],
            });
            i += len;
        } else if let Some((text, len)) = direction_at(rest) {
            v.directions.push(Mark {
                form: "bracket",
                text: text.into(),
                span: [i, i + len],
            });
            i += len;
        } else if let Some((word, reading, len)) = pronunciation_at(rest) {
            v.pronunciations.push(Pronunciation {
                word: word.into(),
                reading: reading.into(),
                span: [i, i + len],
            });
            i += len;
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
}

fn voice_pass(source: &str, raw: &Raw, listed: &[String]) -> Voice {
    let starts = line_starts(source);
    let mut v = Voice::default();
    let mut current: Option<String> = None;
    for block in &raw.blocks {
        match block.kind {
            BlockKind::Heading => {
                current = None;
                continue;
            }
            // Visual cells are never spoken.
            BlockKind::Cell {
                av: Some(AvColumn::Visual),
            } => continue,
            _ => {}
        }
        let mut line_start = true;
        let mut line_open = false;
        for item in &block.inl {
            match item {
                Inl::Break => {
                    line_start = true;
                    line_open = false;
                }
                Inl::Strong { span, text } if line_start => {
                    if let Some(name) = speaker_name(text, listed) {
                        if !v.speakers.contains(&name) {
                            v.speakers.push(name.clone());
                        }
                        current = Some(name);
                        v.lines.push(VoiceLine {
                            speaker: current.clone(),
                            line: line_of(&starts, span.start),
                            span: arr(span),
                            labelled: true,
                        });
                        line_open = true;
                        // A parenthetical may follow the label on the same line.
                        continue;
                    }
                    line_start = false;
                }
                Inl::Text(span) => {
                    if !line_open {
                        v.lines.push(VoiceLine {
                            speaker: current.clone(),
                            line: line_of(&starts, span.start),
                            span: arr(span),
                            labelled: false,
                        });
                        line_open = true;
                    }
                    scan_voice(source, span, line_start, &mut v);
                    line_start = false;
                }
                _ => line_start = false,
            }
        }
    }
    v.av_tables = raw.av_tables;
    v
}

// ---------------------------------------------------------------------------
// Assembly and span checks.

pub fn finish(source: &str, raw: Raw, force_script: bool) -> Semantics {
    let mut sem = Semantics::default();
    let mut checks = SpanChecks::default();
    let mut check = |ok: bool, what: String| {
        checks.checked += 1;
        if !ok {
            checks.failures.push(what);
        }
    };

    let mut raw_body = None;
    if let Some(span) = &raw.front_matter {
        let block = &source[span.clone()];
        check(block.starts_with("---"), format!("front matter {span:?}"));
        let body = block
            .trim_start_matches("---")
            .trim_start_matches(['\r', '\n']);
        let body = body
            .trim_end()
            .trim_end_matches("---")
            .trim_end_matches("...");
        raw_body = Some(body);
        sem.front_matter = FrontMatter {
            present: true,
            properties: parse_front_matter(body),
            span: Some(arr(span)),
        };
    }

    let mut seen = BTreeMap::new();
    for (level, text, span) in &raw.headings {
        check(
            source[span.clone()].trim_start().starts_with('#'),
            format!("heading {span:?}"),
        );
        sem.outline.push(Heading {
            level: *level,
            text: text.clone(),
            slug: slug(text, &mut seen),
            span: arr(span),
        });
    }
    for (lang, span) in &raw.fences {
        let s = &source[span.clone()];
        let first = s.lines().next().unwrap_or("");
        let ok = (first.trim_start().starts_with("```") || first.trim_start().starts_with("~~~"))
            && lang.as_deref().is_none_or(|l| first.contains(l));
        check(ok, format!("fence {span:?}"));
        sem.code_fences.push(Fence {
            language: lang.clone(),
            semantic: fence_semantic(lang.as_deref()),
            span: arr(span),
        });
    }
    for (target, span) in &raw.links {
        check(
            source[span.clone()].contains(target.as_str()),
            format!("link {target} {span:?}"),
        );
        sem.links.push(Link {
            target: target.clone(),
            kind: link_kind(target),
            span: arr(span),
        });
    }
    let mut plain_quotes = raw.blockquotes.len();
    for (kind, span) in &raw.alerts {
        match valid_alert(source, span) {
            Some(k) => {
                check(k == *kind, format!("alert {kind} {span:?}"));
                sem.alerts.push(k);
            }
            // The parser accepted a look-alike: treat it as a plain quote.
            None => plain_quotes += 1,
        }
    }
    for span in &raw.blockquotes {
        // A parser that doesn't validate markers can't tell us; recheck here.
        if let Some(k) = valid_alert(source, span) {
            sem.alerts.push(k);
            plain_quotes -= 1;
        }
    }
    let (mut inline, mut display) = (0, 0);
    for (form, span) in &raw.math {
        let s = &source[span.clone()];
        let ok = match form {
            MathForm::Inline => s.starts_with('$') && s.ends_with('$') && !s.starts_with("$$"),
            MathForm::Display => s.starts_with("$$") && s.trim_end().ends_with("$$"),
        };
        check(ok, format!("math {form:?} {span:?}"));
        match form {
            MathForm::Inline => inline += 1,
            MathForm::Display => display += 1,
        }
        sem.math.push(Math {
            form: if *form == MathForm::Inline {
                "inline"
            } else {
                "display"
            },
            span: arr(span),
        });
    }
    for span in &raw.footnote_definitions {
        check(
            source[span.clone()].starts_with("[^"),
            format!("footnote {span:?}"),
        );
    }

    let listed: Vec<String> = sem
        .front_matter
        .properties
        .as_ref()
        .and_then(|p| p.get("speakers"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if force_script || script_mode(&sem.front_matter.properties, raw_body) {
        let v = voice_pass(source, &raw, &listed);
        for m in v.directions.iter().chain(&v.notes) {
            let s = &source[m.span[0]..m.span[1]];
            let ok = match m.form {
                "bracket" => s == format!("[{}]", m.text),
                "double-bracket" => s == format!("[[{}]]", m.text),
                _ => s == format!("({})", m.text),
            };
            check(ok, format!("voice {} {:?}", m.form, m.span));
        }
        for p in &v.pronunciations {
            let s = &source[p.span[0]..p.span[1]];
            check(
                s.starts_with('{') && s.ends_with('}') && s.contains(&p.word),
                format!("pron {:?}", p.span),
            );
        }
        for l in v.lines.iter().filter(|l| l.labelled) {
            let s = &source[l.span[0]..l.span[1]];
            check(
                s.starts_with("**") && s.ends_with(":**"),
                format!("label {:?}", l.span),
            );
        }
        sem.voice = Some(v);
    }

    let fc = &mut sem.feature_counts;
    fc.insert("headings", raw.headings.len());
    fc.insert("tables", raw.tables);
    fc.insert("task_items", raw.task_items);
    fc.insert("alerts", sem.alerts.len());
    fc.insert("blockquotes", plain_quotes);
    fc.insert("code_fences", raw.fences.len());
    fc.insert(
        "diagram_fences",
        sem.code_fences
            .iter()
            .filter(|f| f.semantic == "diagram")
            .count(),
    );
    fc.insert(
        "math_fences",
        sem.code_fences
            .iter()
            .filter(|f| f.semantic == "math")
            .count(),
    );
    fc.insert("math_inline", inline);
    fc.insert("math_display", display);
    fc.insert("footnote_definitions", raw.footnote_definitions.len());
    fc.insert("raw_html_constructs", raw.html_constructs);
    fc.insert("links", raw.links.len());
    if let Some(v) = &sem.voice {
        fc.insert("notes", v.notes.len());
        fc.insert("pronunciation_guides", v.pronunciations.len());
    }
    sem.span_checks = checks;
    sem
}
