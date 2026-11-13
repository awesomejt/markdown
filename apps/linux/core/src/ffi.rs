//! Hand-written declarations for the parts of `core/include/markdown_core.h` (C ABI v1)
//! this host uses (D-009). Keep each record field-for-field identical to the header;
//! `runtime::validate` compares every size with the linked library at startup.
//! Find, reading statistics and workspace search are declared when a phase adopts them.

#![allow(non_camel_case_types)]

pub type md_status = u32;
pub type md_session = u64;
pub type md_job = u64;
pub type md_snapshot = u64;

pub const MD_ABI_VERSION: u32 = 1;
pub const MD_NONE: u32 = 0xFFFF_FFFF;
pub const MD_ANCHOR_MAX_BYTES: u32 = 1024;

pub const MD_OK: md_status = 0;
pub const MD_ERR_INVALID_ARGUMENT: md_status = 1;
pub const MD_ERR_VERSION_MISMATCH: md_status = 2;
pub const MD_ERR_STRUCT_SIZE: md_status = 3;
pub const MD_ERR_INVALID_HANDLE: md_status = 4;
pub const MD_ERR_UNSUPPORTED_MODEL: md_status = 5;
pub const MD_ERR_BUFFER_TOO_SMALL: md_status = 6;
pub const MD_ERR_INVALID_DOCUMENT_ID: md_status = 9;
pub const MD_ERR_INVALID_REVISION_TOKEN: md_status = 10;
pub const MD_ERR_SUPERSEDED: md_status = 12;

pub const MD_FIND_OPTIONS_SIZE: u32 = 24;
pub const MD_FIND_RESULT_SIZE: u32 = 24;
pub const MD_FIND_MATCH_SIZE: u32 = 24;
pub const MD_MATCH_SEGMENT_SIZE: u32 = 24;
pub const MD_FIND_MAX_QUERY_BYTES: u32 = 4096;
pub const MD_FIND_DEFAULT_MAX_MATCHES: u32 = 10000;
pub const MD_FIND_MAX_MATCHES: u32 = 1000000;

pub const MD_FIND_CASE_SENSITIVE: u32 = 1;
pub const MD_FIND_WHOLE_WORD: u32 = 2;
pub const MD_FIND_TRUNCATED: u32 = 1;
pub const MD_MATCH_EXACT: u32 = 1;

pub const MD_MODEL_V1: u32 = 1;
pub const MD_MODEL_V2: u32 = 2;

pub const MD_AVAILABLE: u32 = 0;
pub const MD_DELETED: u32 = 1;
pub const MD_UNREADABLE: u32 = 2;
pub const MD_REASON_NONE: u32 = 0;
pub const MD_REASON_PERMISSION_DENIED: u32 = 1;
pub const MD_REASON_ACCESS_REVOKED: u32 = 2;
pub const MD_REASON_BUSY: u32 = 3;
pub const MD_REASON_IO: u32 = 4;
pub const MD_CHANGE_DELETED: u32 = 2;
pub const MD_CHANGE_UNREADABLE: u32 = 3;
pub const MD_DISPOSITION_RECORDED: u32 = 4;

pub const MD_VIEW_HAS_FRONT_MATTER: u32 = 1;
pub const MD_VIEW_HAS_TITLE: u32 = 2;

pub const MD_NODE_PARAGRAPH: u32 = 1;
pub const MD_NODE_HEADING: u32 = 2;
pub const MD_NODE_BLOCK_QUOTE: u32 = 3;
pub const MD_NODE_LIST: u32 = 4;
pub const MD_NODE_LIST_ITEM: u32 = 5;
pub const MD_NODE_CODE_BLOCK: u32 = 6;
pub const MD_NODE_THEMATIC_BREAK: u32 = 7;
pub const MD_NODE_TABLE: u32 = 8;
pub const MD_NODE_TABLE_ROW: u32 = 9;
pub const MD_NODE_TABLE_CELL: u32 = 10;
pub const MD_NODE_FOOTNOTE_DEFINITION: u32 = 11;
pub const MD_NODE_ALERT: u32 = 12;
pub const MD_NODE_UNSUPPORTED_BLOCK: u32 = 13;
pub const MD_NODE_TEXT: u32 = 14;
pub const MD_NODE_EMPHASIS: u32 = 15;
pub const MD_NODE_STRONG: u32 = 16;
pub const MD_NODE_STRIKETHROUGH: u32 = 17;
pub const MD_NODE_CODE_SPAN: u32 = 18;
pub const MD_NODE_SOFT_BREAK: u32 = 19;
pub const MD_NODE_HARD_BREAK: u32 = 20;
pub const MD_NODE_LINK: u32 = 21;
pub const MD_NODE_IMAGE: u32 = 22;
pub const MD_NODE_FOOTNOTE_REFERENCE: u32 = 23;
pub const MD_NODE_UNSUPPORTED_INLINE: u32 = 24;
pub const MD_NODE_ORDERED: u32 = 1;
pub const MD_TARGET_FRAGMENT: u32 = 1;
pub const MD_TARGET_REQUIRES_CAPABILITY: u32 = 2;
pub const MD_TARGET_EXTERNAL: u32 = 3;
pub const MD_TARGET_DENIED: u32 = 4;

pub const MD_DENIAL_NONE: u32 = 0;
pub const MD_DENIAL_UNSAFE: u32 = 1;
pub const MD_DENIAL_INVALID_URI: u32 = 2;
pub const MD_DENIAL_UNSUPPORTED_SCHEME: u32 = 3;
pub const MD_DENIAL_REMOTE_RESOURCE: u32 = 4;
pub const MD_DENIAL_LIMIT: u32 = 5;
pub const MD_DENIAL_LEGACY_POLICY: u32 = 6;

/// A UTF-8 byte range in the snapshot string table; not NUL-terminated.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_str {
    pub offset: u32,
    pub length: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_abi_info {
    pub struct_size: u32,
    pub abi_version: u32,
    pub model_versions: u32,
    pub anchor_version: u32,
    pub pointer_size: u32,
    pub max_anchor_bytes: u32,
    pub session_options_size: u32,
    pub session_status_size: u32,
    pub job_result_size: u32,
    pub snapshot_view_size: u32,
    pub node_size: u32,
    pub intent_size: u32,
    pub outline_entry_size: u32,
    pub diagnostic_size: u32,
    pub property_size: u32,
    pub anchor_resolution_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_session_options {
    pub struct_size: u32,
    pub model_version: u32,
    pub max_source_bytes: u64,
    pub max_nodes: u64,
    pub max_depth: u64,
    pub max_events: u64,
    pub max_text_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_session_status_info {
    pub struct_size: u32,
    pub flags: u32,
    pub latest: u64,
    pub confirmed: u64,
    pub pending: u64,
    pub published: u64,
    pub failure_generation: u64,
    pub availability: u32,
    pub unreadable_reason: u32,
    pub failure: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_job_result {
    pub struct_size: u32,
    pub outcome: u32,
    pub generation: u64,
    pub latest: u64,
    pub snapshot: md_snapshot,
    pub failure: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_node {
    pub kind: u32,
    pub parent: u32,
    pub first_child: u32,
    pub child_count: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub flags: u32,
    pub value: u32,
    pub number: u64,
    pub id: md_str,
    pub text: md_str,
    pub extra: md_str,
    pub aux: u32,
    pub aux_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_intent {
    pub id: md_str,
    pub destination: md_str,
    pub target: md_str,
    pub fragment: md_str,
    pub span_start: u32,
    pub span_end: u32,
    pub definition_start: u32,
    pub definition_end: u32,
    pub source_kind: u32,
    pub target_kind: u32,
    pub denial: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_outline_entry {
    pub node: u32,
    pub level: u32,
    pub title: md_str,
    pub anchor: md_str,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_diagnostic {
    pub code: md_str,
    pub message: md_str,
    pub severity: u32,
    pub flags: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub node: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_property {
    pub key: md_str,
    pub kind: u32,
    pub boolean: u32,
    pub integer: i64,
    pub string: md_str,
    pub first_string: u32,
    pub string_count: u32,
}

/// Borrowed view of one snapshot: valid only until that snapshot is released.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct md_snapshot_view {
    pub struct_size: u32,
    pub model_version: u32,
    pub generation: u64,
    pub source_length: u64,
    pub strings_length: u64,
    pub source: *const u8,
    pub strings: *const u8,
    pub nodes: *const md_node,
    pub roots: *const u32,
    pub children: *const u32,
    pub alignments: *const u32,
    pub links: *const md_intent,
    pub resources: *const md_intent,
    pub outline: *const md_outline_entry,
    pub diagnostics: *const md_diagnostic,
    pub properties: *const md_property,
    pub string_list: *const md_str,
    pub node_count: u32,
    pub root_count: u32,
    pub child_count: u32,
    pub alignment_count: u32,
    pub link_count: u32,
    pub resource_count: u32,
    pub outline_count: u32,
    pub diagnostic_count: u32,
    pub property_count: u32,
    pub string_list_count: u32,
    pub capabilities: u32,
    pub flags: u32,
    pub front_matter_start: u32,
    pub front_matter_end: u32,
    pub document_id: md_str,
    pub revision_id: md_str,
    pub source_sha256: md_str,
    pub title: md_str,
}

impl Default for md_snapshot_view {
    fn default() -> Self {
        // SAFETY: every field is an integer, an md_str or a nullable raw pointer,
        // for which all-zero bytes are valid (null pointers, zero counts).
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_anchor_resolution {
    pub struct_size: u32,
    pub quality: u32,
    pub node: u32,
    pub reserved: u32,
    pub offset: u64,
    pub source_offset: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_find_options {
    pub struct_size: u32,
    pub flags: u32,
    pub max_matches: u32,
    pub match_size: u32,
    pub segment_size: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct md_find_result {
    pub struct_size: u32,
    pub flags: u32,
    pub match_count: u64,
    pub segment_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_find_match {
    pub unit: u32,
    pub flags: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub first_segment: u32,
    pub segment_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct md_match_segment {
    pub node: u32,
    pub flags: u32,
    pub text_start: u32,
    pub text_end: u32,
    pub span_start: u32,
    pub span_end: u32,
}

/// `size_of::<T>()` as the ABI's `uint32_t` struct size.
pub const fn size_of<T>() -> u32 {
    std::mem::size_of::<T>() as u32
}

// Statically linked from the staged model-v2 package (see build.rs).
unsafe extern "C" {
    pub fn md_abi_negotiate(abi_version: u32, info: *mut md_abi_info) -> md_status;
    pub fn md_status_name(status: md_status) -> *const std::ffi::c_char;
    pub fn md_live_handles(sessions: *mut u64, jobs: *mut u64, snapshots: *mut u64) -> md_status;

    pub fn md_session_create(
        document_id: *const u8,
        document_id_length: usize,
        options: *const md_session_options,
        out_session: *mut md_session,
    ) -> md_status;
    pub fn md_session_release(session: md_session) -> md_status;
    pub fn md_session_submit(
        session: md_session,
        bytes: *const u8,
        length: usize,
        token: *const u8,
        token_length: usize,
        out_job: *mut md_job,
        out_generation: *mut u64,
    ) -> md_status;
    pub fn md_session_external_change(
        session: md_session,
        change: u32,
        reason: u32,
        token: *const u8,
        token_length: usize,
        out_disposition: *mut u32,
        out_generation: *mut u64,
    ) -> md_status;
    pub fn md_session_cancel(session: md_session, out_cancelled: *mut u32) -> md_status;
    pub fn md_session_get_status(
        session: md_session,
        out_status: *mut md_session_status_info,
    ) -> md_status;
    pub fn md_session_current(session: md_session, out_snapshot: *mut md_snapshot) -> md_status;

    pub fn md_job_run(job: md_job, out_result: *mut md_job_result) -> md_status;
    pub fn md_job_cancel(job: md_job) -> md_status;
    pub fn md_job_release(job: md_job) -> md_status;

    pub fn md_snapshot_get_view(
        snapshot: md_snapshot,
        out_view: *mut md_snapshot_view,
    ) -> md_status;
    pub fn md_snapshot_find(
        snapshot: md_snapshot,
        query: *const u8,
        query_length: usize,
        options: *const md_find_options,
        matches: *mut md_find_match,
        match_capacity: usize,
        segments: *mut md_match_segment,
        segment_capacity: usize,
        out_result: *mut md_find_result,
    ) -> md_status;
    pub fn md_snapshot_release(snapshot: md_snapshot) -> md_status;
}
