use std::env;
use std::fs;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

use comrak::{Arena, Options as ComrakOptions, parse_document};
use pulldown_cmark::{Options as PulldownOptions, Parser};

fn usage() -> ! {
    eprintln!("usage: markdown-parser-probe <comrak|pulldown> <file> <iterations>");
    std::process::exit(2);
}

fn comrak_options() -> ComrakOptions<'static> {
    let mut options = ComrakOptions::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.math_dollars = true;
    options.extension.math_code = true;
    options.extension.alerts = true;
    options.extension.front_matter_delimiter = Some("---".into());
    options
}

fn pulldown_options() -> PulldownOptions {
    PulldownOptions::ENABLE_TABLES
        | PulldownOptions::ENABLE_FOOTNOTES
        | PulldownOptions::ENABLE_STRIKETHROUGH
        | PulldownOptions::ENABLE_TASKLISTS
        | PulldownOptions::ENABLE_GFM
        | PulldownOptions::ENABLE_MATH
        | PulldownOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

fn run_comrak(input: &str, iterations: usize) -> (u128, usize, usize) {
    let options = comrak_options();
    let mut last_nodes = 0;
    let mut last_positioned = 0;
    let start = Instant::now();
    for _ in 0..iterations {
        let arena = Arena::new();
        let root = parse_document(&arena, black_box(input), &options);
        last_nodes = root.descendants().count();
        last_positioned = root
            .descendants()
            .filter(|node| node.data.borrow().sourcepos.start.line > 0)
            .count();
        black_box((last_nodes, last_positioned));
    }
    (start.elapsed().as_nanos(), last_nodes, last_positioned)
}

fn run_pulldown(input: &str, iterations: usize) -> (u128, usize, usize) {
    let options = pulldown_options();
    let mut last_events = 0;
    let mut last_positioned = 0;
    let start = Instant::now();
    for _ in 0..iterations {
        let mut events = 0;
        let mut positioned = 0;
        for (event, range) in Parser::new_ext(black_box(input), options).into_offset_iter() {
            black_box(event);
            events += 1;
            positioned += usize::from(range.start <= range.end && range.end <= input.len());
        }
        last_events = events;
        last_positioned = positioned;
        black_box((last_events, last_positioned));
    }
    (start.elapsed().as_nanos(), last_events, last_positioned)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        usage();
    }
    let engine = &args[1];
    let path = &args[2];
    let iterations: usize = args[3].parse().unwrap_or_else(|_| usage());
    if iterations == 0 {
        usage();
    }
    let input = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("failed to read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };

    // Prime code and allocator paths outside the measured interval.
    match engine.as_str() {
        "comrak" => {
            black_box(run_comrak(&input, 1));
        }
        "pulldown" => {
            black_box(run_pulldown(&input, 1));
        }
        _ => usage(),
    }

    let (elapsed_ns, units, positioned) = match engine.as_str() {
        "comrak" => run_comrak(&input, iterations),
        "pulldown" => run_pulldown(&input, iterations),
        _ => unreachable!(),
    };
    println!("engine,file,bytes,iterations,elapsed_ns,ns_per_iteration,units,positioned_units");
    println!(
        "{engine},{path},{},{iterations},{elapsed_ns},{},{units},{positioned}",
        input.len(),
        elapsed_ns / iterations as u128
    );
    ExitCode::SUCCESS
}
