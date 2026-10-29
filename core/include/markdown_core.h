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

/*
 * ABI v1 additions (D-022): find, word count and section reading time over a
 * snapshot (specs/search/README.md). These add functions only. Each new
 * struct has a size constant below; compare it with sizeof() after
 * negotiation. struct_size and the record sizes in the options structs are
 * checked on every call (MD_ERR_STRUCT_SIZE otherwise).
 *
 * Results go into caller-owned record arrays. If either array is too small,
 * the call returns MD_ERR_BUFFER_TOO_SMALL with the required counts (and
 * flags) in the result struct and writes no records; call again with larger
 * arrays (the search is recomputed). Records are written only on MD_OK.
 * Node indices are md_snapshot_view node indices; spans are source byte
 * ranges [start, end). There is no cancel handle: work is linear in the
 * snapshot's visible text and bounded by max_matches.
 */
#define MD_FIND_OPTIONS_SIZE 24u
#define MD_FIND_RESULT_SIZE 24u
#define MD_FIND_MATCH_SIZE 24u
#define MD_MATCH_SEGMENT_SIZE 24u
#define MD_READ_OPTIONS_SIZE 16u
#define MD_READ_STATS_SIZE 32u
#define MD_READ_SECTION_SIZE 48u
/* Longer queries (raw UTF-8 bytes) are rejected with MD_ERR_INVALID_ARGUMENT. */
#define MD_FIND_MAX_QUERY_BYTES 4096u
/* Results cost about 250 bytes of transient core heap per match (about
   240 MiB at the maximum) besides the caller's arrays; see the contract. */
#define MD_FIND_DEFAULT_MAX_MATCHES 10000u
#define MD_FIND_MAX_MATCHES 1000000u
#define MD_READ_DEFAULT_WPM 160u
#define MD_READ_MIN_WPM 60u
#define MD_READ_MAX_WPM 600u

/* md_find_options.flags */
enum { MD_FIND_CASE_SENSITIVE = 1, MD_FIND_WHOLE_WORD = 2 };
/* md_find_result.flags */
enum { MD_FIND_TRUNCATED = 1 };
/* md_find_match.flags and md_match_segment.flags */
enum { MD_MATCH_EXACT = 1 };
/* md_read_options.flags */
enum { MD_READ_INCLUDE_HEADINGS = 1, MD_READ_INCLUDE_CODE = 2 };

typedef struct md_find_options {
    uint32_t struct_size;            /* MD_FIND_OPTIONS_SIZE */
    uint32_t flags;                  /* MD_FIND_*; unknown bits are rejected */
    uint32_t max_matches;            /* 0 selects MD_FIND_DEFAULT_MAX_MATCHES */
    uint32_t match_size;             /* sizeof(md_find_match) */
    uint32_t segment_size;           /* sizeof(md_match_segment) */
    uint32_t reserved;               /* must be 0 */
} md_find_options;

typedef struct md_find_result {
    uint32_t struct_size;            /* MD_FIND_RESULT_SIZE */
    uint32_t flags;                  /* MD_FIND_TRUNCATED: more than max_matches exist */
    uint64_t match_count;            /* written, or required on BUFFER_TOO_SMALL */
    uint64_t segment_count;
} md_find_result;

/* One match, in reading order. Its segments are
   segments[first_segment .. first_segment + segment_count). */
typedef struct md_find_match {
    uint32_t unit;                   /* leaf block node index */
    uint32_t flags;                  /* MD_MATCH_EXACT */
    uint32_t span_start;             /* smallest source range covering the match */
    uint32_t span_end;
    uint32_t first_segment;
    uint32_t segment_count;
} md_find_match;

/* The part of one match inside one node, for highlighting. */
typedef struct md_match_segment {
    uint32_t node;                   /* node index */
    uint32_t flags;                  /* MD_MATCH_EXACT */
    uint32_t text_start;             /* byte range in the node's display text */
    uint32_t text_end;
    uint32_t span_start;             /* source range (whole node/line when not exact) */
    uint32_t span_end;
} md_match_segment;

typedef struct md_read_options {
    uint32_t struct_size;            /* MD_READ_OPTIONS_SIZE */
    uint32_t flags;                  /* MD_READ_*; unknown bits are rejected */
    uint32_t words_per_minute;       /* 0 selects MD_READ_DEFAULT_WPM; else 60..600 */
    uint32_t section_size;           /* sizeof(md_read_section) */
} md_read_options;

typedef struct md_read_stats {
    uint32_t struct_size;            /* MD_READ_STATS_SIZE */
    uint32_t words_per_minute;       /* the rate used */
    uint64_t words;
    uint64_t seconds;                /* estimate for words, not a sum of sections */
    uint64_t section_count;          /* written, or required on BUFFER_TOO_SMALL */
} md_read_stats;

/* The intro (always first, level 0), then one section per heading. */
typedef struct md_read_section {
    uint32_t heading;                /* heading node index; MD_NONE for the intro */
    uint32_t outline_index;          /* outline entry; MD_NONE for the intro */
    uint32_t level;                  /* 1-6; 0 for the intro */
    uint32_t reserved;
    uint64_t words;                  /* up to the next heading of any level */
    uint64_t seconds;
    uint64_t cumulative_words;       /* including deeper subsections */
    uint64_t cumulative_seconds;
} md_read_section;

/* Literal find. query is UTF-8 (not NUL-terminated); an empty or
   whitespace-only query succeeds with no matches. Arrays may be NULL only
   with a zero capacity. Thread-safe; run off the UI thread. */
md_status md_snapshot_find(md_snapshot snapshot, const uint8_t *query, size_t query_length,
                           const md_find_options *options,
                           md_find_match *matches, size_t match_capacity,
                           md_match_segment *segments, size_t segment_capacity,
                           md_find_result *out_result);
/* Word count and per-section reading time. The section count is always the
   view's outline_count + 1. */
md_status md_snapshot_read_stats(md_snapshot snapshot, const md_read_options *options,
                                 md_read_section *sections, size_t section_capacity,
                                 md_read_stats *out_stats);

/*
 * ABI v1 additions (D-022): workspace search (specs/workspace-search/README.md).
 * No handle is involved. The adapter keeps folder access and file reads and
 * passes display paths (relative, '/'-separated UTF-8 labels that grant no
 * access) and file bytes; the core never opens anything. Size constants,
 * struct_size and record sizes follow the find rules above, and so do
 * BUFFER_TOO_SMALL and zeroed results on failure.
 *
 * Content search accounting is a caller-owned md_content_progress. Start from
 * a zeroed struct with struct_size set and pass the same struct to every
 * admit/search call of one workspace search. It is read on entry and written
 * back only on MD_OK; every failure (including BUFFER_TOO_SMALL) leaves it
 * unchanged. A nonzero stopped is permanent: the search may be incomplete.
 * Calls on one progress struct must not run concurrently.
 */
#define MD_WORKSPACE_PATH_INFO_SIZE 16u
#define MD_WORKSPACE_ENTRY_SIZE 24u
#define MD_NAME_OPTIONS_SIZE 24u
#define MD_NAME_RESULT_SIZE 56u
#define MD_NAME_HIT_SIZE 24u
#define MD_CONTENT_OPTIONS_SIZE 48u
#define MD_CONTENT_PROGRESS_SIZE 88u
#define MD_CONTENT_RESULT_SIZE 112u
#define MD_CONTENT_HIT_SIZE 48u
#define MD_WORKSPACE_MAX_PATH_BYTES 4096u
#define MD_WORKSPACE_MAX_PATH_DEPTH 256u
#define MD_WORKSPACE_MAX_QUERY_BYTES 4096u
#define MD_NAMES_DEFAULT_MAX_RESULTS 200u
#define MD_NAMES_MAX_RESULTS 10000u
#define MD_NAMES_DEFAULT_MAX_ENTRIES 100000u
#define MD_NAMES_MAX_ENTRIES 1000000u
/* Total path_length of the examined entries (ranges may alias); more is
   MD_ERR_LIMIT before any work. */
#define MD_NAMES_MAX_PATH_BYTES 67108864u
#define MD_CONTENT_DEFAULT_MAX_FILES 10000u
#define MD_CONTENT_MAX_FILES 1000000u
#define MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES 268435456u
#define MD_CONTENT_MAX_TOTAL_BYTES 17179869184u
#define MD_CONTENT_DEFAULT_MAX_FILE_BYTES 2097152u
#define MD_CONTENT_MAX_FILE_BYTES 8388608u
#define MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE 100u
#define MD_CONTENT_MAX_MATCHES_PER_FILE 10000u
#define MD_CONTENT_DEFAULT_MAX_TOTAL_MATCHES 2000u
#define MD_CONTENT_MAX_TOTAL_MATCHES 100000u
#define MD_CONTENT_DEFAULT_SNIPPET_CONTEXT 40u
#define MD_CONTENT_MAX_SNIPPET_CONTEXT 200u
/* Upper bound of one hit's snippet text: (2 * 200 + 256) characters of at
   most 4 UTF-8 bytes. hit_capacity * this always suffices for text. */
#define MD_CONTENT_MAX_SNIPPET_BYTES 2624u

/* md_workspace_path_info.error */
enum {
    MD_PATH_VALID = 0,
    MD_PATH_EMPTY = 1,
    MD_PATH_TOO_LONG = 2,            /* over MD_WORKSPACE_MAX_PATH_BYTES */
    MD_PATH_TOO_DEEP = 3,            /* over MD_WORKSPACE_MAX_PATH_DEPTH components */
    MD_PATH_ABSOLUTE = 4,            /* leading '/' */
    MD_PATH_EMPTY_COMPONENT = 5,     /* "//" or a trailing '/' */
    MD_PATH_DOT_COMPONENT = 6,       /* "." or ".." */
    MD_PATH_CONTROL = 7,             /* control or bidi formatting character */
    MD_PATH_INVALID_UTF8 = 8
};
/* md_workspace_path_info.flags (valid paths only) */
enum { MD_PATH_MARKDOWN = 1, MD_PATH_HIDDEN = 2, MD_PATH_INVISIBLE = 4 };
/* md_name_options.flags */
enum { MD_NAMES_CASE_SENSITIVE = 1, MD_NAMES_INCLUDE_HIDDEN = 2, MD_NAMES_ALL_FILES = 4 };
/* md_name_result.flags */
enum { MD_NAMES_TRUNCATED = 1, MD_NAMES_ENTRIES_LIMITED = 2 };
/* md_name_hit.rank, best first */
enum { MD_NAME_RANK_PREFIX = 1, MD_NAME_RANK_NAME = 2, MD_NAME_RANK_PATH = 3 };
/* md_content_options.flags */
enum { MD_CONTENT_CASE_SENSITIVE = 1, MD_CONTENT_WHOLE_WORD = 2, MD_CONTENT_INCLUDE_HIDDEN = 4 };
/* Admission outcomes (READ, SKIPPED, STOPPED) and search outcomes
   (SEARCHED, SKIPPED, STOPPED). */
enum {
    MD_CONTENT_READ = 1,
    MD_CONTENT_SEARCHED = 2,
    MD_CONTENT_SKIPPED = 3,          /* reason is MD_SKIP_*; continue */
    MD_CONTENT_STOPPED = 4           /* reason is MD_STOP_*; stop reading files */
};
enum {
    MD_SKIP_HIDDEN = 1,
    MD_SKIP_NOT_MARKDOWN = 2,
    MD_SKIP_TOO_LARGE = 3,
    MD_SKIP_INVALID_UTF8 = 4,
    MD_SKIP_PARSE_FAILED = 5
};
/* Also md_content_progress.stopped (0 while running). */
enum { MD_STOP_FILES = 1, MD_STOP_BYTES = 2, MD_STOP_MATCHES = 3 };
/* md_content_result.flags */
enum { MD_CONTENT_TRUNCATED = 1 };
/* md_content_hit.flags */
enum { MD_HIT_EXACT = 1, MD_HIT_LEADING_MORE = 2, MD_HIT_TRAILING_MORE = 4, MD_HIT_ANCHOR = 8 };

typedef struct md_workspace_path_info {
    uint32_t struct_size;            /* MD_WORKSPACE_PATH_INFO_SIZE */
    uint32_t error;                  /* MD_PATH_*; MD_PATH_VALID is 0 */
    uint32_t flags;                  /* MD_PATH_MARKDOWN/HIDDEN/INVISIBLE */
    uint32_t name_start;             /* byte offset of the final component */
} md_workspace_path_info;

/* One adapter-supplied file. id is opaque to the core and only copied into
   results; the path is paths[path_offset .. path_offset + path_length). */
typedef struct md_workspace_entry {
    uint64_t id;
    uint64_t path_offset;
    uint32_t path_length;
    uint32_t reserved;               /* must be 0 */
} md_workspace_entry;

typedef struct md_name_options {
    uint32_t struct_size;            /* MD_NAME_OPTIONS_SIZE */
    uint32_t flags;                  /* MD_NAMES_*; unknown bits are rejected */
    uint32_t max_results;            /* 0 selects MD_NAMES_DEFAULT_MAX_RESULTS */
    uint32_t max_entries;            /* 0 selects MD_NAMES_DEFAULT_MAX_ENTRIES */
    uint32_t entry_size;             /* sizeof(md_workspace_entry) */
    uint32_t hit_size;               /* sizeof(md_name_hit) */
} md_name_options;

typedef struct md_name_result {
    uint32_t struct_size;            /* MD_NAME_RESULT_SIZE */
    uint32_t flags;                  /* MD_NAMES_TRUNCATED/ENTRIES_LIMITED */
    uint64_t hit_count;              /* written, or required on BUFFER_TOO_SMALL */
    uint64_t total_hits;             /* every matching entry examined */
    uint64_t entries_examined;       /* at most max_entries, including skipped */
    uint64_t skipped_hidden;
    uint64_t skipped_not_markdown;
    uint64_t skipped_invalid;        /* paths the core rejects; never shown */
} md_name_result;

/* Sorted by rank, then path bytes, then entry index. */
typedef struct md_name_hit {
    uint64_t id;                     /* the entry's id */
    uint32_t entry;                  /* index into the entries array */
    uint32_t rank;                   /* MD_NAME_RANK_* */
    uint32_t start;                  /* byte range of the first match in the path */
    uint32_t end;
} md_name_hit;

/* Zero selects the default for every limit except snippet_context, which is
   used as given (0..200); MD_CONTENT_DEFAULT_SNIPPET_CONTEXT is the default. */
typedef struct md_content_options {
    uint32_t struct_size;            /* MD_CONTENT_OPTIONS_SIZE */
    uint32_t flags;                  /* MD_CONTENT_*; unknown bits are rejected */
    uint64_t max_files;              /* files read, 1..MD_CONTENT_MAX_FILES */
    uint64_t max_total_bytes;        /* bytes read, 1..MD_CONTENT_MAX_TOTAL_BYTES */
    uint32_t max_file_bytes;         /* larger files are skipped; default 2 MiB, max 8 MiB */
    uint32_t max_matches_per_file;
    uint32_t max_total_matches;
    uint32_t snippet_context;        /* characters on each side of a match */
    uint32_t hit_size;               /* sizeof(md_content_hit) */
    uint32_t reserved;               /* must be 0 */
} md_content_options;

typedef struct md_content_progress {
    uint32_t struct_size;            /* MD_CONTENT_PROGRESS_SIZE */
    uint32_t stopped;                /* 0, or the permanent MD_STOP_* reason */
    uint64_t files_read;             /* includes invalid-UTF-8 and parse-failed files */
    uint64_t files_matched;
    uint64_t bytes_read;
    uint64_t matches;
    uint64_t files_truncated;        /* hits cut by a match budget */
    uint64_t skipped_hidden;
    uint64_t skipped_not_markdown;
    uint64_t skipped_too_large;
    uint64_t skipped_invalid_utf8;
    uint64_t skipped_parse_failed;
} md_content_progress;

typedef struct md_content_result {
    uint32_t struct_size;            /* MD_CONTENT_RESULT_SIZE */
    uint32_t outcome;                /* MD_CONTENT_SEARCHED/SKIPPED/STOPPED */
    uint32_t reason;                 /* MD_SKIP_* or MD_STOP_*; 0 when searched */
    uint32_t flags;                  /* MD_CONTENT_TRUNCATED: more matches in this file */
    uint64_t hit_count;              /* written, or required on BUFFER_TOO_SMALL */
    uint64_t text_length;            /* snippet text bytes, likewise */
    uint64_t anchor_length;          /* anchor bytes, likewise */
    /* Searched bytes. If a later snapshot's source length and source_sha256
       are equal, hit spans and units apply to it directly; otherwise resolve
       the hit's anchor. */
    uint64_t source_length;
    uint8_t source_sha256[64];       /* lowercase hex ASCII, not NUL-terminated */
} md_content_result;

/* One match, in reading order. */
typedef struct md_content_hit {
    uint32_t unit;                   /* leaf block node index in a v1 view of these bytes */
    uint32_t flags;                  /* MD_HIT_* */
    uint32_t span_start;             /* source range covering the match */
    uint32_t span_end;
    uint64_t line;                   /* 1-based line of span_start (LF) */
    uint32_t text_offset;            /* snippet: text[text_offset .. + text_length) */
    uint32_t text_length;
    uint32_t match_start;            /* match within the snippet text */
    uint32_t match_end;
    uint32_t anchor_offset;          /* anchors[anchor_offset .. + anchor_length) */
    uint32_t anchor_length;          /* 0 without MD_HIT_ANCHOR */
} md_content_hit;

/* Validate a display path and classify it. Returns MD_OK for any path; the
   verdict is info->error. Use it to skip entries the core would reject. */
md_status md_workspace_classify_path(const uint8_t *path, size_t path_length,
                                     md_workspace_path_info *out_info);
/* Literal file-name search. An empty or whitespace-only query succeeds with
   no hits. An entry whose range lies outside paths is INVALID_ARGUMENT; an
   entry whose path is invalid is skipped and counted in skipped_invalid.
   Examined paths totalling over MD_NAMES_MAX_PATH_BYTES are MD_ERR_LIMIT. */
md_status md_workspace_find_names(const uint8_t *paths, size_t paths_length,
                                  const md_workspace_entry *entries, size_t entry_count,
                                  const uint8_t *query, size_t query_length,
                                  const md_name_options *options,
                                  md_name_hit *hits, size_t hit_capacity,
                                  md_name_result *out_result);
/* Before reading a file: *out_outcome is MD_CONTENT_READ, SKIPPED or STOPPED
   and *out_reason the MD_SKIP_* or MD_STOP_* reason (0 for READ). byte_length is
   the adapter's size estimate. An empty query or invalid path is
   INVALID_ARGUMENT. Optional: content_search re-checks admission. */
md_status md_workspace_content_admit(const uint8_t *query, size_t query_length,
                                     const md_content_options *options,
                                     const uint8_t *path, size_t path_length,
                                     uint64_t byte_length, md_content_progress *progress,
                                     uint32_t *out_outcome, uint32_t *out_reason);
/* Search one file's bytes. Size the arrays from the limits to avoid a
   BUFFER_TOO_SMALL retry, which re-parses the file: max_matches_per_file hits,
   that many MD_CONTENT_MAX_SNIPPET_BYTES of text and MD_ANCHOR_MAX_BYTES of
   anchors. Each call is bounded by max_file_bytes; stop between files to
   cancel. Thread-safe; run off the UI thread. */
md_status md_workspace_content_search(const uint8_t *query, size_t query_length,
                                      const md_content_options *options,
                                      const uint8_t *path, size_t path_length,
                                      const uint8_t *content, size_t content_length,
                                      md_content_progress *progress,
                                      md_content_hit *hits, size_t hit_capacity,
                                      uint8_t *text, size_t text_capacity,
                                      uint8_t *anchors, size_t anchor_capacity,
                                      md_content_result *out_result);

#ifdef __cplusplus
}
#endif

#endif /* MARKDOWN_CORE_H */
