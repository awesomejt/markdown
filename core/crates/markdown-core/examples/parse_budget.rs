//! Measure parser time and peak heap over the generated performance profiles.
//!
//! Run in release on a named host: `cargo run --release --locked --features
//! model-v2 --example parse_budget [runs]`. It prints one row per profile and
//! entry point with min/median/max wall time over `runs` (default 7) and the
//! peak live heap above the baseline, counted by a global allocator. The
//! numbers are host measurements for docs/Parser-Budgets.md, not guarantees.
//! `--cancel` instead reports 5 MiB cancellation latency over a delay sweep.
#[path = "../tests/support/profiles.rs"]
mod profiles;

use markdown_core::search::{FindOptions, ReadOptions, SearchDocument, find, read_stats};
use markdown_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn measure<T>(runs: usize, mut f: impl FnMut() -> T) -> (T, Vec<Duration>, usize) {
    let mut times = Vec::with_capacity(runs);
    let mut peak = 0;
    let mut last = None;
    for _ in 0..runs {
        drop(last.take());
        let before = LIVE.load(Ordering::Relaxed);
        PEAK.store(before, Ordering::Relaxed);
        let start = Instant::now();
        let value = f();
        times.push(start.elapsed());
        peak = peak.max(PEAK.load(Ordering::Relaxed).saturating_sub(before));
        last = Some(value);
    }
    times.sort();
    (last.unwrap(), times, peak)
}

fn row(profile: &str, entry: &str, times: &[Duration], peak: usize, extra: &str) {
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "| {profile} | {entry} | {:.1} | {:.1} | {:.1} | {:.1} | {extra} |",
        ms(times[0]),
        ms(times[times.len() / 2]),
        ms(times[times.len() - 1]),
        peak as f64 / (1024.0 * 1024.0),
    );
}

fn main() {
    if std::env::args().any(|a| a == "--cancel") {
        return cancel_latency();
    }
    let runs = std::env::args()
        .nth(1)
        .map_or(7, |s| s.parse().expect("runs or --cancel"));
    let ids = || (DocumentId("budget".into()), RevisionId("r1".into()));
    println!("| Profile | Entry | Min ms | Median ms | Max ms | Peak heap MiB | Notes |");
    println!("| --- | --- | ---: | ---: | ---: | ---: | --- |");
    // A synthetic link-dense input (not a fixture profile) prices v2's URI and
    // IDNA classification, which the prose profiles never reach.
    let links = "[a](https://example.com/p?q=1) [b](docs/guide.md#intro) \
[c](https://例え.テスト/パス) [d](#local) ![i](img/pic.png) [e](mailto:a@b.c)\n"
        .repeat(1 << 20 >> 7);
    let inputs = profiles::PROFILES
        .iter()
        .map(|p| (p.id, profiles::profile_source(p)))
        .chain([("links-synthetic", links)]);
    for (name, source) in inputs {
        let limits = ParseLimits::default();
        let (doc, times, peak) = measure(runs, || {
            let (d, r) = ids();
            parse_markdown(&source, d, r, limits)
        });
        let doc = match doc {
            Ok(doc) => doc,
            Err(e) => {
                row(name, "parse_markdown", &times, peak, &format!("error {e}"));
                continue;
            }
        };
        let nodes = doc.input().nodes.len();
        row(
            name,
            "parse_markdown",
            &times,
            peak,
            &format!("{nodes} nodes"),
        );
        #[cfg(feature = "model-v2")]
        {
            let (v2, times, peak) = measure(runs, || {
                let (d, r) = ids();
                parse_markdown_v2(&source, d, r, limits)
            });
            let notes = match v2 {
                Ok(v2) => format!("{} intents", v2.input().links.len()),
                Err(e) => format!("error {e}"),
            };
            row(name, "parse_markdown_v2", &times, peak, &notes);
        }
        let (bytes, times, peak) = measure(runs, || doc.to_json().unwrap());
        row(
            name,
            "to_json",
            &times,
            peak,
            &format!("{} bytes", bytes.len()),
        );
        let view = SearchDocument::from_v1(&doc, &source).unwrap();
        let (found, times, peak) = measure(runs, || {
            find(&view, "pending", &FindOptions::default()).unwrap()
        });
        row(
            name,
            "find",
            &times,
            peak,
            &format!("{} matches", found.matches.len()),
        );
        let (stats, times, peak) =
            measure(runs, || read_stats(&view, &ReadOptions::default()).unwrap());
        row(
            name,
            "read_stats",
            &times,
            peak,
            &format!("{} words", stats.words),
        );
    }
}

/// Cancellation latency: cancel a 5 MiB parse after a range of delays and
/// report the time from `cancel()` to the call returning.
fn cancel_latency() {
    let p = &profiles::PROFILES[1];
    let source: std::sync::Arc<str> = profiles::profile_source(p).into();
    let mut latencies = Vec::new();
    for delay in (0..160).step_by(4) {
        let token = CancellationToken::new();
        let (s, t) = (source.clone(), token.clone());
        let worker = std::thread::spawn(move || {
            let r = parse_markdown_cancellable(
                &s,
                DocumentId("budget".into()),
                RevisionId("r1".into()),
                ParseLimits::default(),
                &t,
            );
            (r.is_err(), Instant::now())
        });
        std::thread::sleep(Duration::from_millis(delay));
        let cancelled_at = Instant::now();
        token.cancel();
        let (stopped, returned) = worker.join().unwrap();
        if stopped {
            latencies.push(returned.saturating_duration_since(cancelled_at));
        }
    }
    latencies.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "\ncancel latency {} (n={}): min {:.2} ms, median {:.2} ms, max {:.2} ms",
        p.id,
        latencies.len(),
        ms(latencies[0]),
        ms(latencies[latencies.len() / 2]),
        ms(latencies[latencies.len() - 1]),
    );
}
