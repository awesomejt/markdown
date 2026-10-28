//! Tests call the exported functions exactly as a C host would. They share
//! the process-wide registry, so each takes `serial()` and asserts that every
//! handle it created was released.
use crate::abi::*;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

static SERIAL: Mutex<()> = Mutex::new(());
struct Serial(#[allow(dead_code)] MutexGuard<'static, ()>);
impl Drop for Serial {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(live(), (0, 0, 0), "handles leaked by this test");
            assert!(registry().encodings.is_empty(), "encoding index not pruned");
        }
    }
}
fn serial() -> Serial {
    let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(live(), (0, 0, 0), "handles leaked by an earlier test");
    Serial(guard)
}
fn live() -> (u64, u64, u64) {
    let (mut s, mut j, mut n) = (0, 0, 0);
    // SAFETY: valid aligned out-pointers.
    assert_eq!(unsafe { md_live_handles(&mut s, &mut j, &mut n) }, MD_OK);
    (s, j, n)
}
fn inject_panic() {
    INJECT_PANIC.with(|flag| flag.set(true));
}

fn options(model: u32) -> md_session_options {
    md_session_options {
        struct_size: size::<md_session_options>(),
        model_version: model,
        ..md_session_options::default()
    }
}
fn create_with(options: &md_session_options) -> Result<md_session, md_status> {
    let id = b"doc";
    let mut handle = 0;
    // SAFETY: valid pointers for the call.
    match unsafe { md_session_create(id.as_ptr(), id.len(), options, &mut handle) } {
        MD_OK => Ok(handle),
        status => {
            assert_eq!(handle, 0);
            Err(status)
        }
    }
}
fn create(model: u32) -> md_session {
    create_with(&options(model)).unwrap()
}
fn submit_token(session: md_session, source: &[u8], token: &[u8]) -> (md_job, u64) {
    let (mut job, mut generation) = (0, 0);
    // SAFETY: valid pointers for the call.
    let status = unsafe {
        md_session_submit(
            session,
            source.as_ptr(),
            source.len(),
            token.as_ptr(),
            token.len(),
            &mut job,
            &mut generation,
        )
    };
    assert_eq!(status, MD_OK);
    (job, generation)
}
fn submit(session: md_session, source: &str) -> md_job {
    submit_token(session, source.as_bytes(), b"").0
}
fn result() -> md_job_result {
    md_job_result {
        struct_size: size::<md_job_result>(),
        ..md_job_result::default()
    }
}
/// Run and release a job.
fn run(job: md_job) -> md_job_result {
    let mut out = result();
    // SAFETY: valid pointer for the call.
    assert_eq!(unsafe { md_job_run(job, &mut out) }, MD_OK);
    assert_eq!(md_job_release(job), MD_OK);
    out
}
fn publish(session: md_session, source: &str) -> md_snapshot {
    let out = run(submit(session, source));
    assert_eq!(out.outcome, MD_OUTCOME_PUBLISHED, "{out:?}");
    assert_ne!(out.snapshot, 0);
    out.snapshot
}
fn view(snapshot: md_snapshot) -> md_snapshot_view {
    let mut view = md_snapshot_view {
        struct_size: size::<md_snapshot_view>(),
        ..md_snapshot_view::default()
    };
    // SAFETY: valid pointer for the call.
    assert_eq!(unsafe { md_snapshot_get_view(snapshot, &mut view) }, MD_OK);
    view
}
fn status(session: md_session) -> md_session_status_info {
    let mut out = md_session_status_info {
        struct_size: size::<md_session_status_info>(),
        ..md_session_status_info::default()
    };
    // SAFETY: valid pointer for the call.
    assert_eq!(unsafe { md_session_get_status(session, &mut out) }, MD_OK);
    out
}
fn model(snapshot: md_snapshot) -> Arc<Snapshot> {
    Arc::clone(&self::snapshot(snapshot).unwrap().snapshot)
}
fn corpus() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut sources: Vec<String> = [
        "fixtures/markdown/foundation/gfm-tour.md",
        "fixtures/markdown/foundation/code-fences.md",
        "fixtures/markdown/foundation/unsafe-html.md",
        "fixtures/markdown/advanced/math-alerts.md",
        "fixtures/markdown/advanced/inline-diagrams.md",
        "fixtures/markdown/advanced/voice-parity.md",
        "fixtures/markdown/paths/unicode-filename.md",
        "fixtures/markdown/paths/nfd-filename.md",
        "specs/semantic-model/v1/fixtures/containers.source.txt",
        "specs/semantic-model/v1/fixtures/fallback.source.txt",
        "specs/semantic-model/v1/fixtures/gfm.source.txt",
        "specs/semantic-model/v1/fixtures/resources.source.txt",
        "specs/semantic-model/v1/fixtures/unicode.source.txt",
        "specs/semantic-model/v2/fixtures/references.source.txt",
    ]
    .iter()
    .map(|path| std::fs::read_to_string(root.join(path)).unwrap())
    .collect();
    sources.push(String::new());
    sources.push(
        "---\ntitle: Front\ntags: [a, b]\ncount: 3\ndraft: false\nnone: null\n---\n\n\
         # Head\n\n> [!NOTE]\n> alert\n\n3. three\n4. four\n\n- [x] done\n- [ ] todo\n\n\
         | a | b |\n|:-|-:|\n| [l](#head \"t\") | ![i](x.png \"ti\") |\n\n\
         Text[^1] ~~gone~~ `c`  \nbreak\n\n[^1]: note\n\n<div>html</div>\n\n\
         [ref]: https://example.com/a\n\n[ref] [x](javascript:alert(1))\n"
            .into(),
    );
    sources
}

// ---------------------------------------------------------------------------
// Decoding the bulk view back into model values, as an adapter would.

/// Read-only access to a view's arrays (null only with a zero count).
struct Tables<'a> {
    v: &'a md_snapshot_view,
}
impl<'a> Tables<'a> {
    fn slice<T>(ptr: *const T, len: u32) -> &'a [T] {
        if len == 0 {
            assert!(ptr.is_null(), "empty arrays are NULL");
            return &[];
        }
        assert!(!ptr.is_null() && ptr.is_aligned());
        // SAFETY: the view promises `len` initialized records at `ptr`,
        // valid while the snapshot handle is held (it is, for the test).
        unsafe { std::slice::from_raw_parts(ptr, len as usize) }
    }
    fn bytes(ptr: *const u8, len: u64) -> &'a [u8] {
        Self::slice(ptr, u32::try_from(len).unwrap())
    }
    fn str(&self, s: md_str) -> String {
        let strings = Self::bytes(self.v.strings, self.v.strings_length);
        let range = s.offset as usize..(s.offset + s.length) as usize;
        String::from_utf8(strings[range].to_vec()).unwrap()
    }
    fn opt(&self, s: md_str, flags: u32, flag: u32) -> Option<String> {
        (flags & flag != 0).then(|| self.str(s))
    }
    fn nodes(&self) -> &'a [md_node] {
        Self::slice(self.v.nodes, self.v.node_count)
    }
    fn id(&self, index: u32) -> NodeId {
        NodeId(self.str(self.nodes()[index as usize].id))
    }
    fn span(start: u32, end: u32) -> SourceSpan {
        SourceSpan {
            start: start.into(),
            end: end.into(),
        }
    }
    fn link_id(&self, index: u32) -> LinkId {
        LinkId(self.str(Self::slice(self.v.links, self.v.link_count)[index as usize].id))
    }
    fn resource_id(&self, index: u32) -> ResourceId {
        ResourceId(
            self.str(Self::slice(self.v.resources, self.v.resource_count)[index as usize].id),
        )
    }

    fn structure(&self) -> DocumentInput {
        let v = self.v;
        let children = Self::slice(v.children, v.child_count);
        let alignments = Self::slice(v.alignments, v.alignment_count);
        let nodes = self.nodes();
        for (i, n) in nodes.iter().enumerate() {
            for &c in &children[n.first_child as usize..(n.first_child + n.child_count) as usize] {
                assert_eq!(nodes[c as usize].parent, i as u32);
            }
        }
        let node = |n: &md_node| -> Node {
            let kids = || -> Vec<NodeId> {
                children[n.first_child as usize..(n.first_child + n.child_count) as usize]
                    .iter()
                    .map(|&c| self.id(c))
                    .collect()
            };
            let alert = [
                AlertKind::Note,
                AlertKind::Tip,
                AlertKind::Important,
                AlertKind::Warning,
                AlertKind::Caution,
            ];
            let reasons = [
                UnsupportedReason::Unavailable,
                UnsupportedReason::Disabled,
                UnsupportedReason::InvalidInput,
            ];
            let content = match n.kind {
                MD_NODE_PARAGRAPH => NodeKind::Paragraph { children: kids() },
                MD_NODE_HEADING => NodeKind::Heading {
                    level: n.value as u8,
                    anchor: self.str(n.text),
                    children: kids(),
                },
                MD_NODE_BLOCK_QUOTE => NodeKind::BlockQuote { children: kids() },
                MD_NODE_LIST => NodeKind::List {
                    ordered: n.flags & MD_NODE_ORDERED != 0,
                    start: (n.flags & MD_NODE_HAS_START != 0).then_some(n.number),
                    tight: n.flags & MD_NODE_TIGHT != 0,
                    items: kids(),
                },
                MD_NODE_LIST_ITEM => NodeKind::ListItem {
                    checked: (n.flags & MD_NODE_HAS_CHECKED != 0)
                        .then_some(n.flags & MD_NODE_CHECKED != 0),
                    children: kids(),
                },
                MD_NODE_CODE_BLOCK => NodeKind::CodeBlock {
                    info: self.opt(n.extra, n.flags, MD_NODE_HAS_INFO),
                    text: self.str(n.text),
                },
                MD_NODE_THEMATIC_BREAK => NodeKind::ThematicBreak {},
                MD_NODE_TABLE => NodeKind::Table {
                    alignments: alignments[n.aux as usize..(n.aux + n.aux_count) as usize]
                        .iter()
                        .map(|a| {
                            [
                                Alignment::None,
                                Alignment::Left,
                                Alignment::Center,
                                Alignment::Right,
                            ][*a as usize]
                        })
                        .collect(),
                    rows: kids(),
                },
                MD_NODE_TABLE_ROW => NodeKind::TableRow {
                    header: n.flags & MD_NODE_HEADER != 0,
                    cells: kids(),
                },
                MD_NODE_TABLE_CELL => NodeKind::TableCell { children: kids() },
                MD_NODE_FOOTNOTE_DEFINITION => NodeKind::FootnoteDefinition {
                    label: self.str(n.text),
                    children: kids(),
                },
                MD_NODE_ALERT => NodeKind::Alert {
                    category: alert[n.value as usize - 1],
                    children: kids(),
                },
                MD_NODE_UNSUPPORTED_BLOCK => NodeKind::UnsupportedBlock {
                    feature: self.str(n.text),
                    reason: reasons[n.value as usize - 1],
                },
                MD_NODE_TEXT => NodeKind::Text {
                    text: self.str(n.text),
                },
                MD_NODE_EMPHASIS => NodeKind::Emphasis { children: kids() },
                MD_NODE_STRONG => NodeKind::Strong { children: kids() },
                MD_NODE_STRIKETHROUGH => NodeKind::Strikethrough { children: kids() },
                MD_NODE_CODE_SPAN => NodeKind::CodeSpan {
                    text: self.str(n.text),
                },
                MD_NODE_SOFT_BREAK => NodeKind::SoftBreak {},
                MD_NODE_HARD_BREAK => NodeKind::HardBreak {},
                MD_NODE_LINK => NodeKind::Link {
                    target: self.link_id(n.aux),
                    title: self.opt(n.extra, n.flags, MD_NODE_HAS_TITLE),
                    children: kids(),
                },
                MD_NODE_IMAGE => NodeKind::Image {
                    resource: self.resource_id(n.aux),
                    alt: self.str(n.text),
                    title: self.opt(n.extra, n.flags, MD_NODE_HAS_TITLE),
                },
                MD_NODE_FOOTNOTE_REFERENCE => NodeKind::FootnoteReference {
                    label: self.str(n.text),
                },
                MD_NODE_UNSUPPORTED_INLINE => NodeKind::UnsupportedInline {
                    feature: self.str(n.text),
                    reason: reasons[n.value as usize - 1],
                },
                other => panic!("unknown kind {other}"),
            };
            Node {
                id: NodeId(self.str(n.id)),
                span: Self::span(n.span_start, n.span_end),
                content,
            }
        };
        let capabilities = [
            (MD_CAP_TABLES, Capability::Tables),
            (MD_CAP_TASK_LISTS, Capability::TaskLists),
            (MD_CAP_FOOTNOTES, Capability::Footnotes),
            (MD_CAP_STRIKETHROUGH, Capability::Strikethrough),
            (MD_CAP_FRONT_MATTER, Capability::FrontMatter),
            (MD_CAP_ALERTS, Capability::Alerts),
        ]
        .into_iter()
        .filter(|(bit, _)| v.capabilities & bit != 0)
        .map(|(_, c)| c)
        .collect::<BTreeSet<_>>();
        let string_list = Self::slice(v.string_list, v.string_list_count);
        let properties = Self::slice(v.properties, v.property_count)
            .iter()
            .map(|p| {
                let value = match p.kind {
                    MD_VALUE_NULL => MetadataValue::Null {},
                    MD_VALUE_BOOLEAN => MetadataValue::Boolean {
                        value: p.boolean != 0,
                    },
                    MD_VALUE_INTEGER => MetadataValue::Integer { value: p.integer },
                    MD_VALUE_STRING => MetadataValue::String {
                        value: self.str(p.string),
                    },
                    MD_VALUE_STRINGS => MetadataValue::Strings {
                        value: string_list
                            [p.first_string as usize..(p.first_string + p.string_count) as usize]
                            .iter()
                            .map(|s| self.str(*s))
                            .collect(),
                    },
                    other => panic!("unknown value kind {other}"),
                };
                (self.str(p.key), value)
            })
            .collect::<BTreeMap<_, _>>();
        DocumentInput {
            model_version: 1,
            document_id: DocumentId(self.str(v.document_id)),
            revision_id: RevisionId(self.str(v.revision_id)),
            source: SourceInfo {
                bytes: v.source_length,
                sha256: self.str(v.source_sha256),
            },
            capabilities,
            roots: Self::slice(v.roots, v.root_count)
                .iter()
                .map(|&r| self.id(r))
                .collect(),
            nodes: nodes.iter().map(node).collect(),
            outline: Self::slice(v.outline, v.outline_count)
                .iter()
                .map(|o| OutlineEntry {
                    node: self.id(o.node),
                    level: o.level as u8,
                    title: self.str(o.title),
                    anchor: self.str(o.anchor),
                })
                .collect(),
            metadata: Metadata {
                front_matter_span: (v.flags & MD_VIEW_HAS_FRONT_MATTER != 0)
                    .then(|| Self::span(v.front_matter_start, v.front_matter_end)),
                title: self.opt(v.title, v.flags, MD_VIEW_HAS_TITLE),
                properties,
            },
            links: Vec::new(),
            resources: Vec::new(),
            diagnostics: Self::slice(v.diagnostics, v.diagnostic_count)
                .iter()
                .map(|d| Diagnostic {
                    code: self.str(d.code),
                    severity: [Severity::Info, Severity::Warning, Severity::Error]
                        [d.severity as usize - 1],
                    message: self.str(d.message),
                    span: (d.flags & MD_DIAGNOSTIC_HAS_SPAN != 0)
                        .then(|| Self::span(d.span_start, d.span_end)),
                    node: (d.node != MD_NONE).then(|| self.id(d.node)),
                })
                .collect(),
        }
    }

    fn v1(&self) -> DocumentInput {
        let mut d = self.structure();
        d.links = Self::slice(self.v.links, self.v.link_count)
            .iter()
            .map(|i| LinkIntent {
                id: LinkId(self.str(i.id)),
                span: Self::span(i.span_start, i.span_end),
                destination: self.str(i.destination),
                disposition: match i.target_kind {
                    MD_TARGET_FRAGMENT => LinkDisposition::Fragment {
                        anchor: self.str(i.target),
                    },
                    MD_TARGET_REQUIRES_CAPABILITY => LinkDisposition::RequiresCapability {},
                    MD_TARGET_EXTERNAL => LinkDisposition::External {},
                    MD_TARGET_DENIED => LinkDisposition::Denied {
                        reason: self.str(i.target),
                    },
                    other => panic!("{other}"),
                },
            })
            .collect();
        d.resources = Self::slice(self.v.resources, self.v.resource_count)
            .iter()
            .map(|i| ResourceIntent {
                id: ResourceId(self.str(i.id)),
                span: Self::span(i.span_start, i.span_end),
                destination: self.str(i.destination),
                access: match i.target_kind {
                    MD_TARGET_REQUIRES_CAPABILITY => ResourceAccess::RequiresCapability {},
                    MD_TARGET_DENIED => ResourceAccess::Denied {
                        reason: self.str(i.target),
                    },
                    other => panic!("{other}"),
                },
            })
            .collect();
        d
    }

    #[cfg(feature = "model-v2")]
    fn v2(&self) -> markdown_core::v2::DocumentInput {
        use markdown_core::v2::{Denial, IntentSource, Target};
        let d = self.structure();
        let intent = |i: &md_intent| {
            let span = Self::span(i.span_start, i.span_end);
            let source = match i.source_kind {
                MD_SOURCE_INLINE => IntentSource::Inline { span },
                MD_SOURCE_REFERENCE => IntentSource::Reference {
                    definition: Self::span(i.definition_start, i.definition_end),
                    span,
                },
                other => panic!("{other}"),
            };
            let target = match i.target_kind {
                MD_TARGET_FRAGMENT => Target::Fragment {
                    anchor: self.str(i.target),
                },
                MD_TARGET_REQUIRES_CAPABILITY => Target::RequiresCapability {
                    path: self.str(i.target),
                    fragment: self.opt(i.fragment, i.flags, MD_INTENT_HAS_FRAGMENT),
                },
                MD_TARGET_EXTERNAL => Target::External {
                    uri: self.str(i.target),
                },
                MD_TARGET_DENIED => Target::Denied {
                    reason: [
                        Denial::Unsafe,
                        Denial::InvalidUri,
                        Denial::UnsupportedScheme,
                        Denial::RemoteResource,
                        Denial::Limit,
                        Denial::LegacyPolicy,
                    ][i.denial as usize - 1],
                },
                other => panic!("{other}"),
            };
            (self.str(i.id), source, self.str(i.destination), target)
        };
        markdown_core::v2::DocumentInput {
            model_version: 2,
            document_id: d.document_id,
            revision_id: d.revision_id,
            source: d.source,
            capabilities: d.capabilities,
            roots: d.roots,
            nodes: d.nodes,
            outline: d.outline,
            metadata: d.metadata,
            links: Self::slice(self.v.links, self.v.link_count)
                .iter()
                .map(|i| {
                    let (id, source, destination, target) = intent(i);
                    markdown_core::v2::LinkIntent {
                        id: LinkId(id),
                        source,
                        destination,
                        target,
                    }
                })
                .collect(),
            resources: Self::slice(self.v.resources, self.v.resource_count)
                .iter()
                .map(|i| {
                    let (id, source, destination, target) = intent(i);
                    markdown_core::v2::ResourceIntent {
                        id: ResourceId(id),
                        source,
                        destination,
                        target,
                    }
                })
                .collect(),
            diagnostics: d.diagnostics,
        }
    }
}

// ---------------------------------------------------------------------------
// Version negotiation, names and argument validation

#[test]
fn negotiation_reports_layout_and_rejects_mismatches() {
    let _s = serial();
    let mut info = md_abi_info {
        struct_size: size::<md_abi_info>(),
        ..md_abi_info::default()
    };
    assert_eq!(
        // SAFETY: valid pointer.
        unsafe { md_abi_negotiate(MD_ABI_VERSION, &mut info) },
        MD_OK
    );
    assert_eq!(info.abi_version, 1);
    assert_eq!(info.struct_size, size::<md_abi_info>());
    assert_eq!(info.pointer_size as usize, size_of::<usize>());
    assert_eq!(info.node_size, 72);
    assert_eq!(info.intent_size, 64);
    assert_eq!(info.anchor_version, ANCHOR_VERSION);
    assert_eq!(info.max_anchor_bytes, MD_ANCHOR_MAX_BYTES);
    assert_eq!(info.model_versions & (1 << MD_MODEL_V1), 1 << MD_MODEL_V1);
    assert_eq!(
        info.model_versions & (1 << MD_MODEL_V2) != 0,
        cfg!(feature = "model-v2")
    );

    // Unsupported version: rejected, but the library's version is reported.
    let mut other = md_abi_info {
        struct_size: size::<md_abi_info>(),
        ..md_abi_info::default()
    };
    assert_eq!(
        // SAFETY: valid pointer.
        unsafe { md_abi_negotiate(2, &mut other) },
        MD_ERR_VERSION_MISMATCH
    );
    assert_eq!(other.abi_version, 1);
    assert_eq!(other.node_size, info.node_size);

    for wrong in [0, 4, size::<md_abi_info>() + 4] {
        let mut info = md_abi_info {
            struct_size: wrong,
            abi_version: 77,
            ..md_abi_info::default()
        };
        assert_eq!(
            // SAFETY: valid pointer.
            unsafe { md_abi_negotiate(1, &mut info) },
            MD_ERR_STRUCT_SIZE
        );
        assert_eq!(info.abi_version, 77, "a rejected struct is not written");
    }
    assert_eq!(
        // SAFETY: null is rejected without dereference.
        unsafe { md_abi_negotiate(1, null_mut()) },
        MD_ERR_INVALID_ARGUMENT
    );
    // Misaligned pointer inside a larger buffer.
    let mut buffer = [0u64; 16];
    let misaligned = (buffer.as_mut_ptr() as *mut u8).wrapping_add(1) as *mut md_abi_info;
    assert_eq!(
        // SAFETY: misalignment is detected before any access.
        unsafe { md_abi_negotiate(1, misaligned) },
        MD_ERR_INVALID_ARGUMENT
    );
}

#[test]
fn status_names_are_static_distinct_and_bounded() {
    let _s = serial();
    let mut names = BTreeSet::new();
    for status in 0..=MD_ERR_INTERNAL {
        // SAFETY: md_status_name always returns a static NUL-terminated string.
        let name = unsafe { std::ffi::CStr::from_ptr(md_status_name(status)) };
        let name = name.to_str().unwrap();
        assert!(name.is_ascii() && name.len() < 32 && name != "unknown");
        assert!(names.insert(name.to_owned()), "duplicate {name}");
    }
    // SAFETY: as above.
    let unknown = unsafe { std::ffi::CStr::from_ptr(md_status_name(9999)) };
    assert_eq!(unknown.to_bytes(), b"unknown");
}

#[test]
fn session_creation_validates_arguments() {
    let _s = serial();
    let mut handle = 7;
    let opts = options(MD_MODEL_V1);
    // SAFETY: each call passes deliberately invalid but non-dangling arguments.
    unsafe {
        assert_eq!(
            md_session_create(b"d".as_ptr(), 1, &opts, null_mut()),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            md_session_create(b"d".as_ptr(), 1, null(), &mut handle),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(handle, 0, "outputs are zeroed on failure");
        assert_eq!(
            md_session_create(null(), 3, &opts, &mut handle),
            MD_ERR_INVALID_ARGUMENT
        );
        for bad in [&b""[..], b"has space", b"a/b", &[0xff, 0xfe]] {
            assert_eq!(
                md_session_create(bad.as_ptr(), bad.len(), &opts, &mut handle),
                MD_ERR_INVALID_DOCUMENT_ID
            );
        }
    }
    let mut wrong = opts;
    wrong.struct_size -= 8;
    assert_eq!(create_with(&wrong), Err(MD_ERR_STRUCT_SIZE));
    assert_eq!(create_with(&options(0)), Err(MD_ERR_UNSUPPORTED_MODEL));
    assert_eq!(create_with(&options(3)), Err(MD_ERR_UNSUPPORTED_MODEL));
    #[cfg(not(feature = "model-v2"))]
    assert_eq!(
        create_with(&options(MD_MODEL_V2)),
        Err(MD_ERR_UNSUPPORTED_MODEL)
    );
    for field in 0..5 {
        let mut opts = options(MD_MODEL_V1);
        let slot = [
            &mut opts.max_source_bytes,
            &mut opts.max_nodes,
            &mut opts.max_depth,
            &mut opts.max_events,
            &mut opts.max_text_bytes,
        ];
        *slot.into_iter().nth(field).unwrap() = u64::MAX;
        assert_eq!(create_with(&opts), Err(MD_ERR_INVALID_ARGUMENT));
    }
    let mut limits = options(MD_MODEL_V1);
    limits.max_source_bytes = MAX_SOURCE_BYTES;
    let session = create_with(&limits).unwrap();
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn handles_are_checked_typed_and_never_double_freed() {
    let _s = serial();
    assert_eq!(md_session_release(0), MD_OK);
    assert_eq!(md_job_release(0), MD_OK);
    assert_eq!(md_snapshot_release(0), MD_OK);
    let session = create(MD_MODEL_V1);
    let snapshot = publish(session, "# Title\n");
    assert_eq!(live(), (1, 0, 1));
    // Wrong-type handles are rejected without effect.
    assert_eq!(md_snapshot_release(session), MD_ERR_INVALID_HANDLE);
    assert_eq!(md_session_release(snapshot), MD_ERR_INVALID_HANDLE);
    assert_eq!(md_job_cancel(snapshot), MD_ERR_INVALID_HANDLE);
    assert_eq!(live(), (1, 0, 1));
    assert_eq!(md_snapshot_release(snapshot), MD_OK);
    assert_eq!(md_snapshot_release(snapshot), MD_ERR_INVALID_HANDLE);
    assert_eq!(md_session_release(session), MD_OK);
    assert_eq!(md_session_release(session), MD_ERR_INVALID_HANDLE);
    let mut job = 9;
    // SAFETY: valid pointers.
    let stale =
        unsafe { md_session_submit(session, b"x".as_ptr(), 1, null(), 0, &mut job, null_mut()) };
    assert_eq!((stale, job), (MD_ERR_INVALID_HANDLE, 0));
    let mut view = md_snapshot_view {
        struct_size: size::<md_snapshot_view>(),
        ..md_snapshot_view::default()
    };
    assert_eq!(
        // SAFETY: valid pointer.
        unsafe { md_snapshot_get_view(snapshot, &mut view) },
        MD_ERR_INVALID_HANDLE
    );
    assert!(view.nodes.is_null() && view.struct_size == size::<md_snapshot_view>());
    assert_eq!(md_snapshot_release(u64::MAX), MD_ERR_INVALID_HANDLE);
}

// ---------------------------------------------------------------------------
// Bulk view

#[test]
fn v1_view_round_trips_every_model_value() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let (mut kinds, mut tables) = (BTreeSet::new(), [0u32; 7]);
    for source in corpus() {
        let snapshot = publish(session, &source);
        let v = view(snapshot);
        assert_eq!(v.model_version, MD_MODEL_V1);
        kinds.extend(Tables::slice(v.nodes, v.node_count).iter().map(|n| n.kind));
        for (total, count) in tables.iter_mut().zip([
            v.link_count,
            v.resource_count,
            v.outline_count,
            v.diagnostic_count,
            v.property_count,
            v.string_list_count,
            v.alignment_count,
        ]) {
            *total += count;
        }
        assert_eq!(Tables::bytes(v.source, v.source_length), source.as_bytes());
        let expected = model(snapshot);
        assert_eq!(v.generation, expected.generation().get());
        assert_eq!(&Tables { v: &v }.v1(), expected.v1().unwrap().input());
        // A second view of the same handle reuses the same encoding.
        let again = view(snapshot);
        assert_eq!((again.nodes, again.strings), (v.nodes, v.strings));
        assert_eq!(md_snapshot_release(snapshot), MD_OK);
    }
    // Every kind the parser emits today; footnotes and alerts are CORE-003B3
    // (the encoder handles them, and parser output cannot yet contain them).
    assert_eq!(kinds.len(), 21, "{kinds:?}");
    assert!(tables.iter().all(|n| *n > 0), "{tables:?}");
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn v1_golden_fixtures_round_trip_all_node_kinds() {
    let _s = serial();
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../specs/semantic-model/v1/fixtures");
    let mut kinds = BTreeSet::new();
    for name in ["containers", "fallback", "gfm", "resources", "unicode"] {
        let source = std::fs::read_to_string(root.join(format!("{name}.source.txt"))).unwrap();
        let json = std::fs::read(root.join(format!("{name}.json"))).unwrap();
        let document = Document::from_json(&json, &source, ModelLimits::default()).unwrap();
        let flat = flat::Flat::encode_model(&SnapshotModel::V1(document.clone())).unwrap();
        let v = flat.view(&source, 7);
        assert_eq!(v.generation, 7);
        kinds.extend(Tables::slice(v.nodes, v.node_count).iter().map(|n| n.kind));
        assert_eq!(&Tables { v: &v }.v1(), document.input(), "{name}");
    }
    assert_eq!(kinds, (1..=24).collect(), "goldens cover every node kind");
}

#[cfg(feature = "model-v2")]
#[test]
fn v2_view_round_trips_provenance_and_targets() {
    let _s = serial();
    let session = create(MD_MODEL_V2);
    let mut kinds = BTreeSet::new();
    for source in corpus() {
        let snapshot = publish(session, &source);
        let v = view(snapshot);
        assert_eq!(v.model_version, MD_MODEL_V2);
        let tables = Tables { v: &v };
        let expected = model(snapshot);
        assert_eq!(&tables.v2(), expected.v2().unwrap().input());
        for intent in Tables::slice(v.links, v.link_count) {
            kinds.insert((intent.source_kind, intent.target_kind));
        }
        assert_eq!(md_snapshot_release(snapshot), MD_OK);
    }
    assert!(
        kinds.contains(&(MD_SOURCE_REFERENCE, MD_TARGET_EXTERNAL)),
        "{kinds:?}"
    );
    assert!(kinds.iter().any(|(_, t)| *t == MD_TARGET_DENIED));
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn handles_to_one_snapshot_share_one_encoding() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let first = publish(session, "# Shared\n\nbody\n");
    let mut current = 0;
    // SAFETY: valid pointer.
    assert_eq!(unsafe { md_session_current(session, &mut current) }, MD_OK);
    assert_ne!(current, first);
    let (a, b) = (view(first), view(current));
    assert_eq!((a.nodes, a.strings), (b.nodes, b.strings), "one encoding");
    // Releasing one handle leaves the other's view intact.
    assert_eq!(md_snapshot_release(first), MD_OK);
    assert_eq!(Tables { v: &b }.v1(), *model(current).v1().unwrap().input());
    // A different snapshot gets its own encoding (compared while both live).
    let other = publish(session, "# Other\n");
    assert_ne!(view(other).strings, b.strings);
    assert_eq!(registry().encodings.len(), 2);
    assert_eq!(md_snapshot_release(current), MD_OK);
    assert_eq!(registry().encodings.len(), 1, "last sharer pruned");
    assert_eq!(md_snapshot_release(other), MD_OK);
    assert!(registry().encodings.is_empty());
    // A new handle to an already released snapshot re-registers and shares.
    let (mut again, mut twice) = (0, 0);
    // SAFETY: valid pointers.
    unsafe {
        assert_eq!(md_session_current(session, &mut again), MD_OK);
        assert_eq!(md_session_current(session, &mut twice), MD_OK);
    }
    assert_eq!(registry().encodings.len(), 1);
    assert_eq!(view(again).strings, view(twice).strings);
    assert_eq!(md_snapshot_release(again), MD_OK);
    assert_eq!(registry().encodings.len(), 1, "one sharer remains");
    assert_eq!(md_snapshot_release(twice), MD_OK);
    assert!(registry().encodings.is_empty());
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn overlapping_outputs_are_memory_safe() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    // The contract forbids overlapping outputs; the result is unspecified,
    // but the core only performs raw writes, so this is not UB.
    let mut shared = 5u64;
    let p: *mut u64 = &mut shared;
    // SAFETY: one valid location passed three times.
    assert_eq!(unsafe { md_live_handles(p, p, p) }, MD_OK);
    assert_eq!(shared, 0, "snapshot count, written last");
    let mut both = 0u64;
    let job_ptr: *mut u64 = &mut both;
    // SAFETY: the same valid location for job and generation.
    let status =
        unsafe { md_session_submit(session, b"x".as_ptr(), 1, null(), 0, job_ptr, job_ptr) };
    assert_eq!(status, MD_OK);
    assert_eq!(both, 1, "generation, written after the job handle");
    assert_eq!(md_session_release(session), MD_OK);
    let (_, jobs, _) = live();
    assert_eq!(jobs, 1);
    // Release the job through the registry, whose handle was overwritten.
    let handle = *registry().jobs.keys().next().unwrap();
    assert_eq!(md_job_release(handle), MD_OK);
}

#[test]
fn empty_document_view_has_null_arrays() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let snapshot = publish(session, "");
    let v = view(snapshot);
    assert_eq!((v.node_count, v.root_count, v.source_length), (0, 0, 0));
    assert!(v.nodes.is_null() && v.roots.is_null() && v.source.is_null());
    assert!(
        !v.strings.is_null(),
        "IDs and the digest are always present"
    );
    assert_eq!(md_snapshot_release(snapshot), MD_OK);
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn snapshot_view_outlives_session_and_newer_generations() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let first = publish(session, "# One\n\nalpha\n");
    let v = view(first);
    let second = publish(session, "# Two\n");
    assert_eq!(md_session_release(session), MD_OK);
    // The first view is still intact after a newer publish and session release.
    assert_eq!(
        Tables::bytes(v.source, v.source_length),
        b"# One\n\nalpha\n"
    );
    assert_eq!(Tables { v: &v }.v1(), *model(first).v1().unwrap().input());
    assert_eq!(view(second).generation, 2);
    assert_eq!(md_snapshot_release(first), MD_OK);
    assert_eq!(md_snapshot_release(second), MD_OK);
}

// ---------------------------------------------------------------------------
// Sessions and jobs

#[test]
fn generations_outcomes_and_status() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let st = status(session);
    assert_eq!((st.latest, st.published, st.flags), (0, 0, 0));
    let mut none = 5;
    // SAFETY: valid pointer.
    assert_eq!(unsafe { md_session_current(session, &mut none) }, MD_OK);
    assert_eq!(none, 0);

    let (job, generation) = submit_token(session, b"# A\n", b"rev-1");
    assert_eq!(generation, 1);
    assert_eq!(status(session).pending, 1);
    let out = run(job);
    assert_eq!((out.outcome, out.generation), (MD_OUTCOME_PUBLISHED, 1));
    let st = status(session);
    assert_eq!(
        (st.latest, st.confirmed, st.published, st.pending),
        (1, 1, 1, 0)
    );
    assert_eq!(st.flags, MD_STATUS_IS_CURRENT);
    assert_eq!(md_snapshot_release(out.snapshot), MD_OK);

    // Identical bytes: Unchanged, the original snapshot confirmed at gen 2.
    let out = run(submit(session, "# A\n"));
    assert_eq!((out.outcome, out.generation), (MD_OUTCOME_UNCHANGED, 2));
    assert_eq!(view(out.snapshot).generation, 1);
    assert_eq!(md_snapshot_release(out.snapshot), MD_OK);

    // Superseded: an older job cannot publish.
    let old = submit(session, "old");
    let new = submit(session, "new");
    let out = run(old);
    assert_eq!(
        (out.outcome, out.generation, out.latest),
        (MD_OUTCOME_SUPERSEDED, 3, 4)
    );
    assert_eq!(out.snapshot, 0);
    let out = run(new);
    assert_eq!(out.outcome, MD_OUTCOME_PUBLISHED);
    assert_eq!(md_snapshot_release(out.snapshot), MD_OK);

    // Failure keeps the previous snapshot published but not current.
    let out = run(submit_token(session, &[0xff, 0xfe], b"").0);
    assert_eq!(
        (out.outcome, out.failure),
        (MD_OUTCOME_FAILED, MD_FAILURE_INVALID_UTF8)
    );
    let st = status(session);
    assert_eq!(
        (st.failure, st.failure_generation, st.published),
        (MD_FAILURE_INVALID_UTF8, 5, 4)
    );
    assert_eq!(st.flags, 0);
    let mut current = 0;
    // SAFETY: valid pointer.
    assert_eq!(unsafe { md_session_current(session, &mut current) }, MD_OK);
    assert_eq!(view(current).generation, 4);
    assert_eq!(md_snapshot_release(current), MD_OK);

    // Running twice is an invalid state; the handle still needs one release.
    let job = submit(session, "twice");
    let mut out = result();
    // SAFETY: valid pointer.
    unsafe {
        assert_eq!(md_job_run(job, &mut out), MD_OK);
        assert_eq!(md_snapshot_release(out.snapshot), MD_OK);
        assert_eq!(md_job_run(job, &mut out), MD_ERR_INVALID_STATE);
    }
    assert_eq!(out.snapshot, 0);
    assert_eq!(md_job_release(job), MD_OK);
    assert_eq!(md_job_release(job), MD_ERR_INVALID_HANDLE);
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn limits_map_to_typed_failures() {
    let _s = serial();
    // An oversized submission still consumes a generation and fails as
    // SourceLimit; only a bounded prefix is copied.
    let mut opts = options(MD_MODEL_V1);
    opts.max_source_bytes = 16;
    let session = create_with(&opts).unwrap();
    let big = vec![b'x'; 1 << 20];
    let (job, generation) = submit_token(session, &big, b"");
    assert_eq!(generation, 1);
    let out = run(job);
    assert_eq!(
        (out.outcome, out.failure),
        (MD_OUTCOME_FAILED, MD_FAILURE_SOURCE_LIMIT)
    );
    let at_limit = run(submit(session, "exactly 16 bytes"));
    assert_eq!(at_limit.outcome, MD_OUTCOME_PUBLISHED);
    assert_eq!(md_snapshot_release(at_limit.snapshot), MD_OK);
    assert_eq!(status(session).latest, 2);
    assert_eq!(md_session_release(session), MD_OK);
    let mut opts = options(MD_MODEL_V1);
    opts.max_source_bytes = 4;
    let session = create_with(&opts).unwrap();
    let out = run(submit(session, "too long"));
    assert_eq!(
        (out.outcome, out.failure),
        (MD_OUTCOME_FAILED, MD_FAILURE_SOURCE_LIMIT)
    );
    assert_eq!(md_session_release(session), MD_OK);
    let mut opts = options(MD_MODEL_V1);
    opts.max_nodes = 2;
    let session = create_with(&opts).unwrap();
    let out = run(submit(session, "a *b* c\n\nd\n"));
    assert_eq!(
        (out.outcome, out.failure),
        (MD_OUTCOME_FAILED, MD_FAILURE_NODE_LIMIT)
    );
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn cancellation_release_and_close() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    // Job cancel before run.
    let job = submit(session, "# x");
    assert_eq!(md_job_cancel(job), MD_OK);
    assert_eq!(run(job).outcome, MD_OUTCOME_CANCELLED);
    // Session cancel reports whether a live job existed.
    let job = submit(session, "# y");
    let mut cancelled = 0;
    // SAFETY: valid pointers.
    unsafe {
        assert_eq!(md_session_cancel(session, &mut cancelled), MD_OK);
        assert_eq!(cancelled, 1);
        assert_eq!(md_session_cancel(session, &mut cancelled), MD_OK);
        assert_eq!(cancelled, 0);
        assert_eq!(md_session_cancel(session, null_mut()), MD_OK);
    }
    assert_eq!(run(job).outcome, MD_OUTCOME_CANCELLED);
    // Releasing an unrun job clears its pending state.
    let job = submit(session, "# z");
    assert_eq!(status(session).pending, 3);
    assert_eq!(md_job_release(job), MD_OK);
    assert_eq!(status(session).pending, 0);
    // Releasing the session closes outstanding jobs.
    let job = submit(session, "# w");
    assert_eq!(md_session_release(session), MD_OK);
    let out = run(job);
    assert_eq!((out.outcome, out.generation), (MD_OUTCOME_CLOSED, 4));
}

#[test]
fn cancel_from_another_thread_during_a_large_run() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let source: String = (0..40_000)
        .map(|i| format!("Paragraph {i} with *emphasis* and `code`.\n\n"))
        .collect();
    let mut cancelled = 0;
    for _ in 0..5 {
        let job = submit(session, &source);
        let worker = std::thread::spawn(move || {
            let mut out = result();
            // SAFETY: valid pointer.
            assert_eq!(unsafe { md_job_run(job, &mut out) }, MD_OK);
            out
        });
        // Cancellation is valid before, during or after the run.
        assert_eq!(md_job_cancel(job), MD_OK);
        let out = worker.join().unwrap();
        assert_eq!(md_job_release(job), MD_OK);
        match out.outcome {
            MD_OUTCOME_CANCELLED => cancelled += 1,
            // The worker may already have finished before cancellation.
            MD_OUTCOME_PUBLISHED => assert_eq!(md_snapshot_release(out.snapshot), MD_OK),
            other => panic!("{other}"),
        }
    }
    assert!(cancelled > 0, "no cancellation observed");
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn concurrent_sessions_return_handles_to_zero() {
    let _s = serial();
    let threads: Vec<_> = (0..8)
        .map(|t| {
            std::thread::spawn(move || {
                let session = create(MD_MODEL_V1);
                for i in 0..50 {
                    let snapshot = publish(session, &format!("# T{t}\n\nrun {i}\n"));
                    let v = view(snapshot);
                    assert!(v.node_count > 0);
                    assert_eq!(md_snapshot_release(snapshot), MD_OK);
                }
                assert_eq!(md_session_release(session), MD_OK);
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    // Racing submissions on one shared session: latest wins, no leaks.
    let session = create(MD_MODEL_V1);
    let threads: Vec<_> = (0..8)
        .map(|t| {
            std::thread::spawn(move || {
                for i in 0..25 {
                    let out = run(submit(session, &format!("{t} {i}")));
                    if out.snapshot != 0 {
                        assert_eq!(md_snapshot_release(out.snapshot), MD_OK);
                    }
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(status(session).latest, 200);
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn external_changes_map_dispositions() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let change = |kind: u32, reason: u32, token: &[u8]| -> Result<(u32, u64), md_status> {
        let (mut disposition, mut generation) = (0, 0);
        // SAFETY: valid pointers.
        let status = unsafe {
            md_session_external_change(
                session,
                kind,
                reason,
                token.as_ptr(),
                token.len(),
                &mut disposition,
                &mut generation,
            )
        };
        if status == MD_OK {
            Ok((disposition, generation))
        } else {
            Err(status)
        }
    };
    let out = run(submit_token(session, b"# a", b"tok").0);
    assert_eq!(md_snapshot_release(out.snapshot), MD_OK);
    assert_eq!(
        change(MD_CHANGE_MODIFIED, 0, b"tok"),
        Ok((MD_DISPOSITION_UNCHANGED, 0))
    );
    assert_eq!(
        change(MD_CHANGE_MODIFIED, 0, b"new"),
        Ok((MD_DISPOSITION_READ_REQUIRED, 0))
    );
    assert_eq!(
        change(MD_CHANGE_MODIFIED, 0, b""),
        Ok((MD_DISPOSITION_READ_REQUIRED, 0))
    );
    let (job, generation) = submit_token(session, b"# b", b"tok2");
    assert_eq!(
        change(MD_CHANGE_MODIFIED, 0, b"tok2"),
        Ok((MD_DISPOSITION_PENDING, generation))
    );
    assert_eq!(md_job_release(job), MD_OK);
    assert_eq!(
        change(MD_CHANGE_DELETED, 0, b""),
        Ok((MD_DISPOSITION_RECORDED, 3))
    );
    assert_eq!(
        change(MD_CHANGE_DELETED, 0, b""),
        Ok((MD_DISPOSITION_RECORDED, 3))
    );
    assert_eq!(status(session).availability, MD_DELETED);
    assert_eq!(
        change(MD_CHANGE_UNREADABLE, MD_REASON_ACCESS_REVOKED, b""),
        Ok((MD_DISPOSITION_RECORDED, 4))
    );
    let st = status(session);
    assert_eq!(
        (st.availability, st.unreadable_reason),
        (MD_UNREADABLE, MD_REASON_ACCESS_REVOKED)
    );
    for (kind, reason, token) in [
        (MD_CHANGE_UNREADABLE, 0, &b""[..]),
        (MD_CHANGE_UNREADABLE, 9, b""),
        (MD_CHANGE_MODIFIED, 1, b"t"),
        (MD_CHANGE_DELETED, 0, b"t"),
        (0, 0, b""),
        (9, 0, b""),
    ] {
        assert_eq!(change(kind, reason, token), Err(MD_ERR_INVALID_ARGUMENT));
    }
    assert_eq!(
        change(MD_CHANGE_MODIFIED, 0, &[1; 257]),
        Err(MD_ERR_INVALID_REVISION_TOKEN)
    );
    let mut job = 0;
    // SAFETY: valid pointers.
    let status = unsafe {
        md_session_submit(
            session,
            b"x".as_ptr(),
            1,
            [0u8; 257].as_ptr(),
            257,
            &mut job,
            null_mut(),
        )
    };
    assert_eq!((status, job), (MD_ERR_INVALID_REVISION_TOKEN, 0));
    assert_eq!(md_session_release(session), MD_OK);
}

// ---------------------------------------------------------------------------
// Anchors

fn anchor_at(snapshot: md_snapshot, offset: u64) -> Result<Vec<u8>, md_status> {
    let mut length = 0;
    // SAFETY: a zero-capacity null buffer only queries the length.
    let status = unsafe { md_snapshot_anchor_at(snapshot, offset, null_mut(), 0, &mut length) };
    if status != MD_ERR_BUFFER_TOO_SMALL {
        return Err(status);
    }
    assert!(length > 0 && length <= MD_ANCHOR_MAX_BYTES as usize);
    let mut buffer = vec![0u8; length];
    let mut written = 0;
    // SAFETY: buffer is writable for `length` bytes.
    let status = unsafe {
        md_snapshot_anchor_at(snapshot, offset, buffer.as_mut_ptr(), length, &mut written)
    };
    assert_eq!((status, written), (MD_OK, length));
    Ok(buffer)
}
fn resolve(snapshot: md_snapshot, anchor: &[u8]) -> Result<md_anchor_resolution, md_status> {
    let mut out = md_anchor_resolution {
        struct_size: size::<md_anchor_resolution>(),
        ..md_anchor_resolution::default()
    };
    // SAFETY: valid pointers.
    match unsafe { md_snapshot_resolve_anchor(snapshot, anchor.as_ptr(), anchor.len(), &mut out) } {
        MD_OK => Ok(out),
        status => Err(status),
    }
}

#[test]
fn anchors_round_trip_across_generations() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let old = publish(session, "# Title\n\nAlpha para.\n\nBeta para.\n");
    let offset = "# Title\n\nAlpha para.\n\nBeta".len() as u64;
    let anchor = anchor_at(old, offset).unwrap();
    let same = resolve(old, &anchor).unwrap();
    assert_eq!(
        (same.quality, same.source_offset, same.offset),
        (MD_ANCHOR_EXACT, offset, 4)
    );
    let new = publish(
        session,
        "# Title\n\nInserted.\n\nAlpha para.\n\nBeta para.\n",
    );
    let moved = resolve(new, &anchor).unwrap();
    assert_eq!(moved.quality, MD_ANCHOR_EXACT);
    assert_eq!(moved.source_offset, offset + "Inserted.\n\n".len() as u64);
    let v = view(new);
    let node = Tables::slice(v.nodes, v.node_count)[moved.node as usize];
    assert_eq!(node.kind, 1);
    assert_eq!(
        u64::from(node.span_start) + moved.offset,
        moved.source_offset
    );

    // anchor_for_node with a view index; the buffer can be exactly sized.
    let mut buffer = [0u8; MD_ANCHOR_MAX_BYTES as usize];
    let mut length = 0;
    // SAFETY: buffer valid for its capacity.
    let status = unsafe {
        md_snapshot_anchor_for_node(
            new,
            moved.node,
            2,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut length,
        )
    };
    assert_eq!(status, MD_OK);
    let again = resolve(new, &buffer[..length]).unwrap();
    assert_eq!((again.node, again.offset), (moved.node, 2));
    // SAFETY: buffer valid for its capacity.
    let unknown = unsafe {
        md_snapshot_anchor_for_node(
            new,
            v.node_count,
            0,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut length,
        )
    };
    assert_eq!((unknown, length), (MD_ERR_UNKNOWN_NODE, 0));
    // Too small a buffer reports the required length and writes nothing.
    let mut small = [0u8; 4];
    // SAFETY: small valid for its capacity.
    let status =
        unsafe { md_snapshot_anchor_at(new, 0, small.as_mut_ptr(), small.len(), &mut length) };
    assert_eq!(status, MD_ERR_BUFFER_TOO_SMALL);
    assert!(length > small.len());
    assert_eq!(small, [0; 4]);

    // Untrusted bytes.
    let mut forged: serde_json::Value = serde_json::from_slice(&anchor).unwrap();
    forged["version"] = 2.into();
    assert_eq!(
        resolve(new, forged.to_string().as_bytes()).err(),
        Some(MD_ERR_ANCHOR_VERSION)
    );
    forged["version"] = 1.into();
    forged["extra"] = 1.into();
    assert_eq!(
        resolve(new, forged.to_string().as_bytes()).err(),
        Some(MD_ERR_ANCHOR_INVALID)
    );
    assert_eq!(resolve(new, b"{").err(), Some(MD_ERR_ANCHOR_INVALID));
    assert_eq!(resolve(new, b"").err(), Some(MD_ERR_ANCHOR_INVALID));
    assert_eq!(
        resolve(new, &vec![b' '; MD_ANCHOR_MAX_BYTES as usize + 1]).err(),
        Some(MD_ERR_ANCHOR_INVALID)
    );
    let mut out = md_anchor_resolution {
        struct_size: size::<md_anchor_resolution>(),
        ..md_anchor_resolution::default()
    };
    assert_eq!(
        // SAFETY: null with nonzero length is rejected before reading.
        unsafe { md_snapshot_resolve_anchor(new, null(), 3, &mut out) },
        MD_ERR_INVALID_ARGUMENT
    );

    // Empty documents: no anchor can be taken and resolution is lost.
    let empty = publish(session, "");
    assert_eq!(anchor_at(empty, 0).err(), Some(MD_ERR_NO_BLOCKS));
    let lost = resolve(empty, &anchor).unwrap();
    assert_eq!((lost.quality, lost.node), (MD_ANCHOR_LOST, MD_NONE));
    for handle in [old, new, empty] {
        assert_eq!(md_snapshot_release(handle), MD_OK);
    }
    assert_eq!(md_session_release(session), MD_OK);
}

// ---------------------------------------------------------------------------
// Panic containment

#[test]
fn panics_are_contained_at_every_entry_point() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let snapshot = publish(session, "# P\n");
    let anchor = anchor_at(snapshot, 0).unwrap();
    let mut buffer = [0u8; 64];
    let mut length = 0;
    let mut u = 0u32;
    let mut handle = 0u64;
    type Call<'a> = (&'static str, Box<dyn Fn() -> md_status + 'a>);
    let calls: Vec<Call> = vec![
        (
            "negotiate",
            Box::new(|| {
                let mut info = md_abi_info {
                    struct_size: size::<md_abi_info>(),
                    ..Default::default()
                };
                // SAFETY: valid pointer.
                unsafe { md_abi_negotiate(1, &mut info) }
            }),
        ),
        (
            "live",
            Box::new(|| {
                let mut n = 0;
                // SAFETY: valid pointer. The panic happens while the registry
                // lock is held, poisoning it; later calls must recover.
                unsafe { md_live_handles(&mut n, null_mut(), null_mut()) }
            }),
        ),
        (
            "create",
            Box::new(|| {
                let mut s = 0;
                // SAFETY: valid pointers.
                unsafe { md_session_create(b"d".as_ptr(), 1, &options(1), &mut s) }
            }),
        ),
        (
            "submit",
            Box::new(move || {
                let mut job = 0;
                // SAFETY: valid pointers.
                unsafe {
                    md_session_submit(session, b"x".as_ptr(), 1, null(), 0, &mut job, null_mut())
                }
            }),
        ),
        (
            "change",
            Box::new(move || {
                let mut d = 0;
                // SAFETY: valid pointers.
                unsafe {
                    md_session_external_change(
                        session,
                        MD_CHANGE_DELETED,
                        0,
                        null(),
                        0,
                        &mut d,
                        null_mut(),
                    )
                }
            }),
        ),
        (
            "cancel",
            Box::new(move || {
                // SAFETY: null optional output.
                unsafe { md_session_cancel(session, null_mut()) }
            }),
        ),
        (
            "status",
            Box::new(move || {
                let mut st = md_session_status_info {
                    struct_size: size::<md_session_status_info>(),
                    ..Default::default()
                };
                // SAFETY: valid pointer.
                unsafe { md_session_get_status(session, &mut st) }
            }),
        ),
        (
            "current",
            Box::new(move || {
                let mut s = 0;
                // SAFETY: valid pointer.
                unsafe { md_session_current(session, &mut s) }
            }),
        ),
        (
            "view",
            Box::new(move || {
                let mut v = md_snapshot_view {
                    struct_size: size::<md_snapshot_view>(),
                    ..Default::default()
                };
                // SAFETY: valid pointer.
                unsafe { md_snapshot_get_view(snapshot, &mut v) }
            }),
        ),
        (
            "resolve",
            Box::new(move || resolve(snapshot, &anchor).err().unwrap_or(MD_OK)),
        ),
        ("release", Box::new(|| md_snapshot_release(u64::MAX - 1))),
    ];
    for (name, call) in &calls {
        inject_panic();
        assert_eq!(call(), MD_ERR_PANIC, "{name}");
    }
    // SAFETY: valid pointers.
    unsafe {
        inject_panic();
        assert_eq!(
            md_snapshot_anchor_at(snapshot, 0, buffer.as_mut_ptr(), 64, &mut length),
            MD_ERR_PANIC
        );
        assert_eq!(length, 0);
        length = 7;
        inject_panic();
        assert_eq!(
            md_snapshot_anchor_for_node(snapshot, 0, 0, buffer.as_mut_ptr(), 64, &mut length),
            MD_ERR_PANIC
        );
        assert_eq!(length, 0);
        inject_panic();
        assert_eq!(md_session_cancel(session, &mut u), MD_ERR_PANIC);
        assert_eq!(u, 0);
        inject_panic();
        assert_eq!(md_session_current(session, &mut handle), MD_ERR_PANIC);
        assert_eq!(handle, 0);
    }
    // A panic after the job ran (its snapshot already published) leaves the
    // result zeroed: no generation, outcome or leaked snapshot handle.
    let job = submit(session, "# Q\n");
    let mut out = result();
    out.generation = 99;
    out.snapshot = 99;
    inject_panic();
    // SAFETY: valid pointer.
    assert_eq!(unsafe { md_job_run(job, &mut out) }, MD_ERR_PANIC);
    assert_eq!(
        (
            out.struct_size,
            out.outcome,
            out.generation,
            out.latest,
            out.snapshot,
            out.failure
        ),
        (size::<md_job_result>(), 0, 0, 0, 0, 0)
    );
    assert_eq!(live().2, 1, "only the test's own snapshot handle");
    inject_panic();
    assert_eq!(md_job_cancel(job), MD_ERR_PANIC);
    inject_panic();
    assert_eq!(md_job_release(job), MD_ERR_PANIC);
    assert_eq!(md_job_release(job), MD_OK);
    // The library still works after poisoning and panics.
    assert_eq!(view(snapshot).node_count, 2);
    assert_eq!(md_snapshot_release(snapshot), MD_OK);
    inject_panic();
    assert_eq!(md_session_release(session), MD_ERR_PANIC);
    assert_eq!(md_session_release(session), MD_OK);
}

// ---------------------------------------------------------------------------
// Find and reading statistics (D-022 additions)

use crate::search::{md_snapshot_find, md_snapshot_read_stats};
use markdown_core::search::{self as core_search, SearchDocument};

fn find_opts(flags: u32, max_matches: u32) -> md_find_options {
    md_find_options {
        struct_size: size::<md_find_options>(),
        flags,
        max_matches,
        match_size: size::<md_find_match>(),
        segment_size: size::<md_match_segment>(),
        reserved: 0,
    }
}
fn find_result() -> md_find_result {
    md_find_result {
        struct_size: size::<md_find_result>(),
        ..md_find_result::default()
    }
}
/// A host passes null for an empty array.
fn array<T>(values: &mut [T]) -> *mut T {
    if values.is_empty() {
        null_mut()
    } else {
        values.as_mut_ptr()
    }
}
fn find_into(
    snapshot: md_snapshot,
    query: &[u8],
    options: &md_find_options,
    matches: &mut [md_find_match],
    segments: &mut [md_match_segment],
    out: &mut md_find_result,
) -> md_status {
    // SAFETY: slices are valid for their lengths; empty ones pass null.
    unsafe {
        md_snapshot_find(
            snapshot,
            query.as_ptr(),
            query.len(),
            options,
            array(matches),
            matches.len(),
            array(segments),
            segments.len(),
            out,
        )
    }
}
/// The two-call protocol a host uses: size with empty arrays, then fill.
fn find_all(
    snapshot: md_snapshot,
    query: &[u8],
    options: &md_find_options,
) -> Result<(md_find_result, Vec<md_find_match>, Vec<md_match_segment>), md_status> {
    let mut out = find_result();
    match find_into(snapshot, query, options, &mut [], &mut [], &mut out) {
        MD_OK => {
            assert_eq!((out.match_count, out.segment_count), (0, 0));
            return Ok((out, Vec::new(), Vec::new()));
        }
        MD_ERR_BUFFER_TOO_SMALL => {}
        status => return Err(status),
    }
    let mut matches = vec![md_find_match::default(); out.match_count as usize];
    let mut segments = vec![md_match_segment::default(); out.segment_count as usize];
    let sized = out;
    let mut out = find_result();
    let status = find_into(
        snapshot,
        query,
        options,
        &mut matches,
        &mut segments,
        &mut out,
    );
    assert_eq!(status, MD_OK);
    assert_eq!(
        (out.flags, out.match_count, out.segment_count),
        (sized.flags, sized.match_count, sized.segment_count),
        "a recomputed search is deterministic"
    );
    Ok((out, matches, segments))
}

fn read_opts(flags: u32, words_per_minute: u32) -> md_read_options {
    md_read_options {
        struct_size: size::<md_read_options>(),
        flags,
        words_per_minute,
        section_size: size::<md_read_section>(),
    }
}
fn read_result() -> md_read_stats {
    md_read_stats {
        struct_size: size::<md_read_stats>(),
        ..md_read_stats::default()
    }
}
fn read_into(
    snapshot: md_snapshot,
    options: &md_read_options,
    sections: &mut [md_read_section],
    out: &mut md_read_stats,
) -> md_status {
    // SAFETY: the slice is valid for its length; empty passes null.
    unsafe { md_snapshot_read_stats(snapshot, options, array(sections), sections.len(), out) }
}
fn read_all(
    snapshot: md_snapshot,
    options: &md_read_options,
) -> Result<(md_read_stats, Vec<md_read_section>), md_status> {
    let mut out = read_result();
    match read_into(snapshot, options, &mut [], &mut out) {
        MD_ERR_BUFFER_TOO_SMALL => {}
        status => return Err(status),
    }
    let mut sections = vec![md_read_section::default(); out.section_count as usize];
    let mut out = read_result();
    assert_eq!(read_into(snapshot, options, &mut sections, &mut out), MD_OK);
    Ok((out, sections))
}

/// Check FFI find records against the Rust API, and check every exact
/// segment against the bulk view as a native presenter would read it.
fn assert_find_parity(snapshot: md_snapshot, query: &str, flags: u32, max_matches: u32) {
    let (out, matches, segments) =
        find_all(snapshot, query.as_bytes(), &find_opts(flags, max_matches)).unwrap();
    let snap = model(snapshot);
    let options = core_search::FindOptions {
        case_sensitive: flags & MD_FIND_CASE_SENSITIVE != 0,
        whole_word: flags & MD_FIND_WHOLE_WORD != 0,
        max_matches: if max_matches == 0 {
            core_search::DEFAULT_MAX_MATCHES
        } else {
            max_matches as usize
        },
    };
    let expected =
        core_search::find(&SearchDocument::from_snapshot(&snap), query, &options).unwrap();
    assert_eq!(out.flags & MD_FIND_TRUNCATED != 0, expected.truncated);
    assert_eq!(matches.len(), expected.matches.len(), "{query:?}");

    let v = view(snapshot);
    let t = Tables { v: &v };
    let source = Tables::bytes(v.source, v.source_length);
    let nodes = t.nodes();
    let mut next = 0usize;
    for (m, e) in matches.iter().zip(&expected.matches) {
        assert_eq!(
            (
                m.unit as usize,
                u64::from(m.span_start),
                u64::from(m.span_end),
                m.flags
            ),
            (
                e.unit,
                e.source.start,
                e.source.end,
                if e.exact { MD_MATCH_EXACT } else { 0 }
            )
        );
        assert_eq!(m.first_segment as usize, next, "segments are contiguous");
        next += m.segment_count as usize;
        let own = &segments[m.first_segment as usize..next];
        assert_eq!(own.len(), e.segments.len());
        for (s, e) in own.iter().zip(&e.segments) {
            assert_eq!(
                (
                    s.node as usize,
                    u64::from(s.text_start),
                    u64::from(s.text_end),
                    u64::from(s.span_start),
                    u64::from(s.span_end),
                    s.flags
                ),
                (
                    e.node,
                    e.text_start,
                    e.text_end,
                    e.source.start,
                    e.source.end,
                    if e.exact { MD_MATCH_EXACT } else { 0 }
                )
            );
            let node = &nodes[s.node as usize];
            let display = match node.kind {
                MD_NODE_SOFT_BREAK | MD_NODE_HARD_BREAK => {
                    assert_eq!((s.text_start, s.text_end), (0, 0));
                    continue;
                }
                MD_NODE_UNSUPPORTED_BLOCK | MD_NODE_UNSUPPORTED_INLINE => {
                    source[node.span_start as usize..node.span_end as usize].to_vec()
                }
                _ => t.str(node.text).into_bytes(),
            };
            let text = &display[s.text_start as usize..s.text_end as usize];
            assert!(std::str::from_utf8(text).is_ok());
            assert!(node.span_start <= s.span_start && s.span_end <= node.span_end);
            if s.flags & MD_MATCH_EXACT != 0 {
                assert_eq!(&source[s.span_start as usize..s.span_end as usize], text);
            }
        }
    }
    assert_eq!(next, segments.len());
}

fn assert_read_parity(snapshot: md_snapshot, flags: u32, words_per_minute: u32) {
    let (out, sections) = read_all(snapshot, &read_opts(flags, words_per_minute)).unwrap();
    let snap = model(snapshot);
    let options = core_search::ReadOptions {
        words_per_minute: if words_per_minute == 0 {
            core_search::DEFAULT_WORDS_PER_MINUTE
        } else {
            words_per_minute
        },
        include_headings: flags & MD_READ_INCLUDE_HEADINGS != 0,
        include_code: flags & MD_READ_INCLUDE_CODE != 0,
    };
    let expected =
        core_search::read_stats(&SearchDocument::from_snapshot(&snap), &options).unwrap();
    assert_eq!(
        (
            out.words,
            out.seconds,
            out.words_per_minute,
            out.section_count
        ),
        (
            expected.words,
            expected.seconds,
            expected.words_per_minute,
            expected.sections.len() as u64
        )
    );
    let v = view(snapshot);
    assert_eq!(sections.len(), v.outline_count as usize + 1);
    let outline = Tables::slice(v.outline, v.outline_count);
    let none = |value: Option<usize>| value.map_or(MD_NONE, |i| i as u32);
    for (i, (s, e)) in sections.iter().zip(&expected.sections).enumerate() {
        assert_eq!(
            *s,
            md_read_section {
                heading: none(e.heading),
                outline_index: none(e.outline_index),
                level: e.level.into(),
                reserved: 0,
                words: e.words,
                seconds: e.seconds,
                cumulative_words: e.cumulative_words,
                cumulative_seconds: e.cumulative_seconds,
            }
        );
        if i > 0 {
            let entry = outline[s.outline_index as usize];
            assert_eq!((entry.node, entry.level), (s.heading, s.level));
        }
    }
}

fn models() -> Vec<u32> {
    let mut models = vec![MD_MODEL_V1];
    if cfg!(feature = "model-v2") {
        models.push(MD_MODEL_V2);
    }
    models
}

#[test]
fn find_and_read_stats_match_the_rust_api_over_the_corpus() {
    let _s = serial();
    let fixed = [
        "a", "e", "the", "é", "e\u{301}", "Caf", "ß", "SS", "ẞ", "#", "*", "`", "|", "-", "[!",
        "&", "👍", "中", " ", "", "x y", "foo bar",
    ];
    for model in models() {
        let session = create(model);
        for source in corpus() {
            let snapshot = publish(session, &source);
            let mut queries: Vec<&str> = fixed.to_vec();
            let mut seen = BTreeSet::new();
            queries.extend(
                source
                    .split_whitespace()
                    .filter(|w| seen.insert(*w))
                    .take(60),
            );
            let lines: Vec<&str> = source.lines().filter(|l| !l.trim().is_empty()).collect();
            queries.extend(lines.iter().take(10).map(|l| l.trim()));
            for query in queries {
                for flags in 0..4 {
                    assert_find_parity(snapshot, query, flags, 0);
                }
            }
            assert_find_parity(snapshot, "e", 0, 3);
            for flags in 0..4 {
                for wpm in [0, 60, 160, 173, 600] {
                    assert_read_parity(snapshot, flags, wpm);
                }
            }
            assert_eq!(md_snapshot_release(snapshot), MD_OK);
        }
        assert_eq!(md_session_release(session), MD_OK);
    }
}

#[test]
fn find_maps_unicode_atomic_and_cross_inline_matches() {
    let _s = serial();
    for model in models() {
        let session = create(model);
        let source = "# Caf\u{e9} cafe\u{301}\n\nSome *text* and [a link](#x) &amp; `co de`.\n\n\
                      Stra\u{df}e STRASSE \u{1e9e} \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} \u{1f468}\n";
        let snapshot = publish(session, source);
        let find = |query: &str, flags| find_all(snapshot, query.as_bytes(), &find_opts(flags, 0));

        // Across emphasis and link text: three exact segments in one unit.
        let (_, m, s) = find("TEXT AND A", 0).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].flags, MD_MATCH_EXACT);
        assert_eq!(
            &source[m[0].span_start as usize..m[0].span_end as usize],
            "text* and [a"
        );
        assert_eq!(s.len(), 3);
        assert!(s.iter().all(|s| s.flags == MD_MATCH_EXACT));
        assert_eq!(
            find("TEXT AND A", MD_FIND_CASE_SENSITIVE).unwrap().1.len(),
            0
        );

        // An entity reference is atomic: the match covers the whole `&amp;`.
        let (_, m, s) = find("&", 0).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].flags, 0);
        assert_eq!(
            &source[m[0].span_start as usize..m[0].span_end as usize],
            "&amp;"
        );
        assert_eq!((s[0].text_start, s[0].text_end, s[0].flags), (0, 1, 0));

        // No normalization; simple case folding only; cluster boundaries.
        assert_eq!(find("caf\u{e9}", 0).unwrap().1.len(), 1);
        assert_eq!(find("cafe\u{301}", 0).unwrap().1.len(), 1);
        assert_eq!(find("cafe", 0).unwrap().1.len(), 0, "inside NFD é");
        assert_eq!(find("\u{df}", 0).unwrap().1.len(), 2, "ß and ẞ");
        assert_eq!(find("ss", 0).unwrap().1.len(), 1, "STRASSE only");
        assert_eq!(
            find("\u{1f468}", 0).unwrap().1.len(),
            1,
            "not inside the ZWJ family"
        );

        // Spans are UTF-8 byte offsets.
        let (_, m, _) = find("\u{1f468}", MD_FIND_WHOLE_WORD).unwrap();
        assert_eq!(
            &source[m[0].span_start as usize..m[0].span_end as usize],
            "\u{1f468}"
        );
        for query in ["code", "co de", "Caf", "stra\u{df}e"] {
            for flags in 0..4 {
                assert_find_parity(snapshot, query, flags, 0);
            }
        }
        assert_eq!(md_snapshot_release(snapshot), MD_OK);
        assert_eq!(md_session_release(session), MD_OK);
    }
}

#[test]
fn find_two_call_sizing_truncation_and_untouched_arrays() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let snapshot = publish(session, "a *a* a\n\na a\n");
    let options = find_opts(0, 0);
    let sentinel_match = md_find_match {
        unit: 0xAAAA,
        ..Default::default()
    };
    let sentinel_segment = md_match_segment {
        node: 0xBBBB,
        ..Default::default()
    };

    // Sizing call: required counts, nothing written.
    let mut out = find_result();
    assert_eq!(
        find_into(snapshot, b"a", &options, &mut [], &mut [], &mut out),
        MD_ERR_BUFFER_TOO_SMALL
    );
    assert_eq!((out.flags, out.match_count, out.segment_count), (0, 5, 5));

    // Enough matches but too few segments: nothing written either.
    let mut matches = [sentinel_match; 8];
    let mut segments = [sentinel_segment; 4];
    let mut out = find_result();
    assert_eq!(
        find_into(
            snapshot,
            b"a",
            &options,
            &mut matches,
            &mut segments,
            &mut out
        ),
        MD_ERR_BUFFER_TOO_SMALL
    );
    assert_eq!((out.match_count, out.segment_count), (5, 5));
    assert!(matches.iter().all(|m| *m == sentinel_match));
    assert!(segments.iter().all(|s| *s == sentinel_segment));

    // Larger arrays: only the reported prefix is written.
    let mut segments = [sentinel_segment; 8];
    let mut out = find_result();
    assert_eq!(
        find_into(
            snapshot,
            b"a",
            &options,
            &mut matches,
            &mut segments,
            &mut out
        ),
        MD_OK
    );
    assert_eq!((out.match_count, out.segment_count), (5, 5));
    assert!(matches[..5].iter().all(|m| m.segment_count == 1));
    assert!(matches[5..].iter().all(|m| *m == sentinel_match));
    assert!(segments[5..].iter().all(|s| *s == sentinel_segment));
    let starts: Vec<u32> = matches[..5].iter().map(|m| m.span_start).collect();
    assert_eq!(starts, [0, 3, 6, 9, 11]);

    // Truncation is reported with the capped count.
    let (out, m, _) = find_all(snapshot, b"a", &find_opts(0, 2)).unwrap();
    assert_eq!((out.flags, m.len()), (MD_FIND_TRUNCATED, 2));
    let (out, m, _) = find_all(snapshot, b"a", &find_opts(0, 5)).unwrap();
    assert_eq!((out.flags, m.len()), (0, 5));

    // Empty, whitespace-only and no-match queries succeed with null arrays.
    for query in [&b""[..], b"  \t", b"zzz"] {
        let mut out = find_result();
        out.flags = 9;
        assert_eq!(
            find_into(snapshot, query, &options, &mut [], &mut [], &mut out),
            MD_OK
        );
        assert_eq!((out.flags, out.match_count, out.segment_count), (0, 0, 0));
    }
    let mut out = find_result();
    // SAFETY: null query with zero length is allowed.
    let status = unsafe {
        md_snapshot_find(
            snapshot,
            null(),
            0,
            &options,
            null_mut(),
            0,
            null_mut(),
            0,
            &mut out,
        )
    };
    assert_eq!(status, MD_OK);

    // Read stats: two-call sizing and an untouched short array.
    let sentinel_section = md_read_section {
        heading: 0xCCCC,
        ..Default::default()
    };
    let doc = publish(session, "intro words\n\n# A\n\none\n\n## B\n\ntwo three\n");
    let mut out = read_result();
    assert_eq!(
        read_into(doc, &read_opts(0, 0), &mut [], &mut out),
        MD_ERR_BUFFER_TOO_SMALL
    );
    assert_eq!(
        (
            out.section_count,
            out.words,
            out.seconds,
            out.words_per_minute
        ),
        (3, 5, 2, 160)
    );
    let mut short = [sentinel_section; 2];
    assert_eq!(
        read_into(doc, &read_opts(0, 0), &mut short, &mut read_result()),
        MD_ERR_BUFFER_TOO_SMALL
    );
    assert!(short.iter().all(|s| *s == sentinel_section));
    let (out, sections) = read_all(doc, &read_opts(MD_READ_INCLUDE_HEADINGS, 60)).unwrap();
    assert_eq!((out.words, out.seconds, out.words_per_minute), (7, 7, 60));
    let summary: Vec<_> = sections
        .iter()
        .map(|s| (s.heading != MD_NONE, s.level, s.words, s.cumulative_words))
        .collect();
    assert_eq!(
        summary,
        [(false, 0, 2, 2), (true, 1, 2, 5), (true, 2, 3, 3)]
    );
    for snapshot in [snapshot, doc] {
        assert_eq!(md_snapshot_release(snapshot), MD_OK);
    }
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn find_and_read_stats_validate_arguments_and_zero_outputs() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let snapshot = publish(session, "find me\n");
    let dirty_find = || md_find_result {
        struct_size: size::<md_find_result>(),
        flags: 7,
        match_count: 7,
        segment_count: 7,
    };
    let check_find = |query: &[u8], options: &md_find_options, expected: md_status| {
        let mut out = dirty_find();
        let mut matches = [md_find_match::default(); 2];
        let mut segments = [md_match_segment::default(); 2];
        let status = find_into(
            snapshot,
            query,
            options,
            &mut matches,
            &mut segments,
            &mut out,
        );
        assert_eq!(status, expected, "{options:?}");
        assert_eq!(
            (
                out.struct_size,
                out.flags,
                out.match_count,
                out.segment_count
            ),
            (size::<md_find_result>(), 0, 0, 0),
            "zeroed on failure"
        );
    };
    let base = find_opts(0, 0);
    for (options, expected) in [
        (
            md_find_options {
                struct_size: 0,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_find_options {
                struct_size: 28,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_find_options {
                match_size: 20,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_find_options {
                segment_size: 0,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_find_options { flags: 4, ..base },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_find_options {
                reserved: 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_find_options {
                max_matches: MD_FIND_MAX_MATCHES + 1,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
    ] {
        check_find(b"me", &options, expected);
    }
    check_find(b"\xff", &base, MD_ERR_INVALID_ARGUMENT);
    check_find(
        &vec![b'a'; MD_FIND_MAX_QUERY_BYTES as usize + 1],
        &base,
        MD_ERR_INVALID_ARGUMENT,
    );
    let longest = vec![b'a'; MD_FIND_MAX_QUERY_BYTES as usize];
    assert!(find_all(snapshot, &longest, &base).unwrap().1.is_empty());
    assert_eq!(
        find_all(snapshot, b"me", &find_opts(0, MD_FIND_MAX_MATCHES))
            .unwrap()
            .1
            .len(),
        1
    );

    // SAFETY: every non-null pointer is valid for its use; null, misaligned
    // and oversized arguments are rejected before access.
    unsafe {
        let mut out;
        let mut matches = [md_find_match::default(); 2];
        let mut segments = [md_match_segment::default(); 2];
        let (m, s) = (matches.as_mut_ptr(), segments.as_mut_ptr());
        let q = b"me".as_ptr();
        let status = |snapshot, query, len, options, m, mc, s, sc, out| {
            md_snapshot_find(snapshot, query, len, options, m, mc, s, sc, out)
        };
        assert_eq!(
            status(snapshot, q, 2, &base, m, 2, s, 2, null_mut()),
            MD_ERR_INVALID_ARGUMENT
        );
        let mut wrong = find_result();
        wrong.struct_size = 16;
        wrong.flags = 3;
        assert_eq!(
            status(snapshot, q, 2, &base, m, 2, s, 2, &mut wrong),
            MD_ERR_STRUCT_SIZE
        );
        assert_eq!(wrong.flags, 3, "a rejected struct is not written");
        for (snapshot, query, len, options, mc, sc, expected) in [
            (snapshot, q, 2, null(), 2, 2, MD_ERR_INVALID_ARGUMENT),
            (
                snapshot,
                null(),
                2,
                &base as *const _,
                2,
                2,
                MD_ERR_INVALID_ARGUMENT,
            ),
            (0, q, 2, &base, 2, 2, MD_ERR_INVALID_HANDLE),
            (u64::MAX, q, 2, &base, 2, 2, MD_ERR_INVALID_HANDLE),
            (
                snapshot,
                q,
                2,
                &base,
                usize::MAX,
                2,
                MD_ERR_INVALID_ARGUMENT,
            ),
            (
                snapshot,
                q,
                2,
                &base,
                2,
                usize::MAX / 8,
                MD_ERR_INVALID_ARGUMENT,
            ),
        ] {
            out = dirty_find();
            assert_eq!(
                status(snapshot, query, len, options, m, mc, s, sc, &mut out),
                expected
            );
            assert_eq!((out.flags, out.match_count), (0, 0));
        }
        // Null arrays with a nonzero capacity, and misaligned arrays.
        out = dirty_find();
        assert_eq!(
            status(snapshot, q, 2, &base, null_mut(), 1, s, 2, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            status(snapshot, q, 2, &base, m, 2, null_mut(), 1, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        let odd = (m as *mut u8).wrapping_add(2) as *mut md_find_match;
        assert_eq!(
            status(snapshot, q, 2, &base, odd, 1, s, 2, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        let odd = (s as *mut u8).wrapping_add(2) as *mut md_match_segment;
        assert_eq!(
            status(snapshot, q, 2, &base, m, 2, odd, 0, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        let odd = (&mut out as *mut md_find_result as *mut u8).wrapping_add(1);
        assert_eq!(
            status(
                snapshot,
                q,
                2,
                &base,
                m,
                2,
                s,
                2,
                odd as *mut md_find_result
            ),
            MD_ERR_INVALID_ARGUMENT
        );
        let mut storage = [0u64; 4];
        let odd_options =
            (storage.as_mut_ptr() as *mut u8).wrapping_add(2) as *const md_find_options;
        assert_eq!(
            status(snapshot, q, 2, odd_options, m, 2, s, 2, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
    }

    // Read stats.
    let check_read = |options: &md_read_options, expected: md_status| {
        let mut out = md_read_stats {
            struct_size: size::<md_read_stats>(),
            words_per_minute: 7,
            words: 7,
            seconds: 7,
            section_count: 7,
        };
        let mut sections = [md_read_section::default(); 2];
        assert_eq!(
            read_into(snapshot, options, &mut sections, &mut out),
            expected
        );
        assert_eq!(
            (
                out.struct_size,
                out.words_per_minute,
                out.words,
                out.seconds,
                out.section_count
            ),
            (size::<md_read_stats>(), 0, 0, 0, 0)
        );
    };
    let base = read_opts(0, 0);
    for (options, expected) in [
        (
            md_read_options {
                struct_size: 12,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_read_options {
                section_size: 40,
                ..base
            },
            MD_ERR_STRUCT_SIZE,
        ),
        (
            md_read_options {
                words_per_minute: 59,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_read_options {
                words_per_minute: 601,
                ..base
            },
            MD_ERR_INVALID_ARGUMENT,
        ),
        (
            md_read_options { flags: 4, ..base },
            MD_ERR_INVALID_ARGUMENT,
        ),
    ] {
        check_read(&options, expected);
    }
    for wpm in [60, 600] {
        assert_eq!(
            read_all(snapshot, &read_opts(0, wpm))
                .unwrap()
                .0
                .words_per_minute,
            wpm
        );
    }
    // SAFETY: null, misaligned and oversized arguments are rejected before
    // access; other pointers are valid.
    unsafe {
        let mut sections = [md_read_section::default(); 2];
        let p = sections.as_mut_ptr();
        let mut out = read_result();
        assert_eq!(
            md_snapshot_read_stats(snapshot, null(), p, 2, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            md_snapshot_read_stats(snapshot, &base, p, 2, null_mut()),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            md_snapshot_read_stats(snapshot, &base, null_mut(), 1, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            md_snapshot_read_stats(snapshot, &base, p, usize::MAX / 16, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        let odd = (p as *mut u8).wrapping_add(4) as *mut md_read_section;
        assert_eq!(
            md_snapshot_read_stats(snapshot, &base, odd, 1, &mut out),
            MD_ERR_INVALID_ARGUMENT
        );
        assert_eq!(
            md_snapshot_read_stats(0, &base, p, 2, &mut out),
            MD_ERR_INVALID_HANDLE
        );
    }
    assert_eq!(md_snapshot_release(snapshot), MD_OK);
    // A released handle is rejected.
    check_find(b"me", &find_opts(0, 0), MD_ERR_INVALID_HANDLE);
    check_read(&read_opts(0, 0), MD_ERR_INVALID_HANDLE);
    assert_eq!(md_session_release(session), MD_OK);
}

#[test]
fn find_and_read_stats_contain_panics_and_leave_arrays_untouched() {
    let _s = serial();
    let session = create(MD_MODEL_V1);
    let snapshot = publish(session, "# a\n\na a\n");
    let sentinel = md_find_match {
        unit: 0xAAAA,
        ..Default::default()
    };
    let mut matches = [sentinel; 4];
    let mut segments = [md_match_segment::default(); 4];
    let mut out = find_result();
    out.match_count = 9;
    inject_panic();
    assert_eq!(
        find_into(
            snapshot,
            b"a",
            &find_opts(0, 0),
            &mut matches,
            &mut segments,
            &mut out
        ),
        MD_ERR_PANIC
    );
    assert_eq!(
        (out.struct_size, out.match_count),
        (size::<md_find_result>(), 0)
    );
    assert!(matches.iter().all(|m| *m == sentinel));

    let section = md_read_section {
        heading: 0xCCCC,
        ..Default::default()
    };
    let mut sections = [section; 4];
    let mut stats = read_result();
    stats.words = 9;
    inject_panic();
    assert_eq!(
        read_into(snapshot, &read_opts(0, 0), &mut sections, &mut stats),
        MD_ERR_PANIC
    );
    assert_eq!(
        (stats.struct_size, stats.words),
        (size::<md_read_stats>(), 0)
    );
    assert!(sections.iter().all(|s| *s == section));

    // Still usable afterwards, and from several threads at once.
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..50 {
                    assert_eq!(
                        find_all(snapshot, b"a", &find_opts(0, 0)).unwrap().1.len(),
                        3
                    );
                    assert_eq!(read_all(snapshot, &read_opts(0, 0)).unwrap().0.words, 2);
                }
            });
        }
    });
    assert_eq!(md_snapshot_release(snapshot), MD_OK);
    assert_eq!(md_session_release(session), MD_OK);
}

mod workspace;
