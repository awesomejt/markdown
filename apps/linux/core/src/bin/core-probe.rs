//! Release measurement probe for the linked core package (LNX-001B). One JSON line
//! per trial: parse, flat-view encode and copy times, process RSS, peak and heap.
//!
//! Modes: `single` and `repeated` (20 sessions) parse 5 MiB of prose, or the file
//! given as the second argument; `dense-host` parses 5 MiB of `*a` under host
//! limits; `dense-default` parses 8 MiB of `*a` with every core default.

use std::time::{Duration, Instant};

use markdown_linux_core::{CoreError, CoreSession, HostLimits, live_handles, validate};

unsafe extern "C" {
    fn mallinfo2() -> MallInfo2;
    fn malloc_trim(pad: usize) -> i32;
}

/// glibc `struct mallinfo2`.
#[repr(C)]
#[allow(dead_code)]
struct MallInfo2 {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

const MIB: usize = 1024 * 1024;

fn main() {
    if let Err(message) = run() {
        eprintln!("Core probe failed: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    validate().map_err(|error| error.to_string())?;
    if markdown_linux_core::PACKAGE_CONFIGURATION != "release" {
        return Err("measure with the release package and a release build".into());
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mode = arguments.first().map_or("single", String::as_str);
    let initial = live_handles().map_err(|error| error.to_string())?;
    let (count, limits, source) = match mode {
        "single" | "repeated" => {
            let source = match arguments.get(1) {
                Some(path) => std::fs::read(path).map_err(|_| "unreadable input".to_owned())?,
                None => repeat(
                    "A sentence with readable words and an ordinary paragraph.\n\n",
                    5 * MIB,
                ),
            };
            (
                if mode == "repeated" { 20 } else { 1 },
                HostLimits::HOST,
                source,
            )
        }
        "dense-host" => (1, HostLimits::HOST, repeat("*a", 5 * MIB)),
        // Deliberately bypasses host policy: zero limits select the core defaults.
        "dense-default" => {
            let defaults = HostLimits {
                max_source_bytes: 0,
                max_nodes: 0,
                max_events: 0,
                max_text_bytes: 0,
                max_snapshot_bytes: HostLimits::HOST.max_snapshot_bytes,
            };
            (1, defaults, repeat("*a", 8 * MIB))
        }
        _ => return Err(format!("unknown mode {mode:?}")),
    };
    let baseline = memory();
    for iteration in 1..=count {
        reset_peak();
        let mut row = trial(&source, limits).map_err(|error| error.to_string())?;
        let retained = memory();
        // SAFETY: glibc malloc_trim only returns free heap memory to the kernel.
        unsafe { malloc_trim(0) };
        let trimmed = memory();
        let handles = live_handles().map_err(|error| error.to_string())?;
        row.extend([
            ("mode", format!("{mode:?}")),
            ("iteration", iteration.to_string()),
            ("baseline_rss", baseline.rss.to_string()),
            ("retained_rss", retained.rss.to_string()),
            ("retained_heap", retained.heap.to_string()),
            ("trimmed_rss", trimmed.rss.to_string()),
            (
                "native_handles",
                (handles.total() - initial.total()).to_string(),
            ),
        ]);
        let fields: Vec<String> = row
            .iter()
            .map(|(key, value)| format!("\"{key}\":{value}"))
            .collect();
        println!("{{{}}}", fields.join(","));
    }
    if live_handles().map_err(|error| error.to_string())? != initial {
        return Err("native handles leaked".into());
    }
    Ok(())
}

fn repeat(unit: &str, size: usize) -> Vec<u8> {
    let mut source = unit.repeat(size / unit.len() + 1).into_bytes();
    source.truncate(size);
    source
}

/// Session create, parse (with copy) and awaited close; the result drops before close.
fn trial(source: &[u8], limits: HostLimits) -> Result<Vec<(&'static str, String)>, CoreError> {
    let session = CoreSession::with_limits("probe", limits)?;
    let start = Instant::now();
    let result = session.parse_blocking(source.to_vec(), None, true)?;
    let call = start.elapsed();
    let live = memory();
    let document = result.document.as_ref();
    let mut row = vec![
        ("source_bytes", source.len().to_string()),
        ("outcome", format!("\"{:?}\"", result.outcome)),
        ("failure", format!("\"{:?}\"", result.failure)),
        ("parse_ms", ms(result.timings.parse)),
        ("encode_ms", ms(result.timings.encode)),
        ("copy_ms", ms(result.timings.copy)),
        ("call_ms", ms(call)),
        (
            "copied_bytes",
            document.map_or(0, |d| d.copied_bytes).to_string(),
        ),
        ("nodes", document.map_or(0, |d| d.nodes.len()).to_string()),
        ("live_rss", live.rss.to_string()),
        ("live_heap", live.heap.to_string()),
        ("trial_peak_rss", live.peak.to_string()),
    ];
    drop(result);
    let closing = Instant::now();
    session.close()?;
    row.push(("close_ms", ms(closing.elapsed())));
    Ok(row)
}

fn ms(duration: Duration) -> String {
    format!("{:.2}", duration.as_secs_f64() * 1000.0)
}

struct Memory {
    rss: u64,
    peak: u64,
    heap: u64,
}

/// Resident and peak resident bytes from /proc, and glibc's in-use heap bytes.
fn memory() -> Memory {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .map_or(0, |kib| kib * 1024)
    };
    // SAFETY: mallinfo2 only reads allocator statistics.
    let info = unsafe { mallinfo2() };
    Memory {
        rss: field("VmRSS:"),
        peak: field("VmHWM:"),
        heap: (info.uordblks + info.hblkhd) as u64,
    }
}

/// Reset VmHWM to the current RSS, so each trial reports its own peak.
fn reset_peak() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}
