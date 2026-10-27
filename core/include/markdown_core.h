/*
 * markdown_core.h - hand-written C ABI for the portable Markdown core.
 *
 * SPDX-License-Identifier: Apache-2.0
 *
 * ABI version 1 (D-009). Contract: specs/ffi/README.md. Summary:
 *
 * - Call md_abi_negotiate() first and compare every reported struct size with
 *   sizeof() in this header; refuse to continue on any mismatch.
 * - Every struct passed in either direction starts with struct_size, which the
 *   caller sets to sizeof(type). ABI v1 rejects any other value with
 *   MD_ERR_STRUCT_SIZE.
 * - Every function returns an md_status and never unwinds across the
 *   boundary. In the default panic=unwind build a Rust panic returns
 *   MD_ERR_PANIC; a library built with panic=abort aborts the process
 *   instead (ABI v1 cannot report which). Outputs are zeroed (struct_size kept) before
 *   any other work and receive final values only on success, so they are
 *   zeroed on every failure. Exceptions: BUFFER_TOO_SMALL reports the needed
 *   length, and a VERSION_MISMATCH from md_abi_negotiate fills md_abi_info.
 * - Handles are opaque nonzero 64-bit values owned by the core. Each handle
 *   type has exactly one release function. Releasing 0 is a no-op (MD_OK);
 *   releasing an unknown, already released or wrong-type handle returns
 *   MD_ERR_INVALID_HANDLE and has no effect. Handle values are never reused
 *   within a process.
 * - All memory reachable from an md_snapshot_view is core-owned, immutable
 *   and valid until that snapshot handle is released. Never free it, write
 *   to it, or read it concurrently with or after md_snapshot_release().
 * - Output pointers must not overlap each other, any input or any buffer.
 *   If they do, the values written are unspecified, but the core only ever
 *   writes outputs through raw stores, so it remains memory-safe.
 * - The core never receives paths. Callers read bytes under their own
 *   permission model and pass copies; the core copies its inputs during the
 *   call and retains no caller pointer.
 * - Status names are static ASCII and never contain document contents.
 */
#ifndef MARKDOWN_CORE_H
#define MARKDOWN_CORE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MD_ABI_VERSION 1u
/* "No index" value for uint32_t node/table indices. */
#define MD_NONE 0xFFFFFFFFu
/* Upper bound of a serialized anchor; buffers of this size always suffice. */
#define MD_ANCHOR_MAX_BYTES 1024u

typedef uint32_t md_status;
typedef uint64_t md_session;
typedef uint64_t md_job;
typedef uint64_t md_snapshot;

/* md_status values. Stable; new codes may be added in later ABI versions. */
enum {
    MD_OK = 0,
    MD_ERR_INVALID_ARGUMENT = 1,   /* null/misaligned pointer, bad length or enum */
    MD_ERR_VERSION_MISMATCH = 2,   /* requested ABI version unsupported */
    MD_ERR_STRUCT_SIZE = 3,        /* struct_size differs from this ABI */
    MD_ERR_INVALID_HANDLE = 4,     /* unknown, released or wrong-type handle */
    MD_ERR_UNSUPPORTED_MODEL = 5,  /* model version not available in this build */
    MD_ERR_BUFFER_TOO_SMALL = 6,   /* required length written to out_length */
    MD_ERR_LIMIT = 7,              /* handle/table/ABI ceiling exceeded */
    MD_ERR_INVALID_STATE = 8,      /* for example a job that already ran */
    MD_ERR_INVALID_DOCUMENT_ID = 9,
    MD_ERR_INVALID_REVISION_TOKEN = 10,
    MD_ERR_GENERATION_EXHAUSTED = 11,
    MD_ERR_ANCHOR_INVALID = 12,    /* malformed or oversized anchor bytes */
    MD_ERR_ANCHOR_VERSION = 13,
    MD_ERR_NO_BLOCKS = 14,         /* snapshot has no anchorable block */
    MD_ERR_UNKNOWN_NODE = 15,
    MD_ERR_PANIC = 16,             /* contained internal panic (a core bug) */
    MD_ERR_INTERNAL = 17           /* internal invariant failure (a core bug) */
};

/* md_session_options.model_version and md_abi_info.model_versions bits. */
enum { MD_MODEL_V1 = 1, MD_MODEL_V2 = 2 };

/* md_job_result.outcome */
enum {
    MD_OUTCOME_PUBLISHED = 1,
    MD_OUTCOME_UNCHANGED = 2,
    MD_OUTCOME_SUPERSEDED = 3,
    MD_OUTCOME_CANCELLED = 4,
    MD_OUTCOME_FAILED = 5,
    MD_OUTCOME_CLOSED = 6
};

/* md_job_result.failure and md_session_status_info.failure */
enum {
    MD_FAILURE_NONE = 0,
    MD_FAILURE_INVALID_UTF8 = 1,
    MD_FAILURE_SOURCE_LIMIT = 2,
    MD_FAILURE_EVENT_LIMIT = 3,
    MD_FAILURE_NODE_LIMIT = 4,
    MD_FAILURE_DEPTH_LIMIT = 5,
    MD_FAILURE_TEXT_LIMIT = 6,
    MD_FAILURE_SOURCE_MAPPING = 7,
    MD_FAILURE_MODEL = 8,
    MD_FAILURE_OTHER = 9
};

/* Availability and external changes. */
enum { MD_AVAILABLE = 0, MD_DELETED = 1, MD_UNREADABLE = 2 };
enum {
    MD_REASON_NONE = 0,
    MD_REASON_PERMISSION_DENIED = 1,
    MD_REASON_ACCESS_REVOKED = 2,
    MD_REASON_BUSY = 3,
    MD_REASON_IO = 4
};
enum { MD_CHANGE_MODIFIED = 1, MD_CHANGE_DELETED = 2, MD_CHANGE_UNREADABLE = 3 };
enum {
    MD_DISPOSITION_UNCHANGED = 1,
    MD_DISPOSITION_READ_REQUIRED = 2,
    MD_DISPOSITION_PENDING = 3,
    MD_DISPOSITION_RECORDED = 4
};
/* md_session_status_info.flags */
enum { MD_STATUS_IS_CURRENT = 1 };

/* md_anchor_resolution.quality */
enum { MD_ANCHOR_LOST = 0, MD_ANCHOR_EXACT = 1, MD_ANCHOR_RELOCATED = 2, MD_ANCHOR_NEAREST = 3 };

/* md_node.kind: the semantic model v1/v2 node kinds. */
enum {
    MD_NODE_PARAGRAPH = 1,
    MD_NODE_HEADING = 2,
    MD_NODE_BLOCK_QUOTE = 3,
    MD_NODE_LIST = 4,
    MD_NODE_LIST_ITEM = 5,
    MD_NODE_CODE_BLOCK = 6,
    MD_NODE_THEMATIC_BREAK = 7,
    MD_NODE_TABLE = 8,
    MD_NODE_TABLE_ROW = 9,
    MD_NODE_TABLE_CELL = 10,
    MD_NODE_FOOTNOTE_DEFINITION = 11,
    MD_NODE_ALERT = 12,
    MD_NODE_UNSUPPORTED_BLOCK = 13,
    MD_NODE_TEXT = 14,
    MD_NODE_EMPHASIS = 15,
    MD_NODE_STRONG = 16,
    MD_NODE_STRIKETHROUGH = 17,
    MD_NODE_CODE_SPAN = 18,
    MD_NODE_SOFT_BREAK = 19,
    MD_NODE_HARD_BREAK = 20,
    MD_NODE_LINK = 21,
    MD_NODE_IMAGE = 22,
    MD_NODE_FOOTNOTE_REFERENCE = 23,
    MD_NODE_UNSUPPORTED_INLINE = 24
};
/* md_node.flags */
enum {
    MD_NODE_ORDERED = 1,      /* List */
    MD_NODE_TIGHT = 2,        /* List */
    MD_NODE_HAS_START = 4,    /* List: number holds the start */
    MD_NODE_HAS_CHECKED = 8,  /* ListItem is a task item */
    MD_NODE_CHECKED = 16,     /* ListItem task state */
    MD_NODE_HEADER = 32,      /* TableRow */
    MD_NODE_HAS_INFO = 64,    /* CodeBlock: extra holds the info string */
    MD_NODE_HAS_TITLE = 128   /* Link/Image: extra holds the title */
};
enum { MD_ALIGN_NONE = 0, MD_ALIGN_LEFT = 1, MD_ALIGN_CENTER = 2, MD_ALIGN_RIGHT = 3 };
enum {
    MD_ALERT_NOTE = 1,
    MD_ALERT_TIP = 2,
    MD_ALERT_IMPORTANT = 3,
    MD_ALERT_WARNING = 4,
    MD_ALERT_CAUTION = 5
};
enum { MD_UNSUPPORTED_UNAVAILABLE = 1, MD_UNSUPPORTED_DISABLED = 2, MD_UNSUPPORTED_INVALID_INPUT = 3 };
enum { MD_SEVERITY_INFO = 1, MD_SEVERITY_WARNING = 2, MD_SEVERITY_ERROR = 3 };
/* md_snapshot_view.capabilities bits */
enum {
    MD_CAP_TABLES = 1,
    MD_CAP_TASK_LISTS = 2,
    MD_CAP_FOOTNOTES = 4,
    MD_CAP_STRIKETHROUGH = 8,
    MD_CAP_FRONT_MATTER = 16,
    MD_CAP_ALERTS = 32
};
/* md_snapshot_view.flags */
enum { MD_VIEW_HAS_FRONT_MATTER = 1, MD_VIEW_HAS_TITLE = 2 };
/* md_intent.source_kind (v1 intents are UNSPECIFIED) */
enum { MD_SOURCE_UNSPECIFIED = 0, MD_SOURCE_INLINE = 1, MD_SOURCE_REFERENCE = 2 };
/* md_intent.target_kind (v1 link dispositions/resource access and v2 targets) */
enum {
    MD_TARGET_FRAGMENT = 1,             /* target: anchor */
    MD_TARGET_REQUIRES_CAPABILITY = 2,  /* v2 target: decoded path; fragment */
    MD_TARGET_EXTERNAL = 3,             /* v2 target: canonical URI */
    MD_TARGET_DENIED = 4                /* v1 target: reason text; v2 denial */
};
/* md_intent.denial (v2 only; 0 for v1) */
enum {
    MD_DENIAL_NONE = 0,
    MD_DENIAL_UNSAFE = 1,
    MD_DENIAL_INVALID_URI = 2,
    MD_DENIAL_UNSUPPORTED_SCHEME = 3,
    MD_DENIAL_REMOTE_RESOURCE = 4,
    MD_DENIAL_LIMIT = 5,
    MD_DENIAL_LEGACY_POLICY = 6
};
/* md_intent.flags */
enum { MD_INTENT_HAS_FRAGMENT = 1 };
/* md_property.kind */
enum {
    MD_VALUE_NULL = 1,
    MD_VALUE_BOOLEAN = 2,
    MD_VALUE_INTEGER = 3,
    MD_VALUE_STRING = 4,
    MD_VALUE_STRINGS = 5
};
/* md_diagnostic.flags */
enum { MD_DIAGNOSTIC_HAS_SPAN = 1 };

/* A UTF-8 byte range in md_snapshot_view.strings; not NUL-terminated. */
typedef struct md_str {
    uint32_t offset;
    uint32_t length;
} md_str;

typedef struct md_abi_info {
    uint32_t struct_size;            /* in */
    uint32_t abi_version;            /* out: MD_ABI_VERSION of the library */
    uint32_t model_versions;         /* out: bit (1 << version) per built model */
    uint32_t anchor_version;
    uint32_t pointer_size;
    uint32_t max_anchor_bytes;
    uint32_t session_options_size;
    uint32_t session_status_size;
    uint32_t job_result_size;
    uint32_t snapshot_view_size;
    uint32_t node_size;
    uint32_t intent_size;
    uint32_t outline_entry_size;
    uint32_t diagnostic_size;
    uint32_t property_size;
    uint32_t anchor_resolution_size;
} md_abi_info;

/* Zero limits select the core defaults; larger values than the documented
   ABI ceilings are rejected with MD_ERR_INVALID_ARGUMENT. */
typedef struct md_session_options {
    uint32_t struct_size;
    uint32_t model_version;          /* MD_MODEL_V1 or MD_MODEL_V2 */
    uint64_t max_source_bytes;
    uint64_t max_nodes;
    uint64_t max_depth;
    uint64_t max_events;
    uint64_t max_text_bytes;
} md_session_options;

/* Generations start at 1; 0 means "none". */
typedef struct md_session_status_info {
    uint32_t struct_size;
    uint32_t flags;                  /* MD_STATUS_IS_CURRENT */
    uint64_t latest;
    uint64_t confirmed;
    uint64_t pending;
    uint64_t published;              /* generation of the published snapshot */
    uint64_t failure_generation;
    uint32_t availability;           /* MD_AVAILABLE/MD_DELETED/MD_UNREADABLE */
    uint32_t unreadable_reason;      /* MD_REASON_* */
    uint32_t failure;                /* MD_FAILURE_* of the newest resolved submission */
    uint32_t reserved;
} md_session_status_info;

typedef struct md_job_result {
    uint32_t struct_size;
    uint32_t outcome;                /* MD_OUTCOME_* */
    uint64_t generation;
    uint64_t latest;                 /* MD_OUTCOME_SUPERSEDED only */
    md_snapshot snapshot;            /* PUBLISHED/UNCHANGED: new handle to release */
    uint32_t failure;                /* MD_OUTCOME_FAILED only */
    uint32_t reserved;
} md_job_result;

typedef struct md_node {
    uint32_t kind;                   /* MD_NODE_* */
    uint32_t parent;                 /* node index or MD_NONE for roots */
    uint32_t first_child;            /* index into children[] */
    uint32_t child_count;
    uint32_t span_start;             /* UTF-8 source byte range [start, end) */
    uint32_t span_end;
    uint32_t flags;                  /* MD_NODE_* flags */
    uint32_t value;                  /* heading level, MD_ALERT_*, MD_UNSUPPORTED_* */
    uint64_t number;                 /* list start (MD_NODE_HAS_START) */
    md_str id;                       /* snapshot-local node ID */
    md_str text;                     /* text, code, heading anchor, label, feature, alt */
    md_str extra;                    /* code info or link/image title */
    uint32_t aux;                    /* link/resource index; table: first alignment */
    uint32_t aux_count;              /* table: column count */
} md_node;

typedef struct md_intent {
    md_str id;
    md_str destination;
    md_str target;                   /* anchor, path, URI or v1 denial reason */
    md_str fragment;                 /* MD_INTENT_HAS_FRAGMENT */
    uint32_t span_start;             /* occurrence span */
    uint32_t span_end;
    uint32_t definition_start;       /* MD_SOURCE_REFERENCE only */
    uint32_t definition_end;
    uint32_t source_kind;            /* MD_SOURCE_* */
    uint32_t target_kind;            /* MD_TARGET_* */
    uint32_t denial;                 /* MD_DENIAL_* */
    uint32_t flags;                  /* MD_INTENT_* */
} md_intent;

typedef struct md_outline_entry {
    uint32_t node;
    uint32_t level;
    md_str title;
    md_str anchor;
} md_outline_entry;

typedef struct md_diagnostic {
    md_str code;
    md_str message;
    uint32_t severity;               /* MD_SEVERITY_* */
    uint32_t flags;                  /* MD_DIAGNOSTIC_HAS_SPAN */
    uint32_t span_start;
    uint32_t span_end;
    uint32_t node;                   /* node index or MD_NONE */
    uint32_t reserved;
} md_diagnostic;

typedef struct md_property {
    md_str key;
    uint32_t kind;                   /* MD_VALUE_* */
    uint32_t boolean;
    int64_t integer;
    md_str string;
    uint32_t first_string;           /* MD_VALUE_STRINGS: index into string_list[] */
    uint32_t string_count;
} md_property;

/* Bulk, read-only view of one snapshot. Arrays with a zero count are NULL. */
typedef struct md_snapshot_view {
    uint32_t struct_size;
    uint32_t model_version;          /* MD_MODEL_V1 or MD_MODEL_V2 */
    uint64_t generation;
    uint64_t source_length;
    uint64_t strings_length;
    const uint8_t *source;           /* exact UTF-8 source all spans refer to */
    const uint8_t *strings;          /* string table for every md_str */
    const md_node *nodes;
    const uint32_t *roots;
    const uint32_t *children;
    const uint32_t *alignments;      /* MD_ALIGN_* */
    const md_intent *links;
    const md_intent *resources;
    const md_outline_entry *outline;
    const md_diagnostic *diagnostics;
    const md_property *properties;
    const md_str *string_list;
    uint32_t node_count;
    uint32_t root_count;
    uint32_t child_count;
    uint32_t alignment_count;
    uint32_t link_count;
    uint32_t resource_count;
    uint32_t outline_count;
    uint32_t diagnostic_count;
    uint32_t property_count;
    uint32_t string_list_count;
    uint32_t capabilities;           /* MD_CAP_* */
    uint32_t flags;                  /* MD_VIEW_* */
    uint32_t front_matter_start;
    uint32_t front_matter_end;
    md_str document_id;
    md_str revision_id;
    md_str source_sha256;            /* lowercase hex */
    md_str title;                    /* MD_VIEW_HAS_TITLE */
} md_snapshot_view;

typedef struct md_anchor_resolution {
    uint32_t struct_size;
    uint32_t quality;                /* MD_ANCHOR_* */
    uint32_t node;                   /* node index or MD_NONE when lost */
    uint32_t reserved;
    uint64_t offset;                 /* byte offset within the node span */
    uint64_t source_offset;          /* absolute source byte offset */
} md_anchor_resolution;

/* Version negotiation and diagnostics. Thread-safe. */
md_status md_abi_negotiate(uint32_t abi_version, md_abi_info *info);
const char *md_status_name(md_status status);
md_status md_live_handles(uint64_t *sessions, uint64_t *jobs, uint64_t *snapshots);

/* Sessions. Thread-safe; one session per document lineage. */
md_status md_session_create(const uint8_t *document_id, size_t document_id_length,
                            const md_session_options *options, md_session *out_session);
md_status md_session_release(md_session session);
md_status md_session_submit(md_session session, const uint8_t *bytes, size_t length,
                            const uint8_t *token, size_t token_length,
                            md_job *out_job, uint64_t *out_generation);
md_status md_session_external_change(md_session session, uint32_t change, uint32_t reason,
                                     const uint8_t *token, size_t token_length,
                                     uint32_t *out_disposition, uint64_t *out_generation);
md_status md_session_cancel(md_session session, uint32_t *out_cancelled);
md_status md_session_get_status(md_session session, md_session_status_info *out_status);
md_status md_session_current(md_session session, md_snapshot *out_snapshot);

/* Jobs. Run once on a caller-owned worker thread; cancel from any thread. */
md_status md_job_run(md_job job, md_job_result *out_result);
md_status md_job_cancel(md_job job);
md_status md_job_release(md_job job);

/* Snapshots. Immutable; usable from any thread until released. */
md_status md_snapshot_get_view(md_snapshot snapshot, md_snapshot_view *out_view);
md_status md_snapshot_anchor_at(md_snapshot snapshot, uint64_t source_offset,
                                uint8_t *buffer, size_t capacity, size_t *out_length);
md_status md_snapshot_anchor_for_node(md_snapshot snapshot, uint32_t node, uint64_t offset,
                                      uint8_t *buffer, size_t capacity, size_t *out_length);
md_status md_snapshot_resolve_anchor(md_snapshot snapshot, const uint8_t *anchor,
                                     size_t anchor_length, md_anchor_resolution *out_resolution);
md_status md_snapshot_release(md_snapshot snapshot);

#ifdef __cplusplus
}
#endif

#endif /* MARKDOWN_CORE_H */
