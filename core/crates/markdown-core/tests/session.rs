use markdown_core::session::*;
use markdown_core::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId("doc".into()), SessionOptions::default()).unwrap()
}
fn corpus(path: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/markdown")
            .join(path),
    )
    .unwrap()
}
fn token(value: &str) -> Option<RevisionToken> {
    Some(RevisionToken::new(value.as_bytes()).unwrap())
}
fn publish(s: &DocumentSession, source: &str) -> Arc<Snapshot> {
    match s.submit(source.as_bytes().to_vec(), None).unwrap().run() {
        Outcome::Published(snapshot) => snapshot,
        other => panic!("{other:?}"),
    }
}
fn generation(n: u64) -> Generation {
    serde_json::from_str(&n.to_string()).unwrap()
}
fn large(paragraphs: usize) -> String {
    (0..paragraphs)
        .map(|i| format!("Paragraph {i} with *emphasis* and `code`.\n\n"))
        .collect()
}

#[test]
fn core_types_are_send_and_sync() {
    fn send_sync<T: Send + Sync>() {}
    fn send<T: Send>() {}
    send_sync::<DocumentSession>();
    send_sync::<Snapshot>();
    send_sync::<Arc<Snapshot>>();
    send_sync::<CancellationToken>();
    send_sync::<Outcome>();
    send_sync::<SessionStatus>();
    send_sync::<Anchor>();
    send::<Job>();
}

#[test]
fn cancellable_parse_matches_default_output() {
    let token = CancellationToken::new();
    for source in [
        corpus("foundation/gfm-tour.md"),
        corpus("foundation/code-fences.md"),
        corpus("foundation/unsafe-html.md"),
        large(2_000),
        String::new(),
    ] {
        let id = || (DocumentId("doc".into()), RevisionId("r1".into()));
        let (d, r) = id();
        let plain = parse_markdown(&source, d, r, ParseLimits::default()).unwrap();
        let (d, r) = id();
        let cancellable =
            parse_markdown_cancellable(&source, d, r, ParseLimits::default(), &token).unwrap();
        assert_eq!(plain, cancellable);
        #[cfg(feature = "model-v2")]
        {
            let (d, r) = id();
            let plain = parse_markdown_v2(&source, d, r, ParseLimits::default()).unwrap();
            let (d, r) = id();
            let cancellable =
                parse_markdown_v2_cancellable(&source, d, r, ParseLimits::default(), &token)
                    .unwrap();
            assert_eq!(plain, cancellable);
        }
    }
    // Structural errors are still reported as typed parse errors.
    let limits = ParseLimits {
        max_events: 3,
        ..ParseLimits::default()
    };
    let error = parse_markdown_cancellable(
        "a *b* c\n\nd",
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        limits,
        &token,
    );
    assert_eq!(
        error,
        Err(CancellableParseError::Parse(ParseError::EventLimit))
    );
}

#[test]
fn cancelled_parse_returns_typed_outcome() {
    let token = CancellationToken::new();
    let clone = token.clone();
    clone.cancel();
    clone.cancel();
    assert!(token.is_cancelled());
    assert_eq!(token.check(), Err(Cancelled));
    let result = parse_markdown_cancellable(
        &large(10),
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
        &token,
    );
    assert_eq!(result, Err(CancellableParseError::Cancelled));
    #[cfg(feature = "model-v2")]
    assert_eq!(
        parse_markdown_v2_cancellable(
            "[a]\n\n[a]: /x",
            DocumentId("doc".into()),
            RevisionId("r1".into()),
            ParseLimits::default(),
            &token,
        ),
        Err(CancellableParseError::Cancelled)
    );
}

#[test]
fn concurrent_cancellation_yields_complete_result_or_cancelled() {
    let source = large(20_000);
    let baseline = parse_markdown(
        &source,
        DocumentId("doc".into()),
        RevisionId("r1".into()),
        ParseLimits::default(),
    )
    .unwrap();
    let mut cancelled = 0;
    for delay in 0..24u64 {
        let token = CancellationToken::new();
        let canceller = {
            let token = token.clone();
            thread::spawn(move || {
                thread::sleep(std::time::Duration::from_micros(delay * 250));
                token.cancel();
            })
        };
        match parse_markdown_cancellable(
            &source,
            DocumentId("doc".into()),
            RevisionId("r1".into()),
            ParseLimits::default(),
            &token,
        ) {
            Ok(document) => assert_eq!(document, baseline),
            Err(CancellableParseError::Cancelled) => cancelled += 1,
            Err(other) => panic!("{other:?}"),
        }
        canceller.join().unwrap();
    }
    // The zero-delay iterations race with parse start, so at least one cancels.
    assert!(cancelled > 0);
}

#[test]
fn generations_increase_and_snapshots_are_immutable() {
    let s = session();
    assert!(s.current().is_none());
    assert!(!s.status().is_current());
    let first = publish(&s, "# One\n");
    let second = publish(&s, "# Two\n");
    assert!(first.generation() < second.generation());
    assert_eq!(first.generation().get(), 1);
    assert_eq!(first.revision_id(), &RevisionId("g1".into()));
    assert_eq!(second.revision_id(), &RevisionId("g2".into()));
    assert_eq!(first.source(), "# One\n");
    assert_eq!(first.v1().unwrap().input().outline[0].title, "One");
    assert_eq!(
        second.v1().unwrap().input().document_id,
        DocumentId("doc".into())
    );
    assert_eq!(first.source_info(), &SourceInfo::of("# One\n"));
    let status = s.status();
    assert!(status.is_current());
    assert_eq!(status.latest, Some(second.generation()));
    assert!(Arc::ptr_eq(&s.current().unwrap(), &second));
    // Debug output never includes document contents.
    assert!(!format!("{first:?}").contains("One"));
}

#[test]
fn stale_jobs_never_publish() {
    let s = session();
    let jobs: Vec<Job> = (1..=3)
        .map(|i| s.submit(format!("v{i}").into_bytes(), None).unwrap())
        .collect();
    assert!(jobs[0].cancellation_token().is_cancelled());
    assert!(jobs[1].cancellation_token().is_cancelled());
    assert!(!jobs[2].cancellation_token().is_cancelled());
    assert_eq!(s.status().pending, Some(generation(3)));
    let mut jobs = jobs.into_iter().rev();
    match jobs.next().unwrap().run() {
        Outcome::Published(snapshot) => assert_eq!(snapshot.source(), "v3"),
        other => panic!("{other:?}"),
    }
    for job in jobs {
        let g = job.generation();
        match job.run() {
            Outcome::Superseded {
                generation: x,
                latest,
            } => {
                assert_eq!(x, g);
                assert_eq!(latest, generation(3));
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(s.current().unwrap().source(), "v3");
    assert_eq!(s.status().pending, None);
}

#[test]
fn concurrent_submissions_are_latest_wins() {
    for iteration in 0..40 {
        let s = Arc::new(session());
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads + 1));
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let observer = {
            let (s, done) = (Arc::clone(&s), Arc::clone(&done));
            thread::spawn(move || {
                let mut last = None;
                while !done.load(Ordering::Acquire) {
                    if let Some(snapshot) = s.current() {
                        let g = snapshot.generation();
                        assert!(
                            last.is_none_or(|l| l <= g),
                            "published generation regressed"
                        );
                        // Published content always belongs to its generation.
                        assert!(snapshot.source().starts_with(&format!("g{} ", g.get())));
                        last = Some(g);
                    }
                }
            })
        };
        let workers: Vec<_> = (0..threads)
            .map(|t| {
                let (s, barrier, submitted) =
                    (Arc::clone(&s), Arc::clone(&barrier), Arc::clone(&submitted));
                thread::spawn(move || {
                    barrier.wait();
                    for round in 0..4 {
                        // Hold the record lock so generation order matches payload order.
                        let job = {
                            let mut log = submitted.lock().unwrap();
                            let next = log.len() as u64 + 1;
                            let body = format!(
                                "g{next} thread {t} round {round}\n\n{}",
                                large(round * 50)
                            );
                            let job = s.submit(body.clone().into_bytes(), None).unwrap();
                            assert_eq!(job.generation().get(), next);
                            log.push(body);
                            job
                        };
                        match job.run() {
                            Outcome::Published(_)
                            | Outcome::Superseded { .. }
                            | Outcome::Cancelled { .. } => {}
                            other => panic!("{other:?}"),
                        }
                    }
                })
            })
            .collect();
        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        done.store(true, Ordering::Release);
        observer.join().unwrap();
        let log = submitted.lock().unwrap();
        let current = s.current().unwrap();
        // The newest submission is never superseded, so it always publishes.
        assert_eq!(
            current.generation().get(),
            log.len() as u64,
            "iteration {iteration}"
        );
        assert_eq!(current.source(), log.last().unwrap());
        assert!(s.status().is_current());
        assert_eq!(s.status().pending, None);
    }
}

#[test]
fn explicit_cancel_keeps_previous_snapshot() {
    let s = session();
    let first = publish(&s, "first");
    let job = s.submit(b"second".to_vec(), None).unwrap();
    assert!(s.cancel());
    assert!(!s.cancel());
    assert_eq!(s.status().pending, None);
    match job.run() {
        Outcome::Cancelled { generation: g } => assert_eq!(g, generation(2)),
        other => panic!("{other:?}"),
    }
    assert!(Arc::ptr_eq(&s.current().unwrap(), &first));
    let status = s.status();
    assert!(!status.is_current());
    assert_eq!(status.confirmed, Some(first.generation()));
    // Cancelling one job's token only affects that job.
    let job = s.submit(b"third".to_vec(), None).unwrap();
    job.cancellation_token().cancel();
    assert!(matches!(job.run(), Outcome::Cancelled { .. }));
    assert_eq!(s.current().unwrap().source(), "first");
}

#[test]
fn snapshots_outlive_session_and_jobs_observe_close() {
    let s = session();
    let snapshot = publish(&s, "# Keep\n\nbody");
    let job = s.submit(large(100).into_bytes(), None).unwrap();
    let token = job.cancellation_token().clone();
    let weak = Arc::downgrade(&snapshot);
    drop(s);
    assert!(token.is_cancelled());
    assert!(matches!(job.run(), Outcome::Closed { .. }));
    assert_eq!(snapshot.source(), "# Keep\n\nbody");
    assert_eq!(snapshot.v1().unwrap().input().outline[0].title, "Keep");
    assert!(snapshot.anchor_at(0).is_ok());
    drop(snapshot);
    assert!(weak.upgrade().is_none());
}

#[test]
fn session_dropped_on_another_thread_while_jobs_run() {
    for _ in 0..20 {
        let s = session();
        publish(&s, "base");
        let jobs: Vec<Job> = (0..4)
            .map(|i| s.submit(large(200 + i).into_bytes(), None).unwrap())
            .collect();
        let runners: Vec<_> = jobs
            .into_iter()
            .map(|job| thread::spawn(move || job.run()))
            .collect();
        let held = s.current().unwrap();
        drop(s);
        for runner in runners {
            match runner.join().unwrap() {
                Outcome::Published(snapshot) => assert!(snapshot.source().starts_with("Para")),
                Outcome::Superseded { .. } | Outcome::Cancelled { .. } | Outcome::Closed { .. } => {
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(held.source(), "base");
    }
}

#[test]
fn dropped_job_clears_pending() {
    let s = session();
    let job = s.submit(b"x".to_vec(), None).unwrap();
    assert_eq!(s.status().pending, Some(job.generation()));
    drop(job);
    assert_eq!(s.status().pending, None);
    assert!(s.current().is_none());
}

#[test]
fn unchanged_content_is_deduplicated() {
    let s = session();
    let first = match s.submit(b"# Same\n".to_vec(), token("t1")).unwrap().run() {
        Outcome::Published(snapshot) => snapshot,
        other => panic!("{other:?}"),
    };
    // Same token: no read needed.
    assert_eq!(
        s.external_change(ExternalChange::Modified { token: token("t1") }),
        Ok(ChangeDisposition::Unchanged)
    );
    for change in [token("t2"), None] {
        assert_eq!(
            s.external_change(ExternalChange::Modified { token: change }),
            Ok(ChangeDisposition::ReadRequired)
        );
    }
    // Notices consume no generation.
    assert_eq!(s.status().latest, Some(generation(1)));
    // Same bytes under a new token: no reparse, same Arc, confirmed generation.
    match s.submit(b"# Same\n".to_vec(), token("t2")).unwrap().run() {
        Outcome::Unchanged(snapshot) => assert!(Arc::ptr_eq(&snapshot, &first)),
        other => panic!("{other:?}"),
    }
    let status = s.status();
    assert_eq!(status.confirmed, Some(generation(2)));
    assert!(status.is_current());
    assert_eq!(first.generation(), generation(1));
    assert_eq!(
        s.external_change(ExternalChange::Modified { token: token("t2") }),
        Ok(ChangeDisposition::Unchanged)
    );
}

#[test]
fn deletion_and_reappearance_are_typed_states() {
    let s = session();
    let first = match s.submit(b"body".to_vec(), token("t1")).unwrap().run() {
        Outcome::Published(snapshot) => snapshot,
        other => panic!("{other:?}"),
    };
    let inflight = s.submit(b"newer".to_vec(), token("t2")).unwrap();
    assert_eq!(
        s.external_change(ExternalChange::Deleted),
        Ok(ChangeDisposition::Recorded(generation(3)))
    );
    assert!(inflight.cancellation_token().is_cancelled());
    assert!(matches!(inflight.run(), Outcome::Superseded { .. }));
    let status = s.status();
    assert_eq!(status.availability, Availability::Deleted);
    assert!(!status.is_current());
    assert!(Arc::ptr_eq(status.published.as_ref().unwrap(), &first));
    // A deleted document never deduplicates by token.
    assert_eq!(
        s.external_change(ExternalChange::Modified { token: token("t1") }),
        Ok(ChangeDisposition::ReadRequired)
    );
    // Reappearing with identical bytes confirms the existing snapshot.
    match s.submit(b"body".to_vec(), token("t1")).unwrap().run() {
        Outcome::Unchanged(snapshot) => assert!(Arc::ptr_eq(&snapshot, &first)),
        other => panic!("{other:?}"),
    }
    let status = s.status();
    assert_eq!(status.availability, Availability::Available);
    assert!(status.is_current());
    // Unreadable states carry the adapter's typed reason.
    for reason in [
        UnreadableReason::PermissionDenied,
        UnreadableReason::AccessRevoked,
        UnreadableReason::Busy,
        UnreadableReason::Io,
    ] {
        assert!(matches!(
            s.external_change(ExternalChange::Unreadable(reason)),
            Ok(ChangeDisposition::Recorded(_))
        ));
        assert_eq!(s.status().availability, Availability::Unreadable(reason));
    }
    // Reappearing with different bytes publishes a new generation.
    let changed = publish(&s, "changed");
    assert_eq!(changed.generation(), s.status().latest.unwrap());
    assert_eq!(s.status().availability, Availability::Available);
}

#[test]
fn failures_are_typed_and_keep_the_previous_snapshot() {
    let options = SessionOptions {
        limits: ParseLimits {
            model: ModelLimits {
                max_source_bytes: 64,
                ..ParseLimits::default().model
            },
            ..ParseLimits::default()
        },
        ..SessionOptions::default()
    };
    let s = DocumentSession::new(DocumentId("doc".into()), options).unwrap();
    let first = publish(&s, "ok");
    match s.submit(vec![0xff, 0xfe], token("bad")).unwrap().run() {
        Outcome::Failed {
            generation: g,
            error,
        } => {
            assert_eq!(g, generation(2));
            assert_eq!(error, SessionError::InvalidUtf8);
        }
        other => panic!("{other:?}"),
    }
    let status = s.status();
    assert_eq!(
        status.failure,
        Some((generation(2), SessionError::InvalidUtf8))
    );
    assert!(!status.is_current());
    assert!(Arc::ptr_eq(&s.current().unwrap(), &first));
    // The failing revision is accepted: an identical notice does not retry.
    assert_eq!(
        s.external_change(ExternalChange::Modified {
            token: token("bad")
        }),
        Ok(ChangeDisposition::Unchanged)
    );
    match s.submit(vec![b'a'; 65], None).unwrap().run() {
        Outcome::Failed { error, .. } => {
            assert_eq!(error, SessionError::Parse(ParseError::SourceLimit))
        }
        other => panic!("{other:?}"),
    }
    let recovered = publish(&s, "fine");
    assert_eq!(s.status().failure, None);
    assert!(Arc::ptr_eq(&s.current().unwrap(), &recovered));
}

#[test]
fn invalid_inputs_are_rejected_without_panics() {
    assert_eq!(
        DocumentSession::new(DocumentId("bad id".into()), SessionOptions::default()).err(),
        Some(SessionError::InvalidDocumentId)
    );
    assert_eq!(
        RevisionToken::new(Vec::new()).err(),
        Some(SessionError::RevisionToken)
    );
    assert_eq!(
        RevisionToken::new(vec![1; MAX_REVISION_TOKEN_BYTES + 1]).err(),
        Some(SessionError::RevisionToken)
    );
    let t = RevisionToken::new(b"secret-ish".to_vec()).unwrap();
    assert_eq!(t.as_bytes(), b"secret-ish");
    assert!(!format!("{t:?}").contains("secret"));
}

// --- Anchors ---------------------------------------------------------------

fn anchor_on(snapshot: &Snapshot, needle: &str, within: usize) -> Anchor {
    let at = snapshot.source().find(needle).unwrap() + within;
    snapshot.anchor_at(at as u64).unwrap()
}
fn resolve(snapshot: &Snapshot, anchor: &Anchor) -> ResolvedAnchor {
    snapshot.resolve(anchor).unwrap().unwrap()
}
fn text_at(snapshot: &Snapshot, r: &ResolvedAnchor) -> String {
    snapshot.source()[r.source_offset as usize..]
        .chars()
        .take(8)
        .collect()
}

#[test]
fn anchor_survives_insert_above() {
    let s = session();
    let old = publish(&s, "# Title\n\nAlpha para.\n\nBeta para here.\n");
    let anchor = anchor_on(&old, "Beta", 5);
    assert_eq!(anchor.offset, 5);
    assert_eq!(anchor.kind, BlockKind::Paragraph);
    let new = publish(
        &s,
        "# Title\n\nInserted first.\n\nAnother insert.\n\nAlpha para.\n\nBeta para here.\n",
    );
    let r = resolve(&new, &anchor);
    assert_eq!(r.quality, AnchorMatch::Exact);
    assert_eq!(r.offset, 5);
    assert_eq!(text_at(&new, &r), "para her");
    assert_ne!(r.node, anchor.node);
    // Same snapshot resolves to itself.
    let same = resolve(&old, &anchor);
    assert_eq!(
        (same.quality, same.node.clone()),
        (AnchorMatch::Exact, anchor.node.clone())
    );
}

#[test]
fn anchor_on_deleted_block_moves_to_nearest_survivor() {
    let s = session();
    let old = publish(&s, "One.\n\nTwo.\n\nThree.\n\nFour.\n");
    let anchor = anchor_on(&old, "Two", 2);
    let new = publish(&s, "One.\n\nThree.\n\nFour.\n");
    let r = resolve(&new, &anchor);
    assert_eq!(r.quality, AnchorMatch::Nearest);
    assert_eq!(r.offset, 0);
    assert_eq!(text_at(&new, &r), "Three.\n\n");
    // Predecessor gone too: fall back to the old successor.
    let new = publish(&s, "Three.\n\nFour.\n");
    let r = resolve(&new, &anchor);
    assert_eq!(
        (r.quality, text_at(&new, &r)),
        (AnchorMatch::Nearest, "Three.\n\n".into())
    );
    // Neighbours gone: the enclosing heading, then the proportional position.
    let old = publish(&s, "# Sec\n\nA.\n\nB.\n\nC.\n");
    let anchor = anchor_on(&old, "B.", 0);
    let new = publish(&s, "# Sec\n\nX.\n\nY.\n");
    let r = resolve(&new, &anchor);
    assert_eq!(
        (r.quality, text_at(&new, &r)),
        (AnchorMatch::Nearest, "# Sec\n\nX".into())
    );
    let new = publish(&s, "P.\n\nQ.\n\nR.\n\nS.\n");
    let r = resolve(&new, &anchor);
    assert_eq!(r.quality, AnchorMatch::Nearest);
}

#[test]
fn anchor_follows_renamed_heading_and_edited_paragraph() {
    let s = session();
    let old = publish(&s, "# Intro\n\nText.\n\n## Usage here\n\nMore.\n");
    let heading = anchor_on(&old, "## Usage", 9);
    assert_eq!(heading.kind, BlockKind::Heading);
    let new = publish(&s, "# Intro\n\nText.\n\n## How\n\nMore.\n");
    let r = resolve(&new, &heading);
    assert_eq!(r.quality, AnchorMatch::Relocated);
    // Offset clamps to the shorter heading (its span includes the line ending).
    assert_eq!(r.offset, "## How\n".len() as u64);
    assert_eq!(new.v1().unwrap().input().outline[1].node, r.node);
    // Editing the anchored paragraph in place keeps the intra-block offset.
    let para = anchor_on(&new, "Text", 2);
    let edited = publish(&s, "# Intro\n\nTexture changed.\n\n## How\n\nMore.\n");
    let r = resolve(&edited, &para);
    assert_eq!((r.quality, r.offset), (AnchorMatch::Relocated, 2));
    assert_eq!(text_at(&edited, &r), "xture ch");
}

#[test]
fn duplicate_blocks_resolve_by_context() {
    let s = session();
    let old = publish(&s, "Same.\n\nA.\n\nSame.\n\nB.\n");
    let second = anchor_on(&old, "Same.\n\nB", 1);
    assert_eq!(second.occurrence, 1);
    // A new duplicate above shifts occurrence ordinals; context still wins.
    let new = publish(&s, "Same.\n\nSame.\n\nA.\n\nSame.\n\nB.\n");
    let r = resolve(&new, &second);
    assert_eq!(r.quality, AnchorMatch::Exact);
    assert_eq!(
        r.source_offset as usize,
        new.source().find("Same.\n\nB").unwrap() + 1
    );
}

#[test]
fn anchors_in_containers_and_inline_nodes() {
    let s = session();
    let old = publish(
        &s,
        "> quoted line\n\n- item one\n- item two\n\n| a |\n| - |\n| b |\n",
    );
    let item = anchor_on(&old, "item two", 5);
    assert_eq!(item.kind, BlockKind::Paragraph);
    let table = anchor_on(&old, "| b", 2);
    assert_eq!(table.kind, BlockKind::Table);
    let new = publish(
        &s,
        "Intro.\n\n> quoted line\n\n- item zero\n- item one\n- item two\n\n| a |\n| - |\n| b |\n",
    );
    let r = resolve(&new, &item);
    assert_eq!(
        (r.quality, text_at(&new, &r)),
        (AnchorMatch::Exact, "two\n\n| a".into())
    );
    assert_eq!(resolve(&new, &table).quality, AnchorMatch::Exact);
    // anchor_for maps any node (here an inline text node) to its block.
    let doc = new.v1().unwrap().input();
    let text = doc
        .nodes
        .iter()
        .find(|n| matches!(&n.content, NodeKind::Text { text } if text == "quoted line"))
        .unwrap();
    let a = new.anchor_for(&text.id, 3).unwrap();
    assert_eq!(a.source_offset, text.span.start + 3);
    assert_eq!(
        new.anchor_for(&NodeId("missing".into()), 0),
        Err(AnchorError::UnknownNode)
    );
    // Offsets between or past blocks snap to a block.
    let gap = new.source().find("\n\n").unwrap() as u64 + 1;
    assert!(new.anchor_at(gap).unwrap().source_offset > gap);
    let end = new.anchor_at(u64::MAX).unwrap();
    assert_eq!(end.kind, BlockKind::Table);
}

#[test]
fn empty_documents_and_lost_anchors() {
    let s = session();
    let old = publish(&s, "Only.\n");
    let anchor = anchor_on(&old, "Only", 0);
    let empty = publish(&s, "");
    assert_eq!(empty.anchor_at(0), Err(AnchorError::NoBlocks));
    assert_eq!(empty.resolve(&anchor), Ok(None));
    let whitespace = publish(&s, "\n\n   \n");
    assert_eq!(whitespace.resolve(&anchor), Ok(None));
    // Back to content: a full rewrite shares no real neighbour, so absent ==
    // absent context earns no relocation; the block is found by position.
    let back = publish(&s, "Different.\n");
    let r = resolve(&back, &anchor);
    assert_eq!(r.quality, AnchorMatch::Nearest);
    assert_eq!(r.source_offset, 0);
}

#[test]
fn anchors_round_trip_and_tolerate_forgery() {
    let s = session();
    let old = publish(&s, "# Café\n\nNaïve résumé text.\n");
    let anchor = anchor_on(&old, "résumé", 1);
    let wire = serde_json::to_string(&anchor).unwrap();
    let restored: Anchor = serde_json::from_str(&wire).unwrap();
    assert_eq!(restored, anchor);
    assert!(
        serde_json::from_str::<Anchor>(&wire.replace("\"version\"", "\"extra\":1,\"version\""))
            .is_err()
    );
    // A new session (for example after relaunch) resolves persisted anchors.
    let relaunched = session();
    let new = publish(&relaunched, "# Café\n\nNaïve résumé text.\n");
    let r = resolve(&new, &restored);
    assert_eq!(r.quality, AnchorMatch::Exact);
    assert_eq!(r.source_offset, anchor.source_offset);
    // Forged values never panic and always land on UTF-8 boundaries.
    for (offset, source_offset, source_len) in [
        (u64::MAX, u64::MAX, 0),
        (3, 0, u64::MAX),
        (u64::MAX / 2, 7, 1),
    ] {
        let mut forged = anchor.clone();
        forged.offset = offset;
        forged.source_offset = source_offset;
        forged.source_len = source_len;
        let r = resolve(&new, &forged);
        assert!(new.source().is_char_boundary(r.source_offset as usize));
        forged.fingerprint ^= 1;
        forged.previous = Some(u64::MAX);
        forged.next = None;
        let r = resolve(&new, &forged);
        assert!(new.source().is_char_boundary(r.source_offset as usize));
    }
    // Intra-block offsets inside a multi-byte character floor to a boundary.
    let mut inside = anchor.clone();
    inside.offset = "Naï".len() as u64 - 1;
    let r = resolve(&new, &inside);
    assert_eq!(r.offset, "Na".len() as u64);
    let mut future = anchor;
    future.version = ANCHOR_VERSION + 1;
    assert_eq!(new.resolve(&future), Err(AnchorError::UnsupportedVersion));
}

#[cfg(feature = "model-v2")]
#[test]
fn v2_sessions_publish_v2_snapshots() {
    let s = DocumentSession::new(
        DocumentId("doc".into()),
        SessionOptions {
            model: ModelVersion::V2,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    let snapshot = publish(&s, "[a]\n\n[a]: /x\n\nTail.\n");
    assert!(snapshot.v1().is_none());
    let v2 = snapshot.v2().unwrap();
    assert_eq!(v2.input().links.len(), 1);
    let anchor = anchor_on(&snapshot, "Tail", 2);
    let next = publish(&s, "New.\n\n[a]\n\n[a]: /x\n\nTail.\n");
    assert_eq!(resolve(&next, &anchor).quality, AnchorMatch::Exact);
}

#[test]
fn edited_block_in_context_beats_duplicate_elsewhere() {
    let s = session();
    let old = publish(
        &s,
        "# S1\n\nSee above.\n\nfiller\n\n# S2\n\nintro\n\nSee above.\n\noutro",
    );
    let at = old.source().rfind("See above.").unwrap();
    let anchor = old.anchor_at(at as u64 + 4).unwrap();
    let new = publish(
        &s,
        "# S1\n\nSee above.\n\nfiller\n\n# S2\n\nintro\n\nSee above, edited.\n\noutro",
    );
    let r = resolve(&new, &anchor);
    assert_eq!(r.quality, AnchorMatch::Relocated);
    assert_eq!(
        r.source_offset as usize,
        new.source().find("See above, edited").unwrap() + 4
    );
    // Unedited, the in-context duplicate is still chosen exactly.
    let same = resolve(&old, &anchor);
    assert_eq!(
        (same.quality, same.source_offset),
        (AnchorMatch::Exact, at as u64 + 4)
    );
    // Anchored block deleted while its old neighbours survive: the surviving
    // neighbour pins the position, not the unrelated identical copy in S1.
    let deleted = publish(&s, "# S1\n\nSee above.\n\nfiller\n\n# S2\n\nintro\n\noutro");
    let r = resolve(&deleted, &anchor);
    assert_eq!(r.quality, AnchorMatch::Nearest);
    assert_eq!(
        r.source_offset as usize,
        deleted.source().find("outro").unwrap()
    );
    // Full rewrite around it: with no surviving neighbour the identical copy
    // is still trusted (same-heading context alone is not required).
    let rewritten = publish(&s, "# S1\n\nSee above.\n\nfiller\n\n# S2\n\nnew\n\nother\n");
    assert_eq!(resolve(&rewritten, &anchor).quality, AnchorMatch::Exact);
}

#[test]
fn modified_notice_for_pending_submission_does_not_force_reread() {
    let s = session();
    let job = s.submit(b"v9".to_vec(), token("t9")).unwrap();
    for _ in 0..3 {
        assert_eq!(
            s.external_change(ExternalChange::Modified { token: token("t9") }),
            Ok(ChangeDisposition::Pending(job.generation()))
        );
    }
    assert_eq!(
        s.external_change(ExternalChange::Modified {
            token: token("t10")
        }),
        Ok(ChangeDisposition::ReadRequired)
    );
    // The pending job was not superseded and still publishes.
    assert!(matches!(job.run(), Outcome::Published(_)));
    assert_eq!(
        s.external_change(ExternalChange::Modified { token: token("t9") }),
        Ok(ChangeDisposition::Unchanged)
    );
    // A cancelled pending job is neither reported as pending nor deduplicated.
    let job = s.submit(b"v10".to_vec(), token("t10")).unwrap();
    job.cancellation_token().cancel();
    assert_eq!(s.status().pending, None);
    assert_eq!(
        s.external_change(ExternalChange::Modified {
            token: token("t10")
        }),
        Ok(ChangeDisposition::ReadRequired)
    );
    assert!(!s.cancel());
    assert!(matches!(job.run(), Outcome::Cancelled { .. }));
}

#[test]
fn job_run_after_session_drop_is_closed() {
    let s = session();
    let job = s.submit(b"late".to_vec(), None).unwrap();
    let g = job.generation();
    drop(s);
    assert!(job.cancellation_token().is_cancelled());
    match job.run() {
        Outcome::Closed { generation: x } => assert_eq!(x, g),
        other => panic!("{other:?}"),
    }
}

#[test]
fn repeated_availability_notices_do_not_consume_generations() {
    let s = session();
    publish(&s, "body");
    let first = s.external_change(ExternalChange::Deleted).unwrap();
    assert_eq!(first, ChangeDisposition::Recorded(generation(2)));
    assert_eq!(s.external_change(ExternalChange::Deleted), Ok(first));
    assert_eq!(s.status().latest, Some(generation(2)));
    // A different state, or a state with work in flight, still advances.
    let unreadable = ExternalChange::Unreadable(UnreadableReason::Busy);
    assert_eq!(
        s.external_change(unreadable.clone()),
        Ok(ChangeDisposition::Recorded(generation(3)))
    );
    let job = s.submit(b"newer".to_vec(), None).unwrap();
    assert_eq!(
        s.external_change(unreadable),
        Ok(ChangeDisposition::Recorded(generation(5)))
    );
    assert!(matches!(job.run(), Outcome::Superseded { .. }));
}

#[test]
fn deleted_block_with_duplicate_elsewhere_stays_near_its_neighbours() {
    let s = session();
    let old = publish(
        &s,
        "# T\n\nalpha\n\n---\n\nbeta\n\ngamma\n\ndelta\n\neps\n\n---\n\nzeta",
    );
    let anchor = old
        .anchor_at(old.source().find("---").unwrap() as u64)
        .unwrap();
    assert_eq!(anchor.kind, BlockKind::ThematicBreak);
    let new = publish(
        &s,
        "# T\n\nalpha\n\nbeta\n\ngamma\n\ndelta\n\neps\n\n---\n\nzeta",
    );
    let r = resolve(&new, &anchor);
    assert_eq!(r.quality, AnchorMatch::Nearest);
    assert_eq!(r.source_offset, 12);
    assert_eq!(text_at(&new, &r), "beta\n\nga");
}

#[test]
fn reverted_content_with_newer_submission_in_flight_requires_read() {
    let s = session();
    match s.submit(b"A".to_vec(), token("tA")).unwrap().run() {
        Outcome::Published(_) => {}
        other => panic!("{other:?}"),
    }
    let b = s.submit(b"B".to_vec(), token("tB")).unwrap();
    // Disk reverted to A while B is in flight: A is not the newest known content.
    assert_eq!(
        s.external_change(ExternalChange::Modified { token: token("tA") }),
        Ok(ChangeDisposition::ReadRequired)
    );
    assert!(matches!(b.run(), Outcome::Published(_)));
    // The adapter re-reads A; it supersedes nothing and republishes A.
    match s.submit(b"A".to_vec(), token("tA")).unwrap().run() {
        Outcome::Published(snapshot) => assert_eq!(snapshot.source(), "A"),
        other => panic!("{other:?}"),
    }
    assert!(s.status().is_current());
    assert_eq!(s.current().unwrap().source(), "A");
}
