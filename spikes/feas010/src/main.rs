//! FEAS-010 probe: product syntax extensions over pulldown-cmark and Comrak.
//!
//! `extract <engine> <file>` prints the semantic JSON for one document.
//! `bench <engine> <file> <iterations> <parse|full|script>` prints one CSV row:
//! `parse` walks the parser output only, `full` adds the shared extension
//! passes, and `script` also forces the voice-over pass.

mod cm;
mod common;
mod pd;
#[cfg(test)]
mod tests;

use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

fn usage() -> ExitCode {
    eprintln!("usage: feas010 extract <pulldown|comrak> <file>");
    eprintln!("       feas010 bench <pulldown|comrak> <file> <iterations> <parse|full|script>");
    ExitCode::from(2)
}

fn run(engine: &str, source: &str, mode: &str) -> Option<usize> {
    Some(match (engine, mode) {
        ("pulldown", "parse") => pd::parse_only(source),
        ("comrak", "parse") => cm::parse_only(source),
        ("pulldown", _) => common::finish(source, pd::extract(source), mode == "script")
            .outline
            .len(),
        ("comrak", _) => common::finish(source, cm::extract(source), mode == "script")
            .outline
            .len(),
        _ => return None,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(source) = args.get(2).and_then(|p| std::fs::read_to_string(p).ok()) else {
        return usage();
    };
    match args.first().map(String::as_str) {
        Some("extract") if args.len() == 3 => {
            let raw = match args[1].as_str() {
                "pulldown" => pd::extract(&source),
                "comrak" => cm::extract(&source),
                _ => return usage(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&common::finish(&source, raw, false)).unwrap()
            );
            ExitCode::SUCCESS
        }
        Some("bench") if args.len() == 5 => {
            let (engine, mode) = (args[1].as_str(), args[4].as_str());
            let Ok(iterations) = args[3].parse::<u32>() else {
                return usage();
            };
            if iterations == 0
                || !matches!(mode, "parse" | "full" | "script")
                || run(engine, &source, mode).is_none()
            {
                return usage();
            }
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(run(engine, black_box(&source), mode));
            }
            let ns = start.elapsed().as_nanos() / u128::from(iterations);
            println!("engine,file,mode,bytes,iterations,ns_per_iteration");
            println!(
                "{engine},{},{mode},{},{iterations},{ns}",
                args[2],
                source.len()
            );
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}
