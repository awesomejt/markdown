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
            || raw.starts_with(['&', '*', '!', '{', '[', '>', '|', '@', '%', '`'])
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
        if line.starts_with(' ') {
            let current = pending
                .as_mut()
                .filter(|p| p.block_list)
                .ok_or(Error::Unsupported)?;
            let raw = line.strip_prefix("  - ").ok_or(Error::Unsupported)?;
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
