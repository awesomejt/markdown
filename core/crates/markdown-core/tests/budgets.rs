//! Parser memory budgets and release time guards (CORE-003B4).
//!
//! A dedicated test binary with a counting global allocator measures the peak
//! live heap above the baseline while parsing each generated profile. Counted
//! allocation sizes are deterministic for a given build, so the heap budgets
//! hold on every 64-bit host. Wall time is only guarded in release builds, with
//! wide margins for slower CI runners; docs/Parser-Budgets.md records the
//! named-host measurements these ceilings come from.
mod support;

use markdown_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use support::profiles;

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

// Tests in one binary run on parallel threads; serialize the measurements.
static SERIAL: Mutex<()> = Mutex::new(());

fn peak_during<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let value = f();
    (value, PEAK.load(Ordering::Relaxed).saturating_sub(before))
}

const MIB: usize = 1024 * 1024;

/// (profile, peak heap budget, release wall-time guard). Budgets are about
/// 1.25x the measured v1 peak; guards are about 10x the named-host median.
const BUDGETS: [(&str, usize, Duration); 4] = [
    ("prose-1mib", 44 * MIB, Duration::from_millis(250)),
    ("prose-5mib", 184 * MIB, Duration::from_millis(1500)),
    ("delimiter-storm-1mib", 84 * MIB, Duration::from_millis(600)),
    ("long-lines-1mib", 4 * MIB, Duration::from_millis(100)),
];

/// A 2 MiB `*a` storm, about 1.25x its 407.7 MiB measured peak.
const DENSE_STORM_BUDGET: usize = 510 * MIB;

fn parse(source: &str) -> Document {
    parse_markdown(
        source,
        DocumentId("budget".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap()
}

#[test]
fn profiles_stay_within_parser_heap_budgets_and_release_time_guards() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (p, (id, heap, time)) in profiles::PROFILES.iter().zip(BUDGETS) {
        assert_eq!(p.id, id);
        let source = profiles::profile_source(p);
        let (doc, peak) = peak_during(|| parse(&source));
        eprintln!("{id}: v1 peak {:.1} MiB", peak as f64 / MIB as f64);
        assert!(peak <= heap, "{id}: v1 peak {peak} bytes over {heap}");
        drop(doc);
        #[cfg(feature = "model-v2")]
        {
            let (doc, peak) = peak_during(|| {
                parse_markdown_v2(
                    &source,
                    DocumentId("budget".into()),
                    RevisionId("r1".into()),
                    ParseLimits::default(),
                )
                .unwrap()
            });
            eprintln!("{id}: v2 peak {:.1} MiB", peak as f64 / MIB as f64);
            assert!(peak <= heap, "{id}: v2 peak {peak} bytes over {heap}");
            drop(doc);
        }
        if !cfg!(debug_assertions) {
            let best = (0..3)
                .map(|_| {
                    let start = Instant::now();
                    drop(parse(&source));
                    start.elapsed()
                })
                .min()
                .unwrap();
            eprintln!("{id}: best of 3 {best:?}");
            assert!(best <= time, "{id}: {best:?} over the {time:?} guard");
        }
    }
}

#[test]
fn retained_snapshot_is_smaller_than_the_parse_peak() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let source = profiles::profile_source(&profiles::PROFILES[1]);
    let before = LIVE.load(Ordering::Relaxed);
    let (doc, peak) = peak_during(|| parse(&source));
    let retained = LIVE.load(Ordering::Relaxed).saturating_sub(before);
    eprintln!(
        "prose-5mib: retained {:.1} MiB of a {:.1} MiB peak",
        retained as f64 / MIB as f64,
        peak as f64 / MIB as f64
    );
    // Parser scaffolding (upstream tree, builder maps, validation index) is
    // released; the published snapshot is what a session keeps.
    assert!(retained < peak);
    assert!(retained <= 128 * MIB, "retained {retained} bytes");
    drop(doc);
}

#[test]
fn dense_delimiter_storm_stays_within_its_heap_budget() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Upstream inline work runs before the node limit can fire, so a dense
    // storm peaks far above its source size (about 140x at the 8 MiB ceiling).
    // 2 MiB keeps CI light while catching a regression in that ratio.
    let source = "*a".repeat(MIB);
    let (result, peak) = peak_during(|| {
        parse_markdown(
            &source,
            DocumentId("budget".into()),
            RevisionId("r1".into()),
            ParseLimits::default(),
        )
    });
    eprintln!(
        "dense-storm-2mib: peak {:.1} MiB, {:?}",
        peak as f64 / MIB as f64,
        result.as_ref().err()
    );
    drop(result);
    assert!(
        peak <= DENSE_STORM_BUDGET,
        "peak {peak} bytes over the budget"
    );
}
