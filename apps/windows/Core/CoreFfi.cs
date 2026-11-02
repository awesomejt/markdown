// Hand-written declarations for core/include/markdown_core.h (C ABI v1) under D-009.
// Keep every record field-for-field identical to the C header; CoreRuntime.Validate()
// verifies every negotiated and D-022 struct size against the linked library at startup.
using System.Runtime.InteropServices;

namespace Markdown.Windows.Core;

internal static unsafe class CoreFfi
{
    internal const string LibraryName = "markdown_ffi";

    internal const uint MD_ABI_VERSION = 1;
    internal const uint MD_ANCHOR_VERSION = 1;
    internal const uint MD_NONE = 0xFFFF_FFFFu;
    internal const uint MD_ANCHOR_MAX_BYTES = 1024;

    // Status codes (md_status).
    internal const uint MD_OK = 0;
    internal const uint MD_ERR_INVALID_ARGUMENT = 1;
    internal const uint MD_ERR_VERSION_MISMATCH = 2;
    internal const uint MD_ERR_STRUCT_SIZE = 3;
    internal const uint MD_ERR_INVALID_HANDLE = 4;
    internal const uint MD_ERR_UNSUPPORTED_MODEL = 5;
    internal const uint MD_ERR_BUFFER_TOO_SMALL = 6;
    internal const uint MD_ERR_LIMIT = 7;
    internal const uint MD_ERR_INVALID_STATE = 8;
    internal const uint MD_ERR_INVALID_DOCUMENT_ID = 9;
    internal const uint MD_ERR_INVALID_REVISION_TOKEN = 10;
    internal const uint MD_ERR_GENERATION_EXHAUSTED = 11;
    internal const uint MD_ERR_ANCHOR_INVALID = 12;
    internal const uint MD_ERR_ANCHOR_VERSION = 13;
    internal const uint MD_ERR_NO_BLOCKS = 14;
    internal const uint MD_ERR_UNKNOWN_NODE = 15;
    internal const uint MD_ERR_PANIC = 16;
    internal const uint MD_ERR_INTERNAL = 17;

    // Semantic model versions.
    internal const uint MD_MODEL_V1 = 1;
    internal const uint MD_MODEL_V2 = 2;

    // Parse job outcomes (md_job_result.outcome).
    internal const uint MD_OUTCOME_PUBLISHED = 1;
    internal const uint MD_OUTCOME_UNCHANGED = 2;
    internal const uint MD_OUTCOME_SUPERSEDED = 3;
    internal const uint MD_OUTCOME_CANCELLED = 4;
    internal const uint MD_OUTCOME_FAILED = 5;
    internal const uint MD_OUTCOME_CLOSED = 6;

    // Parse failure kinds (md_job_result.failure).
    internal const uint MD_FAILURE_NONE = 0;
    internal const uint MD_FAILURE_INVALID_UTF8 = 1;
    internal const uint MD_FAILURE_SOURCE_LIMIT = 2;
    internal const uint MD_FAILURE_EVENT_LIMIT = 3;
    internal const uint MD_FAILURE_NODE_LIMIT = 4;
    internal const uint MD_FAILURE_DEPTH_LIMIT = 5;
    internal const uint MD_FAILURE_TEXT_LIMIT = 6;
    internal const uint MD_FAILURE_SOURCE_MAPPING = 7;
    internal const uint MD_FAILURE_MODEL = 8;
    internal const uint MD_FAILURE_OTHER = 9;

    // Document availability (md_session_status_info.availability).
    internal const uint MD_AVAILABLE = 0;
    internal const uint MD_DELETED = 1;
    internal const uint MD_UNREADABLE = 2;

    // Unreadable reasons.
    internal const uint MD_REASON_NONE = 0;
    internal const uint MD_REASON_PERMISSION_DENIED = 1;
    internal const uint MD_REASON_ACCESS_REVOKED = 2;
    internal const uint MD_REASON_BUSY = 3;
    internal const uint MD_REASON_IO = 4;

    // External change kinds and dispositions.
    internal const uint MD_CHANGE_MODIFIED = 1;
    internal const uint MD_CHANGE_DELETED = 2;
    internal const uint MD_CHANGE_UNREADABLE = 3;
    internal const uint MD_DISPOSITION_UNCHANGED = 1;
    internal const uint MD_DISPOSITION_READ_REQUIRED = 2;
    internal const uint MD_DISPOSITION_PENDING = 3;
    internal const uint MD_DISPOSITION_RECORDED = 4;

    // Session status flags.
    internal const uint MD_STATUS_IS_CURRENT = 1;

    // Snapshot view flags.
    internal const uint MD_VIEW_HAS_FRONT_MATTER = 1;
    internal const uint MD_VIEW_HAS_TITLE = 2;

    // Node kinds.
    internal const uint MD_NODE_PARAGRAPH = 1;
    internal const uint MD_NODE_HEADING = 2;
    internal const uint MD_NODE_BLOCK_QUOTE = 3;
    internal const uint MD_NODE_LIST = 4;
    internal const uint MD_NODE_LIST_ITEM = 5;
    internal const uint MD_NODE_CODE_BLOCK = 6;
    internal const uint MD_NODE_THEMATIC_BREAK = 7;
    internal const uint MD_NODE_TABLE = 8;
    internal const uint MD_NODE_TABLE_ROW = 9;
    internal const uint MD_NODE_TABLE_CELL = 10;
    internal const uint MD_NODE_FOOTNOTE_DEFINITION = 11;
    internal const uint MD_NODE_ALERT = 12;
    internal const uint MD_NODE_UNSUPPORTED_BLOCK = 13;
    internal const uint MD_NODE_TEXT = 14;
    internal const uint MD_NODE_EMPHASIS = 15;
    internal const uint MD_NODE_STRONG = 16;
    internal const uint MD_NODE_STRIKETHROUGH = 17;
    internal const uint MD_NODE_CODE_SPAN = 18;
    internal const uint MD_NODE_SOFT_BREAK = 19;
    internal const uint MD_NODE_HARD_BREAK = 20;
    internal const uint MD_NODE_LINK = 21;
    internal const uint MD_NODE_IMAGE = 22;
    internal const uint MD_NODE_FOOTNOTE_REFERENCE = 23;
    internal const uint MD_NODE_UNSUPPORTED_INLINE = 24;

    // Node flags.
    internal const uint MD_NODE_ORDERED = 1;
    internal const uint MD_NODE_TIGHT = 2;
    internal const uint MD_NODE_HAS_START = 4;
    internal const uint MD_NODE_HAS_CHECKED = 8;
    internal const uint MD_NODE_CHECKED = 16;
    internal const uint MD_NODE_HEADER = 32;
    internal const uint MD_NODE_HAS_INFO = 64;
    internal const uint MD_NODE_HAS_TITLE = 128;

    // Alert kinds.
    internal const uint MD_ALERT_NOTE = 1;
    internal const uint MD_ALERT_TIP = 2;
    internal const uint MD_ALERT_IMPORTANT = 3;
    internal const uint MD_ALERT_WARNING = 4;
    internal const uint MD_ALERT_CAUTION = 5;

    // Table column alignments.
    internal const uint MD_ALIGN_NONE = 0;
    internal const uint MD_ALIGN_LEFT = 1;
    internal const uint MD_ALIGN_CENTER = 2;
    internal const uint MD_ALIGN_RIGHT = 3;

    // Intent target kinds.
    internal const uint MD_TARGET_FRAGMENT = 1;
    internal const uint MD_TARGET_REQUIRES_CAPABILITY = 2;
    internal const uint MD_TARGET_EXTERNAL = 3;
    internal const uint MD_TARGET_DENIED = 4;

    // D-022 ABI size constants.
    internal const uint MD_FIND_MAX_QUERY_BYTES = 4096;
    internal const uint MD_FIND_DEFAULT_MAX_MATCHES = 10000;
    internal const uint MD_FIND_MAX_MATCHES = 1000000;
    internal const uint MD_FIND_CASE_SENSITIVE = 1;
    internal const uint MD_FIND_WHOLE_WORD = 2;
    internal const uint MD_FIND_TRUNCATED = 1;
    internal const uint MD_MATCH_EXACT = 1;
    internal const uint MD_FIND_OPTIONS_SIZE = 24;
    internal const uint MD_FIND_RESULT_SIZE = 24;
    internal const uint MD_FIND_MATCH_SIZE = 24;
    internal const uint MD_MATCH_SEGMENT_SIZE = 24;
    internal const uint MD_READ_OPTIONS_SIZE = 16;
    internal const uint MD_READ_STATS_SIZE = 32;
    internal const uint MD_READ_SECTION_SIZE = 48;
    internal const uint MD_WORKSPACE_PATH_INFO_SIZE = 16;
    internal const uint MD_WORKSPACE_ENTRY_SIZE = 24;
    internal const uint MD_NAME_OPTIONS_SIZE = 24;
    internal const uint MD_NAME_RESULT_SIZE = 56;
    internal const uint MD_NAME_HIT_SIZE = 24;
    internal const uint MD_CONTENT_OPTIONS_SIZE = 48;
    internal const uint MD_CONTENT_PROGRESS_SIZE = 88;
    internal const uint MD_CONTENT_RESULT_SIZE = 112;
    internal const uint MD_CONTENT_HIT_SIZE = 48;

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_str
    {
        public uint offset;
        public uint length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_abi_info
    {
        public uint struct_size;
        public uint abi_version;
        public uint model_versions;
        public uint anchor_version;
        public uint pointer_size;
        public uint max_anchor_bytes;
        public uint session_options_size;
        public uint session_status_size;
        public uint job_result_size;
        public uint snapshot_view_size;
        public uint node_size;
        public uint intent_size;
        public uint outline_entry_size;
        public uint diagnostic_size;
        public uint property_size;
        public uint anchor_resolution_size;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_session_options
    {
        public uint struct_size;
        public uint model_version;
        public ulong max_source_bytes;
        public ulong max_nodes;
        public ulong max_depth;
        public ulong max_events;
        public ulong max_text_bytes;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_session_status_info
    {
        public uint struct_size;
        public uint flags;
        public ulong latest;
        public ulong confirmed;
        public ulong pending;
        public ulong published;
        public ulong failure_generation;
        public uint availability;
        public uint unreadable_reason;
        public uint failure;
        public uint reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_job_result
    {
        public uint struct_size;
        public uint outcome;
        public ulong generation;
        public ulong latest;
        public ulong snapshot;
        public uint failure;
        public uint reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_node
    {
        public uint kind;
        public uint parent;
        public uint first_child;
        public uint child_count;
        public uint span_start;
        public uint span_end;
        public uint flags;
        public uint value;
        public ulong number;
        public md_str id;
        public md_str text;
        public md_str extra;
        public uint aux;
        public uint aux_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_intent
    {
        public md_str id;
        public md_str destination;
        public md_str target;
        public md_str fragment;
        public uint span_start;
        public uint span_end;
        public uint definition_start;
        public uint definition_end;
        public uint source_kind;
        public uint target_kind;
        public uint denial;
        public uint flags;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_outline_entry
    {
        public uint node;
        public uint level;
        public md_str title;
        public md_str anchor;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_diagnostic
    {
        public md_str code;
        public md_str message;
        public uint severity;
        public uint flags;
        public uint span_start;
        public uint span_end;
        public uint node;
        public uint reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_property
    {
        public md_str key;
        public uint kind;
        public uint boolean;
        public long integer;
        public md_str @string;
        public uint first_string;
        public uint string_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_snapshot_view
    {
        public uint struct_size;
        public uint model_version;
        public ulong generation;
        public ulong source_length;
        public ulong strings_length;
        public byte* source;
        public byte* strings;
        public md_node* nodes;
        public uint* roots;
        public uint* children;
        public uint* alignments;
        public md_intent* links;
        public md_intent* resources;
        public md_outline_entry* outline;
        public md_diagnostic* diagnostics;
        public md_property* properties;
        public md_str* string_list;
        public uint node_count;
        public uint root_count;
        public uint child_count;
        public uint alignment_count;
        public uint link_count;
        public uint resource_count;
        public uint outline_count;
        public uint diagnostic_count;
        public uint property_count;
        public uint string_list_count;
        public uint capabilities;
        public uint flags;
        public uint front_matter_start;
        public uint front_matter_end;
        public md_str document_id;
        public md_str revision_id;
        public md_str source_sha256;
        public md_str title;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_anchor_resolution
    {
        public uint struct_size;
        public uint quality;
        public uint node;
        public uint reserved;
        public ulong offset;
        public ulong source_offset;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_find_options
    {
        public uint struct_size;
        public uint flags;
        public uint max_matches;
        public uint match_size;
        public uint segment_size;
        public uint reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_find_result
    {
        public uint struct_size;
        public uint flags;
        public ulong match_count;
        public ulong segment_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_find_match
    {
        public uint unit;
        public uint flags;
        public uint span_start;
        public uint span_end;
        public uint first_segment;
        public uint segment_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_match_segment
    {
        public uint node;
        public uint flags;
        public uint text_start;
        public uint text_end;
        public uint span_start;
        public uint span_end;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_read_options
    {
        public uint struct_size;
        public uint flags;
        public uint words_per_minute;
        public uint section_size;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_read_stats
    {
        public uint struct_size;
        public uint words_per_minute;
        public ulong words;
        public ulong seconds;
        public ulong section_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_read_section
    {
        public uint heading;
        public uint outline_index;
        public uint level;
        public uint reserved;
        public ulong words;
        public ulong seconds;
        public ulong cumulative_words;
        public ulong cumulative_seconds;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_workspace_path_info
    {
        public uint struct_size;
        public uint error;
        public uint flags;
        public uint name_start;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_workspace_entry
    {
        public ulong id;
        public ulong path_offset;
        public uint path_length;
        public uint reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_name_options
    {
        public uint struct_size;
        public uint flags;
        public uint max_results;
        public uint max_entries;
        public uint entry_size;
        public uint hit_size;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_name_result
    {
        public uint struct_size;
        public uint flags;
        public ulong hit_count;
        public ulong total_hits;
        public ulong entries_examined;
        public ulong skipped_hidden;
        public ulong skipped_not_markdown;
        public ulong skipped_invalid;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_name_hit
    {
        public ulong id;
        public uint entry;
        public uint rank;
        public uint start;
        public uint end;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_content_options
    {
        public uint struct_size;
        public uint flags;
        public ulong max_files;
        public ulong max_total_bytes;
        public uint max_file_bytes;
        public uint max_matches_per_file;
        public uint max_total_matches;
        public uint snippet_context;
        public uint hit_size;
        public uint reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_content_progress
    {
        public uint struct_size;
        public uint stopped;
        public ulong files_read;
        public ulong files_matched;
        public ulong bytes_read;
        public ulong matches;
        public ulong files_truncated;
        public ulong skipped_hidden;
        public ulong skipped_not_markdown;
        public ulong skipped_too_large;
        public ulong skipped_invalid_utf8;
        public ulong skipped_parse_failed;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_content_result
    {
        public uint struct_size;
        public uint outcome;
        public uint reason;
        public uint flags;
        public ulong hit_count;
        public ulong text_length;
        public ulong anchor_length;
        public ulong source_length;
        public fixed byte source_sha256[64];
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct md_content_hit
    {
        public uint unit;
        public uint flags;
        public uint span_start;
        public uint span_end;
        public ulong line;
        public uint text_offset;
        public uint text_length;
        public uint match_start;
        public uint match_end;
        public uint anchor_offset;
        public uint anchor_length;
    }

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_abi_negotiate(uint abi_version, md_abi_info* info);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern byte* md_status_name(uint status);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_live_handles(ulong* sessions, ulong* jobs, ulong* snapshots);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_create(
        byte* document_id,
        nuint document_id_length,
        md_session_options* options,
        ulong* out_session);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_release(ulong session);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_submit(
        ulong session,
        byte* bytes,
        nuint length,
        byte* token,
        nuint token_length,
        ulong* out_job,
        ulong* out_generation);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_external_change(
        ulong session,
        uint change,
        uint reason,
        byte* token,
        nuint token_length,
        uint* out_disposition,
        ulong* out_generation);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_cancel(ulong session, uint* out_cancelled);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_get_status(
        ulong session,
        md_session_status_info* out_status);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_session_current(ulong session, ulong* out_snapshot);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_job_run(ulong job, md_job_result* out_result);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_job_cancel(ulong job);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_job_release(ulong job);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_snapshot_get_view(ulong snapshot, md_snapshot_view* out_view);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_snapshot_find(
        ulong snapshot,
        byte* query,
        nuint query_length,
        md_find_options* options,
        md_find_match* matches,
        nuint match_capacity,
        md_match_segment* segments,
        nuint segment_capacity,
        md_find_result* out_result);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    internal static extern uint md_snapshot_release(ulong snapshot);
}
