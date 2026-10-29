//! Workspace content-search memory guard (CORE-005C).
//!
//! A dedicated test binary with a counting global allocator, as in
//! `budgets.rs`: the peak of live heap bytes during one `ContentQuery::search`
//! call, above the bytes live before it. Under the default limits, hostile
//! files just under the 2 MiB file budget must stay within
//! [`HOSTILE_BUDGET`], and legitimate dense documents must still be searched.
//! docs/Parser-Budgets.md records the measurements behind these figures.
mod support;

use markdown_core::workspace::*;
use markdown_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// About 1.25x the worst measured default-limit peak (334.4 MiB, the `$a$`
/// storm). The reader's 8 MiB ceiling reaches about 1,152 MiB.
const HOSTILE_BUDGET: usize = 420 * MIB;

/// `seed` repeated to exactly `len` bytes (ASCII seeds only).
fn fill(seed: &str, len: usize) -> String {
    let mut s = seed.repeat(len / seed.len() + 1);
    s.truncate(len);
    s
}

/// A legitimate document of about `len` bytes, ending with one `needle`.
fn legit(head: &str, seed: &str, len: usize) -> String {
    let tail = "\n\nneedle\n";
    let mut s = head.to_owned();
    s.push_str(&seed.repeat((len - head.len() - tail.len()) / seed.len()));
    s.push_str(tail);
    s
}

fn entry() -> WorkspaceEntry {
    WorkspaceEntry {
        id: 1,
        path: WorkspacePath::new("notes/a.md").unwrap(),
    }
}

fn query(max_file_bytes: usize) -> ContentQuery {
    let limits = ContentLimits {
        max_file_bytes,
        ..ContentLimits::default()
    };
    ContentQuery::new(
        "needle",
        ContentOptions {
            limits,
            ..ContentOptions::default()
        },
    )
    .unwrap()
}

fn search(q: &ContentQuery, progress: &mut ContentProgress, source: &str) -> DocumentOutcome {
    q.search(
        progress,
        &entry(),
        source.as_bytes(),
        &CancellationToken::new(),
    )
    .unwrap()
}

/// Prose, tables and lists at the default file budget, all denser than the
/// repository's own documents (at most about 127,000 nodes per MiB).
fn legitimate() -> Vec<(&'static str, String)> {
    let len = DEFAULT_MAX_FILE_BYTES;
    let prose = profiles::generate("prose-sections-v1", len - 16, 0);
    let prose = String::from_utf8_lossy(&prose).into_owned() + "\n\nneedle\n";
    let storm = profiles::generate("delimiter-storm-v1", len - 16, 0);
    let storm = String::from_utf8(storm).unwrap() + "\n\nneedle\n";
    vec![
        ("prose profile", prose),
        ("delimiter-storm profile", storm),
        (
            "data table",
            legit(
                "| Name | Value | Notes | Flag |\n|---|---:|---|:-:|\n",
                "| Alpha beta | 12.50 | Some short note | yes |\n",
                len,
            ),
        ),
        (
            "changelog",
            legit(
                "# Changes\n\n",
                "## 1.2.3\n\n- Fixed `foo` in [#123](https://x.y/123).\n- Added **bar**.\n\n",
                len,
            ),
        ),
        (
            "checklist",
            legit("", "- [ ] Task item number one\n- [x] Done item\n", len),
        ),
        (
            "link list",
            legit(
                "",
                "- [Some page title](docs/some-page.md) - a short description\n",
                len,
            ),
        ),
    ]
}

#[test]
fn hostile_files_stay_within_the_default_content_search_budget() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let len = DEFAULT_MAX_FILE_BYTES;
    // The costliest constructs measured: each fills the default file budget.
    let hostile = [
        ("inline math", fill("$a$ ", len)),
        ("code spans", fill("`a", len)),
        ("emphasis", fill("*a", len)),
        ("soft breaks", fill("a\n", len)),
        ("open brackets", fill("[", len)),
        ("headings", fill("# h\n", len)),
        ("links", fill("[l](u) ", len)),
        ("list items", fill("- a\n", len)),
        (
            "narrow table",
            format!("|a|b|\n|-|-|\n{}", fill("|x|y|\n", len - 12)),
        ),
        (
            "references",
            fill("[a]: b.md\n", len / 2) + &fill("[a] ", len / 2),
        ),
    ];
    let q = query(DEFAULT_MAX_FILE_BYTES);
    assert_eq!(q.options().limits, ContentLimits::default());
    let mut progress = ContentProgress::default();
    let (mut bytes, mut failed) = (0u64, 0u64);
    for (name, source) in &hostile {
        assert!(source.len() <= len, "{name}");
        let (outcome, peak) = peak_during(|| search(&q, &mut progress, source));
        eprintln!(
            "{name}: peak {:.1} MiB, {outcome:?}",
            peak as f64 / MIB as f64
        );
        assert!(peak <= HOSTILE_BUDGET, "{name}: peak {peak} bytes");
        match outcome {
            DocumentOutcome::Skipped(SkipReason::ParseFailed) => failed += 1,
            DocumentOutcome::Searched(_) => {}
            other => panic!("{name}: {other:?}"),
        }
        bytes += source.len() as u64;
    }
    // Every storm here exceeds the node or event allowance, and each one is
    // still charged to the file and byte budgets.
    assert_eq!(failed, hostile.len() as u64);
    assert_eq!(progress.skipped.parse_failed, failed);
    assert_eq!((progress.files_read, progress.bytes_read), (10, bytes));
    assert_eq!(progress.stopped, None);
}

#[test]
fn legitimate_dense_documents_are_searched_at_the_default_budget() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let q = query(DEFAULT_MAX_FILE_BYTES);
    for (name, source) in legitimate() {
        assert!(source.len() <= DEFAULT_MAX_FILE_BYTES, "{name}");
        let (outcome, peak) = peak_during(|| search(&q, &mut ContentProgress::default(), &source));
        eprintln!("{name}: peak {:.1} MiB", peak as f64 / MIB as f64);
        match outcome {
            DocumentOutcome::Searched(found) => assert_eq!(found.hits.len(), 1, "{name}"),
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn explicit_file_budgets_keep_the_reader_limits_and_a_node_floor() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Over the default file budget: skipped before reading, unless the caller
    // chooses the reader's 8 MiB ceiling, which also restores its node limit.
    let prose = profiles::profile_source(&profiles::PROFILES[1]) + "\n\nneedle\n";
    let mut p = ContentProgress::default();
    assert_eq!(
        search(&query(DEFAULT_MAX_FILE_BYTES), &mut p, &prose),
        DocumentOutcome::Skipped(SkipReason::TooLarge)
    );
    assert_eq!((p.files_read, p.skipped.too_large), (0, 1));
    assert!(matches!(
        search(&query(MAX_FILE_BYTES), &mut p, &prose),
        DocumentOutcome::Searched(found) if found.hits.len() == 1
    ));
    // A narrow table (about 375,000 nodes per MiB) over the default node
    // allowance: the known false-positive class, which the reader opens and
    // the largest budget searches.
    let table = legit("|k|v|\n|-|-|\n", "| key1 | 42 |\n", 1900 * 1024);
    let mut p = ContentProgress::default();
    assert_eq!(
        search(&query(DEFAULT_MAX_FILE_BYTES), &mut p, &table),
        DocumentOutcome::Skipped(SkipReason::ParseFailed)
    );
    assert_eq!((p.files_read, p.skipped.parse_failed), (1, 1));
    assert!(matches!(
        search(&query(MAX_FILE_BYTES), &mut p, &table),
        DocumentOutcome::Searched(found) if found.hits.len() == 1
    ));
    // Small file budgets keep a node floor: a 60 KiB table needs about
    // 22,000 nodes, far more than 64 KiB / 4 bytes per node.
    let small = legit("|k|v|\n|-|-|\n", "| key1 | 42 |\n", 60 * 1024);
    assert!(matches!(
        search(&query(64 * 1024), &mut ContentProgress::default(), &small),
        DocumentOutcome::Searched(found) if found.hits.len() == 1
    ));
}
