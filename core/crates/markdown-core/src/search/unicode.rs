//! Dependency-free Unicode helpers for find and word counting. Character
//! properties come from Rust std (`char::is_alphabetic`, `is_numeric`,
//! `is_whitespace`, `to_lowercase`) plus small explicit tables documented in
//! `specs/search/README.md`. They approximate, and do not claim to implement,
//! Unicode simple case folding, UAX #29 grapheme and word boundaries.

/// Per-scalar case fold used by case-insensitive find: the Unicode simple
/// case folding exceptions that differ from lowercasing, otherwise a
/// single-scalar `to_lowercase`. Scalars whose lowercase expands to several
/// scalars (for example U+0130) are unchanged. No locale tailoring: Turkish
/// dotless/dotted i are not folded with i/I, and ß does not match "ss".
pub fn fold_case(c: char) -> char {
    match c {
        '\u{B5}' => '\u{3BC}',               // micro sign
        '\u{3C2}' => '\u{3C3}',              // final sigma
        '\u{3D0}' => '\u{3B2}',              // beta symbol
        '\u{3D1}' => '\u{3B8}',              // theta symbol
        '\u{3D5}' => '\u{3C6}',              // phi symbol
        '\u{3D6}' => '\u{3C0}',              // pi symbol
        '\u{3F0}' => '\u{3BA}',              // kappa symbol
        '\u{3F1}' => '\u{3C1}',              // rho symbol
        '\u{3F5}' => '\u{3B5}',              // lunate epsilon
        '\u{1E9B}' => '\u{1E61}',            // long s with dot above
        '\u{17F}' => 's',                    // long s
        '\u{345}' | '\u{1FBE}' => '\u{3B9}', // iota subscript / prosgegrammeni
        '\u{1C80}' => '\u{432}',             // Cyrillic rounded ve ...
        '\u{1C81}' => '\u{434}',
        '\u{1C82}' => '\u{43E}',
        '\u{1C83}' => '\u{441}',
        '\u{1C84}' | '\u{1C85}' => '\u{442}',
        '\u{1C86}' => '\u{44A}',
        '\u{1C87}' => '\u{463}',
        '\u{1C88}' => '\u{A64B}',
        _ => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        }
    }
}

/// Approximate Grapheme_Cluster_Break=Extend/ZWJ (plus SpacingMark-like
/// Indic/Southeast Asian signs): characters that never start a cluster.
pub(crate) fn is_extend(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2
        | 0x05C4..=0x05C5 | 0x05C7 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670
        | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED | 0x0711
        | 0x0730..=0x074A | 0x07A6..=0x07B0 | 0x07EB..=0x07F3 | 0x0816..=0x082D
        | 0x0859..=0x085B | 0x0898..=0x089F | 0x08CA..=0x08E1 | 0x08E3..=0x0903
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200C..=0x200D | 0x20D0..=0x20FF
        | 0x2CEF..=0x2CF1 | 0x2DE0..=0x2DFF | 0x302A..=0x302F | 0x3099..=0x309A
        | 0xA66F..=0xA672 | 0xA674..=0xA67D | 0xA69E..=0xA69F | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F | 0xFF9E..=0xFF9F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F
        | 0xE0100..=0xE01EF)
        // Brahmic and Southeast Asian blocks: non-alphanumeric signs (viramas,
        // vowel signs, nuktas) attach to the preceding letter.
        || (matches!(u, 0x0900..=0x0DFF | 0x0E00..=0x0FFF | 0x1000..=0x109F | 0x1780..=0x17FF)
            && !c.is_alphanumeric()
            && !c.is_whitespace()
            && !matches!(u, 0x0964 | 0x0965 | 0x0970 | 0x0DF4 | 0x0E3F | 0x0E4F | 0x0E5A | 0x0E5B
                | 0x0F04..=0x0F14 | 0x104A..=0x104F | 0x17D4..=0x17DA))
}

pub(crate) const ZWJ: char = '\u{200D}';

pub(crate) fn is_regional_indicator(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// Word-boundary classes, simplified from UAX #29.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WordClass {
    Letter,
    Digit,
    /// Han ideographs and Hiragana: every scalar is its own word.
    Ideograph,
    Katakana,
    Extend,
    /// Connector punctuation such as `_`: joins adjacent word runs.
    Connector,
    /// Joins letter-letter (`:` `·`).
    MidLetter,
    /// Joins digit-digit (`,` `;`).
    MidNum,
    /// Joins letter-letter or digit-digit (`.` `'` U+2019).
    MidNumLet,
    Other,
}

fn is_ideograph(u: u32) -> bool {
    matches!(u,
        0x3005 | 0x3007 | 0x3021..=0x3029 | 0x3038..=0x303B | 0x3040..=0x309F // Hiragana
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x1B001..=0x1B11F
        | 0x20000..=0x2FA1F | 0x30000..=0x323AF)
}
fn is_katakana(u: u32) -> bool {
    matches!(u, 0x30A0..=0x30FF | 0x31F0..=0x31FF | 0x32D0..=0x32FE | 0x3300..=0x3357
        | 0xFF66..=0xFF9D | 0x1B000)
}

/// Superscripts, vulgar fractions and enclosed numbers are not word-like.
fn is_symbol_number(u: u32) -> bool {
    matches!(u, 0xB2 | 0xB3 | 0xB9 | 0xBC..=0xBE | 0x2070..=0x209F | 0x2150..=0x215F | 0x2189
        | 0x2460..=0x24FF | 0x2776..=0x2793 | 0x3192..=0x3195 | 0x3220..=0x3229
        | 0x3248..=0x324F | 0x3251..=0x325F | 0x3280..=0x3289 | 0x32B1..=0x32BF)
}

pub(crate) fn word_class(c: char) -> WordClass {
    let u = c as u32;
    if is_extend(c) {
        WordClass::Extend
    } else if is_ideograph(u) {
        WordClass::Ideograph
    } else if is_katakana(u) {
        WordClass::Katakana
    } else if c.is_alphabetic() {
        WordClass::Letter
    } else if c.is_numeric() && !is_symbol_number(u) {
        WordClass::Digit
    } else {
        match u {
            0x5F | 0x203F | 0x2040 | 0x2054 | 0xFE33 | 0xFE34 | 0xFE4D..=0xFE4F | 0xFF3F => {
                WordClass::Connector
            }
            0x3A | 0xB7 | 0x387 | 0x55F | 0x5F4 | 0x2027 | 0xFE13 | 0xFE55 | 0xFF1A => {
                WordClass::MidLetter
            }
            0x2C | 0x3B | 0x37E | 0x589 | 0x60C | 0x60D | 0x66C | 0x7F8 | 0x2044 | 0xFE10
            | 0xFE14 | 0xFE50 | 0xFE54 | 0xFF0C | 0xFF1B => WordClass::MidNum,
            0x27 | 0x2E | 0x2018 | 0x2019 | 0x2024 | 0xFE52 | 0xFF07 | 0xFF0E => {
                WordClass::MidNumLet
            }
            _ => WordClass::Other,
        }
    }
}

/// True when `a` immediately followed by `b` continues one word (used for
/// whole-word find). Ideographs are words of their own and never join.
pub(crate) fn joins_word(a: char, b: char) -> bool {
    let joinable = |c| {
        matches!(
            word_class(c),
            WordClass::Letter
                | WordClass::Digit
                | WordClass::Katakana
                | WordClass::Connector
                | WordClass::Extend
        )
    };
    joinable(a) && joinable(b)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    None,
    /// A connector seen without a preceding word: not yet a word.
    Connector,
    AlNum,
    Katakana,
}

/// Streaming word counter shared by [`word_count`] and reading statistics.
#[derive(Clone, Copy)]
pub(crate) struct WordCounter {
    run: Run,
    last: WordClass,
    mid: Option<WordClass>,
    pub words: u64,
}
impl Default for WordCounter {
    fn default() -> Self {
        Self {
            run: Run::None,
            last: WordClass::Other,
            mid: None,
            words: 0,
        }
    }
}
impl WordCounter {
    /// A hard boundary: excluded inline content or a unit edge.
    pub fn separate(&mut self) {
        self.run = Run::None;
        self.mid = None;
    }
    pub fn push(&mut self, c: char) {
        let class = word_class(c);
        match class {
            WordClass::Extend => {}
            WordClass::Letter | WordClass::Digit => {
                let continues = self.run == Run::AlNum
                    && match self.mid {
                        None => true,
                        Some(WordClass::MidLetter) => {
                            self.last == WordClass::Letter && class == WordClass::Letter
                        }
                        Some(WordClass::MidNum) => {
                            self.last == WordClass::Digit && class == WordClass::Digit
                        }
                        Some(_) => self.last == class,
                    };
                if !continues {
                    self.words += 1;
                }
                self.run = Run::AlNum;
                self.last = class;
                self.mid = None;
            }
            WordClass::Katakana => {
                if self.run != Run::Katakana {
                    self.words += 1;
                }
                self.run = Run::Katakana;
                self.mid = None;
            }
            WordClass::Ideograph => {
                self.words += 1;
                self.separate();
            }
            WordClass::Connector => {
                if self.mid.is_some() {
                    self.separate();
                }
                if self.run == Run::None {
                    self.run = Run::Connector;
                }
            }
            WordClass::MidLetter | WordClass::MidNum | WordClass::MidNumLet => {
                if self.run == Run::AlNum && self.mid.is_none() {
                    self.mid = Some(class);
                } else {
                    self.separate();
                }
            }
            WordClass::Other => self.separate(),
        }
    }
}

/// Count words in plain text with the documented segmentation rule.
pub fn word_count(text: &str) -> u64 {
    let mut counter = WordCounter::default();
    for c in text.chars() {
        counter.push(c);
    }
    counter.words
}
