//! Bare GFM extended autolinks (CORE-003D): `http(s)://`, `www.` and email.
//!
//! Detection follows cmark-gfm's autolink extension, which GitHub renders with,
//! over one run of exact source text: contiguous text events with no escape,
//! entity or other construct, outside links, images and code. The parser and
//! the v2 provenance pass both call [`find`], so each proves the other's spans.
//! Differences from cmark-gfm: `ftp://` and the `mailto:`/`xmpp:` forms are not
//! detected, and any Unicode whitespace ends a link.
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Url,
    Www,
    Email,
}

/// The semantic destination; the lexeme itself is the link's exact source span.
pub(crate) fn destination(lexeme: &str, kind: Kind) -> String {
    match kind {
        Kind::Url => lexeme.to_owned(),
        Kind::Www => format!("http://{lexeme}"),
        Kind::Email => format!("mailto:{lexeme}"),
    }
}

/// Autolinks in `run`, as byte ranges in order. `before` is the source character
/// preceding the run. Linear in the run length: a candidate that fails resumes
/// past whatever it scanned, and links never overlap.
pub(crate) fn find(run: &str, before: Option<char>) -> Vec<(Range<usize>, Kind)> {
    let b = run.as_bytes();
    let mut found = Vec::new();
    let mut floor = 0; // email local parts never reach back into a previous link
    let mut i = 0;
    // Only called at ASCII bytes, which are always character boundaries.
    let prev = |i: usize| {
        if i == 0 {
            before
        } else {
            run[..i].chars().next_back()
        }
    };
    while i < b.len() {
        let result = match b[i] {
            b'h' | b'H' => url(run, i, prev(i)),
            b'w' => www(run, i, prev(i)),
            b'@' => email(run, i, floor),
            _ => Err(i + 1),
        };
        match result {
            Ok((range, kind)) => {
                i = range.end;
                floor = range.end;
                found.push((range, kind));
            }
            Err(next) => i = next.max(i + 1),
        }
    }
    found
}

type Scan = Result<(Range<usize>, Kind), usize>;

fn url(run: &str, start: usize, prev: Option<char>) -> Scan {
    // cmark-gfm rewinds over letters to find the scheme, so `xhttp://` is no link.
    if prev.is_some_and(|c| c.is_ascii_alphabetic()) {
        return Err(start + 1);
    }
    let rest = &run[start..];
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|s| {
            rest.get(..s.len())
                .is_some_and(|p| p.eq_ignore_ascii_case(s))
        })
        .ok_or(start + 1)?;
    let host = start + scheme.len();
    let end = domain(run, host, true).ok_or_else(|| resume(run, host))?;
    let end = delimit(run, start, path(run, end));
    if end <= host {
        return Err(host);
    }
    Ok((start..end, Kind::Url))
}

fn www(run: &str, start: usize, prev: Option<char>) -> Scan {
    if !run[start..].starts_with("www.")
        || prev.is_some_and(|c| !(c.is_whitespace() || matches!(c, '*' | '_' | '~' | '(')))
    {
        return Err(start + 1);
    }
    let end = domain(run, start, false).ok_or_else(|| resume(run, start))?;
    let end = delimit(run, start, path(run, end));
    if end <= start + 4 {
        return Err(start + 4);
    }
    Ok((start..end, Kind::Www))
}

fn email(run: &str, at: usize, floor: usize) -> Scan {
    let b = run.as_bytes();
    let local = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'-' | b'_');
    let mut start = at;
    while start > floor && local(b[start - 1]) {
        start -= 1;
    }
    if start == at {
        return Err(at + 1);
    }
    // Domain: alphanumerics, `-` and `_`, and `.` only before an alphanumeric.
    // A second `@` fails the candidate, so scans never pass the next `@`.
    let mut end = at + 1;
    let mut dots = 0;
    while end < b.len() {
        let c = b[end];
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_') {
            end += 1;
        } else if c == b'.' && b.get(end + 1).is_some_and(u8::is_ascii_alphanumeric) {
            dots += 1;
            end += 1;
        } else if c == b'@' {
            return Err(end);
        } else {
            break;
        }
    }
    let last = b[end - 1];
    if end < at + 2 || dots == 0 || !(last.is_ascii_alphabetic() || last == b'.') {
        return Err(end.max(at + 1));
    }
    let end = delimit(run, start, end);
    if end <= at + 1 {
        return Err(at + 1);
    }
    Ok((start..end, Kind::Email))
}

/// End of a valid domain starting at `start`: host characters and dots, no
/// underscore in the last two labels, and a dot unless `short` is allowed.
/// Labels are judged without trailing dots, which [`delimit`] drops.
fn domain(run: &str, start: usize, short: bool) -> Option<usize> {
    let end = domain_end(run, start);
    let host = run[start..end].trim_end_matches('.');
    let clean = host.rsplit('.').take(2).all(|label| !label.contains('_'));
    let valid = !host.is_empty() && clean && (short || host.contains('.'));
    valid.then_some(end)
}
fn domain_end(run: &str, start: usize) -> usize {
    run[start..]
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-')))
        .map_or(run.len(), |(i, _)| start + i)
}

/// Where to resume after a failed domain. A `www.` or email candidate inside
/// it would share its final labels and fail too, but a scheme can start in
/// its last five characters (`https` before the `:` that ended it).
fn resume(run: &str, start: usize) -> usize {
    domain_end(run, start).saturating_sub(5).max(start + 1)
}

/// A link continues to whitespace or `<`.
fn path(run: &str, from: usize) -> usize {
    run[from..]
        .char_indices()
        .find(|&(_, c)| c.is_whitespace() || c == '<')
        .map_or(run.len(), |(i, _)| from + i)
}

/// GFM trailing-delimiter rules: drop trailing `?!.,:*_~'"`, a trailing
/// entity-like `&name;` (or a lone `;`), and each `)` that is unbalanced
/// within the link. Stripping never removes a `(`, so the counts are taken
/// once and this stays linear.
fn delimit(run: &str, start: usize, mut end: usize) -> usize {
    let b = run.as_bytes();
    let open = b[start..end].iter().filter(|&&c| c == b'(').count();
    let mut close = b[start..end].iter().filter(|&&c| c == b')').count();
    while end > start {
        match b[end - 1] {
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => end -= 1,
            b';' => {
                let mut name = end - 1;
                while name > start && b[name - 1].is_ascii_alphanumeric() {
                    name -= 1;
                }
                end = if name < end - 1 && name > start && b[name - 1] == b'&' {
                    name - 1
                } else {
                    end - 1
                };
            }
            b')' if close > open => {
                close -= 1;
                end -= 1;
            }
            _ => break,
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(text: &str) -> Vec<&str> {
        find(text, None)
            .into_iter()
            .map(|(r, _)| &text[r])
            .collect()
    }

    #[test]
    fn gfm_spec_examples() {
        assert_eq!(links("www.commonmark.org"), ["www.commonmark.org"]);
        assert_eq!(
            links("Visit www.commonmark.org/help for more information."),
            ["www.commonmark.org/help"]
        );
        assert_eq!(links("Visit www.commonmark.org."), ["www.commonmark.org"]);
        assert_eq!(
            links("Visit www.commonmark.org/a.b."),
            ["www.commonmark.org/a.b"]
        );
        assert_eq!(
            links("www.google.com/search?q=Markup+(business)"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("www.google.com/search?q=Markup+(business)))"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("(www.google.com/search?q=Markup+(business))"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("(www.google.com/search?q=Markup+(business)"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("www.google.com/search?q=(business))+ok"),
            ["www.google.com/search?q=(business))+ok"]
        );
        assert_eq!(
            links("www.google.com/search?q=commonmark&hl=en"),
            ["www.google.com/search?q=commonmark&hl=en"]
        );
        assert_eq!(
            links("www.google.com/search?q=commonmark&hl;"),
            ["www.google.com/search?q=commonmark"]
        );
        assert_eq!(links("www.commonmark.org/he<lp"), ["www.commonmark.org/he"]);
        assert_eq!(links("http://commonmark.org"), ["http://commonmark.org"]);
        assert_eq!(
            links("(Visit https://encrypted.google.com/search?q=Markup+(business))"),
            ["https://encrypted.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(links("foo@bar.baz"), ["foo@bar.baz"]);
        assert_eq!(
            links("hello@mail+xyz.example isn't valid, but hello+xyz@mail.example is."),
            ["hello+xyz@mail.example"]
        );
        assert_eq!(links("a.b-c_d@a.b"), ["a.b-c_d@a.b"]);
        assert_eq!(links("a.b-c_d@a.b."), ["a.b-c_d@a.b"]);
        assert!(links("a.b-c_d@a.b-").is_empty());
        assert!(links("a.b-c_d@a.b_").is_empty());
    }

    #[test]
    fn domains_and_boundaries() {
        assert!(
            links("www.a_b.c_d").is_empty(),
            "underscore in the last two labels"
        );
        assert_eq!(links("www.a_b.c.d"), ["www.a_b.c.d"]);
        // A trailing dot is dropped, so it cannot hide an underscore (PR #24 review).
        assert!(links("www.example_.com.").is_empty());
        assert!(links("http://ex_ample.com.").is_empty());
        // A failed domain does not hide a scheme that starts inside it.
        assert_eq!(
            links("http://bad_.http://good.example/x"),
            ["http://good.example/x"]
        );
        assert_eq!(links("www.a_.x-https://b.example"), ["https://b.example"]);
        assert!(links("www.").is_empty());
        assert!(links("awww.example.com").is_empty(), "www needs a boundary");
        assert_eq!(links("*www.example.com*"), ["www.example.com"]);
        assert!(links("xhttp://example.com").is_empty());
        assert_eq!(links("1http://example.com"), ["http://example.com"]);
        assert_eq!(
            links("http://localhost:8080/x"),
            ["http://localhost:8080/x"]
        );
        assert_eq!(links("HTTPS://Example.com"), ["HTTPS://Example.com"]);
        assert!(links("http://").is_empty());
        assert!(links("ftp://example.com").is_empty());
        assert_eq!(
            links("https://例え.テスト/パス"),
            ["https://例え.テスト/パス"]
        );
        assert_eq!(links("see https://a.com\u{a0}next"), ["https://a.com"]);
        assert_eq!(
            links("a https://a.com, b www.b.com; c@d.ef!"),
            ["https://a.com", "www.b.com", "c@d.ef"]
        );
        assert_eq!(find("www.a.com", Some('x')), vec![]);
        assert_eq!(find("www.a.com", Some('(')), vec![(0..9, Kind::Www)]);
    }

    #[test]
    fn hostile_runs_stay_linear() {
        for text in [
            "www._".repeat(200_000),
            "www.".repeat(200_000),
            "a@".repeat(200_000),
            "http://a_".repeat(100_000),
            format!("https://a.b/{}", ")".repeat(500_000)),
            format!("https://a.b/{}", "&a;".repeat(200_000)),
        ] {
            let start = std::time::Instant::now();
            let _ = find(&text, None);
            assert!(start.elapsed().as_secs() < 2, "{}", &text[..12]);
        }
    }
}
