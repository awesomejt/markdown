//! Rust replica of `tools/generate_performance_fixtures.py` for the profiles in
//! `fixtures/performance/profiles.yaml`. The conformance contract carries each
//! profile's size and SHA-256, so drift from the Python generator fails tests.
#![allow(dead_code)]

pub struct Profile {
    pub id: &'static str,
    pub generator: &'static str,
    pub target_bytes: usize,
    pub line_bytes: usize,
}

pub const PROFILES: [Profile; 4] = [
    Profile {
        id: "prose-1mib",
        generator: "prose-sections-v1",
        target_bytes: 1 << 20,
        line_bytes: 0,
    },
    Profile {
        id: "prose-5mib",
        generator: "prose-sections-v1",
        target_bytes: 5 << 20,
        line_bytes: 0,
    },
    Profile {
        id: "delimiter-storm-1mib",
        generator: "delimiter-storm-v1",
        target_bytes: 1 << 20,
        line_bytes: 0,
    },
    Profile {
        id: "long-lines-1mib",
        generator: "long-lines-v1",
        target_bytes: 1 << 20,
        line_bytes: 131_072,
    },
];

fn fill(seed: &[u8], target: usize) -> Vec<u8> {
    seed.iter().copied().cycle().take(target).collect()
}

/// Exact profile bytes. Every generator emits ASCII except the prose seed's
/// combining acute accent; truncation can split it, so callers must check UTF-8.
pub fn generate(generator: &str, target_bytes: usize, line_bytes: usize) -> Vec<u8> {
    match generator {
        "prose-sections-v1" => fill(
            b"## Generated section\n\n\
A representative paragraph has links, **emphasis**, `code`, Unicode cafe\xcc\x81, \
and enough ordinary prose to exercise parsing and native text layout.\n\n\
- first item\n- second item\n- [ ] pending item\n\n",
            target_bytes,
        ),
        "delimiter-storm-v1" => fill(
            b"# Hostile delimiters\n\n\
[unclosed **emphasis _and `code <tag attr='value'> ((({{{[[[\\\\\n",
            target_bytes,
        ),
        "long-lines-v1" => {
            let prefix = b"# Long lines\n\n";
            let mut line = vec![b'x'; line_bytes.max(2) - 1];
            line.push(b'\n');
            let mut out = prefix.to_vec();
            out.extend(fill(&line, target_bytes - prefix.len()));
            out.truncate(target_bytes);
            out
        }
        other => panic!("unknown generator {other}"),
    }
}

pub fn profile_source(p: &Profile) -> String {
    String::from_utf8(generate(p.generator, p.target_bytes, p.line_bytes))
        .expect("profile bytes are UTF-8")
}
