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
    assert_eq!(md_snapshot_release(current), MD_OK);
    assert_eq!(md_snapshot_release(other), MD_OK);
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
