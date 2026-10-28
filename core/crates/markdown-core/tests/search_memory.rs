//! Working-memory bound for find and reading statistics (PR #8 review).
//!
//! A dedicated test binary with a counting global allocator: the peak of live
//! heap bytes during one call, above the bytes live before it, must stay
//! O(query + nodes), not O(text). One large single paragraph has a handful of
//! nodes, so its peak must be tiny compared with its 8 MB of text.
use markdown_core::search::*;
use markdown_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Ceiling for one call over a few-node document: far below the text size.
const BOUND: usize = 1 << 20;

#[test]
fn large_paragraph_find_and_stats_use_bounded_memory() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let source = "abc ".repeat(2_000_000); // one 8 MB paragraph
    let doc = parse_markdown(
        &source,
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let view = SearchDocument::from_v1(&doc, &source).unwrap();
    for (query, whole_word) in [
        ("zzz", false),
        ("abc abd", true),
        (&"q".repeat(4096)[..], false),
    ] {
        let options = FindOptions {
            whole_word,
            ..FindOptions::default()
        };
        let (result, peak) = peak_during(|| find(&view, query, &options).unwrap());
        assert!(result.matches.is_empty());
        eprintln!("find {} bytes: peak {peak} bytes", query.len());
        assert!(peak < BOUND, "find peak {peak} bytes for an 8 MB paragraph");
    }
    // A match-heavy search is bounded by max_matches, not by the text.
    let options = FindOptions {
        max_matches: 100,
        ..FindOptions::default()
    };
    let (result, peak) = peak_during(|| find(&view, "abc", &options).unwrap());
    assert!(result.truncated);
    eprintln!("find truncated: peak {peak} bytes");
    assert!(peak < BOUND);
    let (stats, peak) = peak_during(|| read_stats(&view, &ReadOptions::default()).unwrap());
    assert_eq!(stats.words, 2_000_000);
    eprintln!("read_stats: peak {peak} bytes");
    assert!(peak < BOUND, "read_stats peak {peak} bytes");
}
