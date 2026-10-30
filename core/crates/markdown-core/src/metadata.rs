//! A deliberately restricted front-matter grammar, not a general YAML loader.
use crate::MetadataValue;
use std::collections::BTreeMap;

pub(crate) const MAX_BYTES: usize = 64 * 1024;
const MAX_PROPERTIES: usize = 256;
const MAX_ITEMS: usize = 1024;
const MAX_SCALAR_BYTES: usize = 16 * 1024;
const MAX_KEY_BYTES: usize = 128;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Unsupported,
    Duplicate,
    Limit,
    TextLimit,
}
struct Context<'a> {
    copied: &'a mut usize,
    max_text: usize,
    items: usize,
}
impl Context<'_> {
    fn own(&mut self, value: &str) -> Result<String, Error> {
        if !display_text(value) {
            return Err(Error::Unsupported);
        }
        *self.copied = self
            .copied
            .checked_add(value.len())
            .ok_or(Error::TextLimit)?;
        if *self.copied > self.max_text {
            return Err(Error::TextLimit);
        }
        Ok(value.into())
    }
    fn item(&mut self, raw: &str) -> Result<String, Error> {
        if self.items >= MAX_ITEMS {
            return Err(Error::Limit);
        }
        self.items += 1;
        match self.scalar(raw)? {
            MetadataValue::String { value } => Ok(value),
            _ => Err(Error::Unsupported),
        }
    }
    fn scalar(&mut self, raw: &str) -> Result<MetadataValue, Error> {
        let raw = raw.trim_matches(' ');
        if raw.len() > MAX_SCALAR_BYTES {
            return Err(Error::Limit);
        }
        if raw.starts_with(['\'', '"']) {
            let quote = raw.as_bytes()[0];
            let mut pos = 1;
            let bytes = raw.as_bytes();
            let end = loop {
                if pos >= bytes.len() {
                    return Err(Error::Unsupported);
                }
                if quote == b'"' && bytes[pos] == b'\\' {
                    pos += 2;
                    continue;
                }
                if bytes[pos] == quote {
                    if quote == b'\'' && bytes.get(pos + 1) == Some(&b'\'') {
                        pos += 2;
                        continue;
                    }
                    break pos + 1;
                }
                pos += 1;
            };
            if !comment(raw.get(end..).ok_or(Error::Unsupported)?)?.is_empty() {
                return Err(Error::Unsupported);
            }
            let value = if quote == b'"' {
                serde_json::from_str::<String>(&raw[..end]).map_err(|_| Error::Unsupported)?
            } else {
                raw[1..end - 1].replace("''", "'")
            };
            return Ok(MetadataValue::String {
                value: self.own(&value)?,
            });
        }
        let raw = comment(raw)?;
        if raw.is_empty() || matches!(raw, "~" | "null" | "Null" | "NULL") {
            return Ok(MetadataValue::Null {});
        }
        if matches!(raw, "true" | "True" | "TRUE") {
            return Ok(MetadataValue::Boolean { value: true });
        }
        if matches!(raw, "false" | "False" | "FALSE") {
            return Ok(MetadataValue::Boolean { value: false });
        }
        if iso_date(raw) {
            return Ok(MetadataValue::String {
                value: self.own(raw)?,
            });
        }
        let digits = raw.strip_prefix('-').unwrap_or(raw);
        if !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) {
            if digits.len() > 1 && digits.starts_with('0') {
                return Err(Error::Unsupported);
            }
            return raw
                .parse::<i64>()
                .map(|value| MetadataValue::Integer { value })
                .map_err(|_| Error::Unsupported);
        }
        let number = raw.strip_prefix(['+', '-']).unwrap_or(raw);
        if number.starts_with(|c: char| c.is_ascii_digit())
            || number
                .strip_prefix('.')
                .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_digit()))
            || raw.starts_with(['+', '-']) && raw.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
            || matches!(
                raw.to_ascii_lowercase().as_str(),
                ".nan" | ".inf" | "+.inf" | "-.inf"
            )
            || raw.starts_with(['&', '*', '!', '{', '[', '>', '|', '@', '%', '`', ','])
            || raw.starts_with("- ")
            || raw.starts_with("? ")
            || raw.contains(['[', ']', '{', '}'])
            || raw == "---"
            || raw == "..."
            || raw.char_indices().any(|(i, c)| {
                c == ':' && raw[i + 1..].chars().next().is_none_or(char::is_whitespace)
            })
        {
            return Err(Error::Unsupported);
        }
        Ok(MetadataValue::String {
            value: self.own(raw)?,
        })
    }
    fn value(&mut self, raw: &str) -> Result<MetadataValue, Error> {
        if !raw.starts_with('[') {
            return self.scalar(raw);
        }
        // Flow lists contain only string scalars. No nesting, aliases or implicit
        // conversion of booleans/numbers to strings.
        let mut quote = None;
        let mut start = 1;
        let mut pos = 1;
        let bytes = raw.as_bytes();
        let mut items = Vec::new();
        while pos < bytes.len() {
            let c = bytes[pos];
            if let Some(q) = quote {
                if q == b'"' && c == b'\\' {
                    pos += 2;
                    continue;
                }
                if c == q {
                    if q == b'\'' && bytes.get(pos + 1) == Some(&b'\'') {
                        pos += 2;
                        continue;
                    }
                    quote = None;
                }
            } else if matches!(c, b'\'' | b'"') {
                // Quotes are allowed only at the beginning of an item.
                if !raw[start..pos].trim_matches(' ').is_empty() {
                    return Err(Error::Unsupported);
                }
                quote = Some(c);
            } else if c == b',' || c == b']' {
                let item = raw[start..pos].trim_matches(' ');
                if !item.is_empty() {
                    items.push(self.item(item)?);
                } else if c == b',' || !items.is_empty() {
                    return Err(Error::Unsupported);
                }
                if c == b']' {
                    if !comment(&raw[pos + 1..])?.is_empty() {
                        return Err(Error::Unsupported);
                    }
                    return Ok(MetadataValue::Strings { value: items });
                }
                start = pos + 1;
            } else if matches!(c, b'[' | b'{' | b'}' | b'#') {
                return Err(Error::Unsupported);
            }
            pos += 1;
        }
        Err(Error::Unsupported)
    }
}

// Only a '#' at scalar start or after a space begins a comment. Quoted scalars
// are handled separately, so hashes inside quotes are always literal.
fn comment(raw: &str) -> Result<&str, Error> {
    if raw.chars().any(char::is_control) {
        return Err(Error::Unsupported);
    }
    let end = raw
        .char_indices()
        .find_map(|(i, c)| (c == '#' && (i == 0 || raw.as_bytes()[i - 1] == b' ')).then_some(i))
        .unwrap_or(raw.len());
    Ok(raw[..end].trim_matches(' '))
}
struct Pending {
    key: String,
    value: MetadataValue,
    block_list: bool,
    list_indent: Option<usize>,
}

/// Input includes the already-detected delimiter lines. All failures discard the
/// entire map; attempted source-string copies still charge the shared budget.
pub(crate) fn parse(
    front: &str,
    copied: &mut usize,
    max_text: usize,
) -> Result<BTreeMap<String, MetadataValue>, Error> {
    if front.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut context = Context {
        copied,
        max_text,
        items: 0,
    };
    let mut properties = BTreeMap::new();
    let mut pending: Option<Pending> = None;
    for line in front.lines().skip(1) {
        if matches!(line, "---" | "...") {
            break;
        }
        if line.chars().any(char::is_control) {
            return Err(Error::Unsupported);
        }
        if line.trim_matches(' ').is_empty() || line.trim_start_matches(' ').starts_with('#') {
            continue;
        }
        let trimmed = line.trim_start_matches(' ');
        if let Some(raw) = trimmed.strip_prefix("- ") {
            let indent = line.len() - trimmed.len();
            let current = pending
                .as_mut()
                .filter(|p| p.block_list)
                .ok_or(Error::Unsupported)?;
            if current
                .list_indent
                .is_some_and(|expected| expected != indent)
            {
                return Err(Error::Unsupported);
            }
            current.list_indent = Some(indent);
            let value = context.item(raw)?;
            if matches!(current.value, MetadataValue::Null {}) {
                current.value = MetadataValue::Strings { value: Vec::new() };
            }
            if let MetadataValue::Strings { value: values } = &mut current.value {
                values.push(value);
            } else {
                return Err(Error::Unsupported);
            }
            continue;
        }
        if line.starts_with(' ') {
            return Err(Error::Unsupported);
        }
        if let Some(current) = pending.take() {
            properties.insert(current.key, current.value);
        }
        if properties.len() >= MAX_PROPERTIES {
            return Err(Error::Limit);
        }
        let (key, raw) = line.split_once(':').ok_or(Error::Unsupported)?;
        if key.len() > MAX_KEY_BYTES {
            return Err(Error::Limit);
        }
        if !key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            || !raw.is_empty() && !raw.starts_with(' ')
        {
            return Err(Error::Unsupported);
        }
        if properties.contains_key(key) {
            return Err(Error::Duplicate);
        }
        let key = context.own(key)?;
        let raw = raw.trim_matches(' ');
        let block_list = comment_if_plain(raw)?.is_empty();
        let value = context.value(raw)?;
        pending = Some(Pending {
            key,
            value,
            block_list,
            list_indent: None,
        });
    }
    if let Some(current) = pending {
        properties.insert(current.key, current.value);
    }
    Ok(properties)
}
fn comment_if_plain(raw: &str) -> Result<&str, Error> {
    if raw.starts_with(['\'', '"']) {
        Ok(raw)
    } else {
        comment(raw)
    }
}

// Unicode 16.0 General_Category=Cf ranges. Copyright (c) 2024 Unicode, Inc.
// Source/provenance and Unicode-3.0
// notice: docs/Front-Matter.md and licenses/Unicode-3.0.txt. No runtime data load.
pub(crate) fn unsafe_character(c: char) -> bool {
    c.is_control()
        || matches!(c as u32,
            0xAD |
            0x600..=0x605 |
            0x61C |
            0x6DD |
            0x70F |
            0x890..=0x891 |
            0x8E2 |
            0x180E |
            0x200B..=0x200F |
            0x202A..=0x202E |
            0x2060..=0x2064 |
            0x2066..=0x206F |
            0xFEFF |
            0xFFF9..=0xFFFB |
            0x110BD |
            0x110CD |
            0x13430..=0x1343F |
            0x1BCA0..=0x1BCA3 |
            0x1D173..=0x1D17A |
            0xE0001 |
            0xE0020..=0xE007F
        )
}

// Unicode 16.0 Default_Ignorable_Code_Point plus U+2800 BRAILLE PATTERN BLANK.
// Copyright (c) 2024 Unicode, Inc. Source/provenance and Unicode-3.0 notice:
// docs/URI-Policy.md and licenses/Unicode-3.0.txt. URI targets deny all of it;
// front matter rejects and file names flag it except display modifiers.
pub(crate) fn invisible_character(c: char) -> bool {
    matches!(c as u32,
        0xAD |
        0x34F |
        0x61C |
        0x115F..=0x1160 |
        0x17B4..=0x17B5 |
        0x180B..=0x180F |
        0x200B..=0x200F |
        0x202A..=0x202E |
        0x2060..=0x206F |
        0x2800 |
        0x3164 |
        0xFE00..=0xFE0F |
        0xFEFF |
        0xFFA0 |
        0xFFF0..=0xFFF8 |
        0x1BCA0..=0x1BCA3 |
        0x1D173..=0x1D17A |
        0xE0000..=0xE0FFF
    )
}

// Default-ignorable values that display text needs: CGJ, Mongolian FVS 1-4
// and the variation selectors (emoji presentation, ideographic variants).
pub(crate) fn display_modifier(c: char) -> bool {
    matches!(c as u32,
        0x34F |
        0x180B..=0x180D |
        0x180F |
        0xFE00..=0xFE0F |
        0xE0100..=0xE01EF
    )
}

// The display-text guard (D-032): controls, Cf, and the blank default-ignorable
// values such as Hangul fillers and U+2800, except the display modifiers.
// Front-matter values reject it; file names flag it (minus ZWNJ/ZWJ).
pub(crate) fn hidden_character(c: char) -> bool {
    unsafe_character(c) || (invisible_character(c) && !display_modifier(c))
}

// Front-matter titles and string/list values (D-034): the display-text guard,
// except ZWNJ and ZWJ, which Persian, Indic and emoji text need, and the tag
// characters of a subdivision flag: U+1F3F4, 3 to 7 tag digits or lowercase
// letters (the length of a CLDR subdivision id), then U+E007F. Keys keep the
// full guard. Bidi controls, LRM and RLM stay rejected.
pub(crate) fn display_text(value: &str) -> bool {
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1F3F4}' {
            let rest = chars.as_str();
            let spec = rest
                .find(|t| !matches!(t, '\u{E0030}'..='\u{E0039}' | '\u{E0061}'..='\u{E007A}'))
                .unwrap_or(rest.len());
            // Each tag character is four UTF-8 bytes.
            if (12..=28).contains(&spec) && rest[spec..].starts_with('\u{E007F}') {
                chars = rest[spec + 4..].chars();
            }
        } else if !matches!(c, '\u{200C}' | '\u{200D}') && hidden_character(c) {
            return false;
        }
    }
    true
}

// Deliberate literal ISO subset: calendar date, optionally time and zone.
// No locale, timestamp conversion or timezone arithmetic is involved.
fn iso_date(raw: &str) -> bool {
    if !raw.is_ascii() || raw.len() < 10 {
        return false;
    }
    let number = |s: &str| -> Option<u32> {
        if s.bytes().all(|c| c.is_ascii_digit()) {
            s.parse().ok()
        } else {
            None
        }
    };
    let (Some(year), Some(month), Some(day)) =
        (number(&raw[..4]), number(&raw[5..7]), number(&raw[8..10]))
    else {
        return false;
    };
    if &raw[4..5] != "-" || &raw[7..8] != "-" || year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > days {
        return false;
    }
    if raw.len() == 10 {
        return true;
    }
    if raw.len() < 16 || !matches!(raw.as_bytes()[10], b'T' | b' ') || &raw[13..14] != ":" {
        return false;
    }
    if !number(&raw[11..13]).is_some_and(|n| n <= 23)
        || !number(&raw[14..16]).is_some_and(|n| n <= 59)
    {
        return false;
    }
    let mut tail = &raw[16..];
    if tail.starts_with(':') {
        if tail.len() < 3 || !number(&tail[1..3]).is_some_and(|n| n <= 59) {
            return false;
        }
        tail = &tail[3..];
    }
    tail.is_empty()
        || tail == "Z"
        || (tail.len() == 6
            && tail.starts_with(['+', '-'])
            && &tail[3..4] == ":"
            && number(&tail[1..3]).is_some_and(|n| n <= 23)
            && number(&tail[4..6]).is_some_and(|n| n <= 59))
}
