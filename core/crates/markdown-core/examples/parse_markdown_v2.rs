//! Parse bounded UTF-8 stdin into semantic JSON; no resource I/O is performed.
use markdown_core::{DocumentId, ParseLimits, RevisionId, parse_markdown_v2};
use std::io::{Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ParseLimits::default();
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(limits.model.max_source_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    // Do not include invalid user bytes in an error or diagnostic.
    let source = std::str::from_utf8(&bytes).map_err(|_| "invalid_utf8")?;
    let document = parse_markdown_v2(
        source,
        DocumentId("example".into()),
        RevisionId("r1".into()),
        limits,
    )?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer_pretty(&mut out, &document)?;
    out.write_all(b"\n")?;
    Ok(())
}
