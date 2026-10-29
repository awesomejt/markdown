/*
 * Current-host package smoke for the markdown_core C ABI. Compiled against
 * core/include/markdown_core.h and linked to the built shared library by
 * tests/c_abi.rs. Contains no Rust: it consumes the library the way a native
 * host would.
 *
 *   smoke layout   print sizeof/offsetof for every ABI struct field
 *   smoke run MASK exercise negotiation, sessions, bulk views, find, reading
 *                  statistics, workspace search, anchors and release;
 *                  MASK is the model_versions bitmask the linked build must report
 *
 * SPDX-License-Identifier: Apache-2.0
 */
/* dladdr/Dl_info are GNU extensions in glibc's <dlfcn.h>; macOS declares them
   unconditionally under -std=c11. Define before any system header. */
#define _GNU_SOURCE 1
#include "markdown_core.h"

#include <dlfcn.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define FIELDS(X)                                                                      \
    X(md_str, offset) X(md_str, length)                                                \
    X(md_abi_info, struct_size) X(md_abi_info, abi_version) X(md_abi_info, model_versions) \
    X(md_abi_info, anchor_version) X(md_abi_info, pointer_size)                        \
    X(md_abi_info, max_anchor_bytes) X(md_abi_info, session_options_size)              \
    X(md_abi_info, session_status_size) X(md_abi_info, job_result_size)                \
    X(md_abi_info, snapshot_view_size) X(md_abi_info, node_size)                       \
    X(md_abi_info, intent_size) X(md_abi_info, outline_entry_size)                     \
    X(md_abi_info, diagnostic_size) X(md_abi_info, property_size)                      \
    X(md_abi_info, anchor_resolution_size)                                             \
    X(md_session_options, struct_size) X(md_session_options, model_version)            \
    X(md_session_options, max_source_bytes) X(md_session_options, max_nodes)           \
    X(md_session_options, max_depth) X(md_session_options, max_events)                 \
    X(md_session_options, max_text_bytes)                                              \
    X(md_session_status_info, struct_size) X(md_session_status_info, flags)            \
    X(md_session_status_info, latest) X(md_session_status_info, confirmed)             \
    X(md_session_status_info, pending) X(md_session_status_info, published)            \
    X(md_session_status_info, failure_generation)                                      \
    X(md_session_status_info, availability) X(md_session_status_info, unreadable_reason) \
    X(md_session_status_info, failure) X(md_session_status_info, reserved)             \
    X(md_job_result, struct_size) X(md_job_result, outcome) X(md_job_result, generation) \
    X(md_job_result, latest) X(md_job_result, snapshot) X(md_job_result, failure)      \
    X(md_job_result, reserved)                                                         \
    X(md_node, kind) X(md_node, parent) X(md_node, first_child) X(md_node, child_count) \
    X(md_node, span_start) X(md_node, span_end) X(md_node, flags) X(md_node, value)    \
    X(md_node, number) X(md_node, id) X(md_node, text) X(md_node, extra)               \
    X(md_node, aux) X(md_node, aux_count)                                              \
    X(md_intent, id) X(md_intent, destination) X(md_intent, target) X(md_intent, fragment) \
    X(md_intent, span_start) X(md_intent, span_end) X(md_intent, definition_start)     \
    X(md_intent, definition_end) X(md_intent, source_kind) X(md_intent, target_kind)   \
    X(md_intent, denial) X(md_intent, flags)                                           \
    X(md_outline_entry, node) X(md_outline_entry, level) X(md_outline_entry, title)    \
    X(md_outline_entry, anchor)                                                        \
    X(md_diagnostic, code) X(md_diagnostic, message) X(md_diagnostic, severity)        \
    X(md_diagnostic, flags) X(md_diagnostic, span_start) X(md_diagnostic, span_end)    \
    X(md_diagnostic, node) X(md_diagnostic, reserved)                                  \
    X(md_property, key) X(md_property, kind) X(md_property, boolean)                   \
    X(md_property, integer) X(md_property, string) X(md_property, first_string)        \
    X(md_property, string_count)                                                       \
    X(md_snapshot_view, struct_size) X(md_snapshot_view, model_version)                \
    X(md_snapshot_view, generation) X(md_snapshot_view, source_length)                 \
    X(md_snapshot_view, strings_length) X(md_snapshot_view, source)                    \
    X(md_snapshot_view, strings) X(md_snapshot_view, nodes) X(md_snapshot_view, roots) \
    X(md_snapshot_view, children) X(md_snapshot_view, alignments)                      \
    X(md_snapshot_view, links) X(md_snapshot_view, resources)                          \
    X(md_snapshot_view, outline) X(md_snapshot_view, diagnostics)                      \
    X(md_snapshot_view, properties) X(md_snapshot_view, string_list)                   \
    X(md_snapshot_view, node_count) X(md_snapshot_view, root_count)                    \
    X(md_snapshot_view, child_count) X(md_snapshot_view, alignment_count)              \
    X(md_snapshot_view, link_count) X(md_snapshot_view, resource_count)                \
    X(md_snapshot_view, outline_count) X(md_snapshot_view, diagnostic_count)           \
    X(md_snapshot_view, property_count) X(md_snapshot_view, string_list_count)         \
    X(md_snapshot_view, capabilities) X(md_snapshot_view, flags)                       \
    X(md_snapshot_view, front_matter_start) X(md_snapshot_view, front_matter_end)      \
    X(md_snapshot_view, document_id) X(md_snapshot_view, revision_id)                  \
    X(md_snapshot_view, source_sha256) X(md_snapshot_view, title)                      \
    X(md_anchor_resolution, struct_size) X(md_anchor_resolution, quality)              \
    X(md_anchor_resolution, node) X(md_anchor_resolution, reserved)                    \
    X(md_anchor_resolution, offset) X(md_anchor_resolution, source_offset)             \
    X(md_find_options, struct_size) X(md_find_options, flags)                          \
    X(md_find_options, max_matches) X(md_find_options, match_size)                     \
    X(md_find_options, segment_size) X(md_find_options, reserved)                      \
    X(md_find_result, struct_size) X(md_find_result, flags)                            \
    X(md_find_result, match_count) X(md_find_result, segment_count)                    \
    X(md_find_match, unit) X(md_find_match, flags) X(md_find_match, span_start)        \
    X(md_find_match, span_end) X(md_find_match, first_segment)                         \
    X(md_find_match, segment_count)                                                    \
    X(md_match_segment, node) X(md_match_segment, flags) X(md_match_segment, text_start) \
    X(md_match_segment, text_end) X(md_match_segment, span_start)                      \
    X(md_match_segment, span_end)                                                      \
    X(md_read_options, struct_size) X(md_read_options, flags)                          \
    X(md_read_options, words_per_minute) X(md_read_options, section_size)              \
    X(md_read_stats, struct_size) X(md_read_stats, words_per_minute)                   \
    X(md_read_stats, words) X(md_read_stats, seconds) X(md_read_stats, section_count)  \
    X(md_read_section, heading) X(md_read_section, outline_index)                      \
    X(md_read_section, level) X(md_read_section, reserved) X(md_read_section, words)   \
    X(md_read_section, seconds) X(md_read_section, cumulative_words)                   \
    X(md_read_section, cumulative_seconds)                                             \
    X(md_workspace_path_info, struct_size) X(md_workspace_path_info, error)            \
    X(md_workspace_path_info, flags) X(md_workspace_path_info, name_start)             \
    X(md_workspace_entry, id) X(md_workspace_entry, path_offset)                       \
    X(md_workspace_entry, path_length) X(md_workspace_entry, reserved)                 \
    X(md_name_options, struct_size) X(md_name_options, flags)                          \
    X(md_name_options, max_results) X(md_name_options, max_entries)                    \
    X(md_name_options, entry_size) X(md_name_options, hit_size)                        \
    X(md_name_result, struct_size) X(md_name_result, flags)                            \
    X(md_name_result, hit_count) X(md_name_result, total_hits)                         \
    X(md_name_result, entries_examined) X(md_name_result, skipped_hidden)              \
    X(md_name_result, skipped_not_markdown) X(md_name_result, skipped_invalid)         \
    X(md_name_hit, id) X(md_name_hit, entry) X(md_name_hit, rank)                      \
    X(md_name_hit, start) X(md_name_hit, end)                                          \
    X(md_content_options, struct_size) X(md_content_options, flags)                    \
    X(md_content_options, max_files) X(md_content_options, max_total_bytes)            \
    X(md_content_options, max_file_bytes) X(md_content_options, max_matches_per_file)  \
    X(md_content_options, max_total_matches) X(md_content_options, snippet_context)    \
    X(md_content_options, hit_size) X(md_content_options, reserved)                    \
    X(md_content_progress, struct_size) X(md_content_progress, stopped)                \
    X(md_content_progress, files_read) X(md_content_progress, files_matched)           \
    X(md_content_progress, bytes_read) X(md_content_progress, matches)                 \
    X(md_content_progress, files_truncated) X(md_content_progress, skipped_hidden)     \
    X(md_content_progress, skipped_not_markdown)                                       \
    X(md_content_progress, skipped_too_large)                                          \
    X(md_content_progress, skipped_invalid_utf8)                                       \
    X(md_content_progress, skipped_parse_failed)                                       \
    X(md_content_result, struct_size) X(md_content_result, outcome)                    \
    X(md_content_result, reason) X(md_content_result, flags)                           \
    X(md_content_result, hit_count) X(md_content_result, text_length)                  \
    X(md_content_result, anchor_length) X(md_content_result, source_length)            \
    X(md_content_result, source_sha256)                                                \
    X(md_content_hit, unit) X(md_content_hit, flags) X(md_content_hit, span_start)     \
    X(md_content_hit, span_end) X(md_content_hit, line) X(md_content_hit, text_offset) \
    X(md_content_hit, text_length) X(md_content_hit, match_start)                      \
    X(md_content_hit, match_end) X(md_content_hit, anchor_offset)                      \
    X(md_content_hit, anchor_length)

#define STRUCTS(X)                                                                     \
    X(md_str) X(md_abi_info) X(md_session_options) X(md_session_status_info)           \
    X(md_job_result) X(md_node) X(md_intent) X(md_outline_entry) X(md_diagnostic)      \
    X(md_property) X(md_snapshot_view) X(md_anchor_resolution) X(md_find_options)      \
    X(md_find_result) X(md_find_match) X(md_match_segment) X(md_read_options)          \
    X(md_read_stats) X(md_read_section) X(md_workspace_path_info) X(md_workspace_entry) \
    X(md_name_options) X(md_name_result) X(md_name_hit) X(md_content_options)          \
    X(md_content_progress) X(md_content_result) X(md_content_hit)

/* D-022: each addition's size constant is part of the ABI. */
_Static_assert(sizeof(md_find_options) == MD_FIND_OPTIONS_SIZE, "md_find_options");
_Static_assert(sizeof(md_find_result) == MD_FIND_RESULT_SIZE, "md_find_result");
_Static_assert(sizeof(md_find_match) == MD_FIND_MATCH_SIZE, "md_find_match");
_Static_assert(sizeof(md_match_segment) == MD_MATCH_SEGMENT_SIZE, "md_match_segment");
_Static_assert(sizeof(md_read_options) == MD_READ_OPTIONS_SIZE, "md_read_options");
_Static_assert(sizeof(md_read_stats) == MD_READ_STATS_SIZE, "md_read_stats");
_Static_assert(sizeof(md_read_section) == MD_READ_SECTION_SIZE, "md_read_section");
_Static_assert(sizeof(md_workspace_path_info) == MD_WORKSPACE_PATH_INFO_SIZE, "md_workspace_path_info");
_Static_assert(sizeof(md_workspace_entry) == MD_WORKSPACE_ENTRY_SIZE, "md_workspace_entry");
_Static_assert(sizeof(md_name_options) == MD_NAME_OPTIONS_SIZE, "md_name_options");
_Static_assert(sizeof(md_name_result) == MD_NAME_RESULT_SIZE, "md_name_result");
_Static_assert(sizeof(md_name_hit) == MD_NAME_HIT_SIZE, "md_name_hit");
_Static_assert(sizeof(md_content_options) == MD_CONTENT_OPTIONS_SIZE, "md_content_options");
_Static_assert(sizeof(md_content_progress) == MD_CONTENT_PROGRESS_SIZE, "md_content_progress");
_Static_assert(sizeof(md_content_result) == MD_CONTENT_RESULT_SIZE, "md_content_result");
_Static_assert(sizeof(md_content_hit) == MD_CONTENT_HIT_SIZE, "md_content_hit");

static int layout(void) {
#define PRINT_SIZE(type) printf("size %s %zu\n", #type, sizeof(type));
#define PRINT_FIELD(type, field) printf("offset %s.%s %zu\n", #type, #field, offsetof(type, field));
    STRUCTS(PRINT_SIZE)
    FIELDS(PRINT_FIELD)
    return 0;
}

static int failures = 0;
#define CHECK(cond)                                                                    \
    do {                                                                               \
        if (!(cond)) {                                                                 \
            fprintf(stderr, "check failed at line %d: %s\n", __LINE__, #cond);         \
            failures++;                                                                \
        }                                                                              \
    } while (0)
#define CHECK_STATUS(call, expected)                                                   \
    do {                                                                               \
        md_status actual_ = (call);                                                    \
        if (actual_ != (md_status)(expected)) {                                        \
            fprintf(stderr, "line %d: %s returned %s, expected %s\n", __LINE__, #call, \
                    md_status_name(actual_), md_status_name(expected));                \
            failures++;                                                                \
        }                                                                              \
    } while (0)

/* A host-owned copy of one heading, made before the snapshot is released. */
typedef struct {
    uint32_t level;
    char title[64];
} heading_copy;

static md_snapshot publish(md_session session, const char *text, uint64_t *generation) {
    md_job job = 0;
    md_job_result result;
    memset(&result, 0, sizeof result);
    result.struct_size = sizeof result;
    CHECK_STATUS(md_session_submit(session, (const uint8_t *)text, strlen(text), NULL, 0, &job,
                                   generation),
                 MD_OK);
    CHECK_STATUS(md_job_run(job, &result), MD_OK);
    CHECK(result.outcome == MD_OUTCOME_PUBLISHED);
    CHECK_STATUS(md_job_release(job), MD_OK);
    return result.snapshot;
}

static md_snapshot_view get_view(md_snapshot snapshot) {
    md_snapshot_view view;
    memset(&view, 0, sizeof view);
    view.struct_size = sizeof view;
    CHECK_STATUS(md_snapshot_get_view(snapshot, &view), MD_OK);
    return view;
}

static size_t copy_headings(const md_snapshot_view *view, heading_copy *out, size_t capacity) {
    size_t count = 0;
    for (uint32_t i = 0; i < view->outline_count && count < capacity; i++) {
        const md_outline_entry *entry = &view->outline[i];
        CHECK(entry->node < view->node_count);
        CHECK(view->nodes[entry->node].kind == MD_NODE_HEADING);
        CHECK((uint64_t)entry->title.offset + entry->title.length <= view->strings_length);
        size_t n = entry->title.length < 63 ? entry->title.length : 63;
        memcpy(out[count].title, view->strings + entry->title.offset, n);
        out[count].title[n] = '\0';
        out[count].level = entry->level;
        count++;
    }
    return count;
}

static void check_view(const md_snapshot_view *view) {
    for (uint32_t i = 0; i < view->node_count; i++) {
        const md_node *node = &view->nodes[i];
        CHECK(node->kind >= MD_NODE_PARAGRAPH && node->kind <= MD_NODE_UNSUPPORTED_INLINE);
        CHECK(node->span_start <= node->span_end && node->span_end <= view->source_length);
        CHECK((uint64_t)node->first_child + node->child_count <= view->child_count);
        for (uint32_t c = 0; c < node->child_count; c++) {
            CHECK(view->nodes[view->children[node->first_child + c]].parent == i);
        }
        if (node->kind == MD_NODE_LINK) CHECK(node->aux < view->link_count);
        if (node->kind == MD_NODE_IMAGE) CHECK(node->aux < view->resource_count);
    }
    for (uint32_t r = 0; r < view->root_count; r++) {
        CHECK(view->nodes[view->roots[r]].parent == MD_NONE);
    }
    CHECK(view->source_sha256.length == 64);
}

/* Find "text and a" (across emphasis and link text) with the two-call
   protocol, then check each exact segment against the view, as a presenter
   highlighting display text would. */
static void check_find(md_snapshot snapshot, const md_snapshot_view *view) {
    const char *query = "TEXT AND A";
    md_find_options options;
    memset(&options, 0, sizeof options);
    options.struct_size = sizeof options;
    options.match_size = sizeof(md_find_match);
    options.segment_size = sizeof(md_match_segment);
    md_find_result result;
    memset(&result, 0, sizeof result);
    result.struct_size = sizeof result;
    CHECK_STATUS(md_snapshot_find(snapshot, (const uint8_t *)query, strlen(query), &options, NULL,
                                  0, NULL, 0, &result),
                 MD_ERR_BUFFER_TOO_SMALL);
    CHECK(result.match_count == 1 && result.segment_count == 3 && result.flags == 0);
    md_find_match matches[1];
    md_match_segment segments[3];
    CHECK_STATUS(md_snapshot_find(snapshot, (const uint8_t *)query, strlen(query), &options,
                                  matches, 1, segments, 3, &result),
                 MD_OK);
    const char *covered = "text* and [a";
    CHECK(matches[0].flags == MD_MATCH_EXACT && matches[0].first_segment == 0 &&
          matches[0].segment_count == 3);
    CHECK(matches[0].span_end - matches[0].span_start == strlen(covered));
    CHECK(memcmp(view->source + matches[0].span_start, covered, strlen(covered)) == 0);
    CHECK(view->nodes[matches[0].unit].kind == MD_NODE_PARAGRAPH);
    for (uint32_t i = 0; i < 3; i++) {
        const md_match_segment *s = &segments[i];
        const md_node *node = &view->nodes[s->node];
        CHECK(node->kind == MD_NODE_TEXT && s->flags == MD_MATCH_EXACT);
        CHECK(s->text_end <= node->text.length && s->span_end <= node->span_end);
        CHECK(s->text_end - s->text_start == s->span_end - s->span_start);
        CHECK(memcmp(view->strings + node->text.offset + s->text_start,
                     view->source + s->span_start, s->span_end - s->span_start) == 0);
    }
    options.flags = MD_FIND_CASE_SENSITIVE;
    CHECK_STATUS(md_snapshot_find(snapshot, (const uint8_t *)query, strlen(query), &options, NULL,
                                  0, NULL, 0, &result),
                 MD_OK);
    CHECK(result.match_count == 0);
    options.match_size = sizeof(md_find_match) - 4;
    CHECK_STATUS(md_snapshot_find(snapshot, (const uint8_t *)query, strlen(query), &options, NULL,
                                  0, NULL, 0, &result),
                 MD_ERR_STRUCT_SIZE);
}

/* Reading time: the intro plus one section per outline entry. */
static void check_read_stats(md_snapshot snapshot, const md_snapshot_view *view) {
    md_read_options options;
    memset(&options, 0, sizeof options);
    options.struct_size = sizeof options;
    options.section_size = sizeof(md_read_section);
    md_read_stats stats;
    memset(&stats, 0, sizeof stats);
    stats.struct_size = sizeof stats;
    md_read_section sections[4];
    CHECK_STATUS(md_snapshot_read_stats(snapshot, &options, NULL, 0, &stats),
                 MD_ERR_BUFFER_TOO_SMALL);
    CHECK(stats.section_count == (uint64_t)view->outline_count + 1);
    CHECK_STATUS(md_snapshot_read_stats(snapshot, &options, sections, 4, &stats), MD_OK);
    CHECK(stats.words_per_minute == MD_READ_DEFAULT_WPM && stats.section_count == 3);
    /* "Some text and a link" plus four table cells; the image alt never counts. */
    CHECK(stats.words == 9 && stats.seconds == 3);
    CHECK(sections[0].heading == MD_NONE && sections[0].level == 0 && sections[0].words == 0);
    CHECK(sections[1].level == 1 && sections[1].words == 5 && sections[1].cumulative_words == 9);
    CHECK(sections[2].level == 2 && sections[2].words == 4);
    for (uint32_t i = 1; i < 3; i++) {
        CHECK(sections[i].outline_index == i - 1);
        CHECK(view->outline[i - 1].node == sections[i].heading);
    }
    options.words_per_minute = MD_READ_MAX_WPM + 1;
    CHECK_STATUS(md_snapshot_read_stats(snapshot, &options, sections, 4, &stats),
                 MD_ERR_INVALID_ARGUMENT);
    CHECK(stats.words == 0 && stats.section_count == 0);
}

/* A host-side workspace. The adapter enumerates files under its own
   permissions, keeps each file's capability behind an opaque id and passes
   only display paths and bytes to the core. */
typedef struct {
    const char *path;
    const char *content;
} workspace_file;

static const workspace_file workspace_files[] = {
    {"docs/Notes.md", "# Notes\n\nFirst line.\nThe *alpha* plan.\n"},
    {".git/notes.md", "alpha hidden\n"},
    {"notes.txt", "alpha text\n"},
    {"a/b/alpha-notes.md", "No match here.\n"},
    {"bad//x.md", "alpha\n"},
    {"zeta.md", "alpha and alpha again\n"},
};
#define WORKSPACE_FILES (sizeof workspace_files / sizeof workspace_files[0])
#define WORKSPACE_HITS 8

static md_workspace_path_info classify(const char *path) {
    md_workspace_path_info info;
    memset(&info, 0, sizeof info);
    info.struct_size = sizeof info;
    CHECK_STATUS(md_workspace_classify_path((const uint8_t *)path, strlen(path), &info), MD_OK);
    return info;
}

static void check_workspace_names(void) {
    md_workspace_path_info info = classify("docs/Notes.md");
    CHECK(info.error == MD_PATH_VALID && info.flags == MD_PATH_MARKDOWN && info.name_start == 5);
    info = classify(".git/notes.md");
    CHECK(info.error == MD_PATH_VALID && info.flags == (MD_PATH_MARKDOWN | MD_PATH_HIDDEN));
    info = classify("bad//x.md");
    CHECK(info.error == MD_PATH_EMPTY_COMPONENT && info.flags == 0);

    uint8_t paths[256];
    md_workspace_entry entries[WORKSPACE_FILES];
    size_t used = 0;
    for (size_t i = 0; i < WORKSPACE_FILES; i++) {
        size_t n = strlen(workspace_files[i].path);
        memcpy(paths + used, workspace_files[i].path, n);
        entries[i].id = 100 + i;
        entries[i].path_offset = used;
        entries[i].path_length = (uint32_t)n;
        entries[i].reserved = 0;
        used += n;
    }
    md_name_options options;
    memset(&options, 0, sizeof options);
    options.struct_size = sizeof options;
    options.entry_size = sizeof(md_workspace_entry);
    options.hit_size = sizeof(md_name_hit);
    md_name_result result;
    memset(&result, 0, sizeof result);
    result.struct_size = sizeof result;
    md_name_hit hits[WORKSPACE_FILES];
    const char *query = "NOTES";
    CHECK_STATUS(md_workspace_find_names(paths, used, entries, WORKSPACE_FILES,
                                         (const uint8_t *)query, strlen(query), &options, hits,
                                         WORKSPACE_FILES, &result),
                 MD_OK);
    /* The hidden, non-Markdown and invalid entries are skipped. */
    CHECK(result.hit_count == 2 && result.total_hits == 2 && result.flags == 0);
    CHECK(result.entries_examined == WORKSPACE_FILES && result.skipped_hidden == 1 &&
          result.skipped_not_markdown == 1 && result.skipped_invalid == 1);
    CHECK(hits[0].id == 100 && hits[0].entry == 0 && hits[0].rank == MD_NAME_RANK_PREFIX);
    CHECK(hits[0].start == 5 && hits[0].end == 10);
    CHECK(hits[1].id == 103 && hits[1].entry == 3 && hits[1].rank == MD_NAME_RANK_NAME);
    CHECK(memcmp(paths + entries[3].path_offset + hits[1].start, "notes", 5) == 0);
    CHECK_STATUS(md_workspace_find_names(paths, used, entries, WORKSPACE_FILES,
                                         (const uint8_t *)query, strlen(query), &options, hits, 1,
                                         &result),
                 MD_ERR_BUFFER_TOO_SMALL);
    CHECK(result.hit_count == 2);
}

/* Admit, read and search file by file with arrays sized from the limits,
   then open the matched file in a session and navigate to the hit. */
static void check_workspace_content(void) {
    const char *query = "alpha";
    md_content_options options;
    memset(&options, 0, sizeof options);
    options.struct_size = sizeof options;
    options.max_matches_per_file = WORKSPACE_HITS;
    options.snippet_context = MD_CONTENT_DEFAULT_SNIPPET_CONTEXT;
    options.hit_size = sizeof(md_content_hit);
    md_content_progress progress;
    memset(&progress, 0, sizeof progress);
    progress.struct_size = sizeof progress;
    static md_content_hit hits[WORKSPACE_HITS];
    static uint8_t text[WORKSPACE_HITS * MD_CONTENT_MAX_SNIPPET_BYTES];
    static uint8_t anchors[WORKSPACE_HITS * MD_ANCHOR_MAX_BYTES];
    md_content_result notes;
    memset(&notes, 0, sizeof notes);
    md_content_hit notes_hit;
    memset(&notes_hit, 0, sizeof notes_hit);
    uint8_t notes_anchor[MD_ANCHOR_MAX_BYTES];
    int searched = 0, skipped = 0;
    uint64_t bytes = 0;
    for (size_t i = 0; i < WORKSPACE_FILES; i++) {
        const workspace_file *file = &workspace_files[i];
        if (classify(file->path).error != MD_PATH_VALID) continue; /* never shown */
        size_t length = strlen(file->content);
        uint32_t outcome = 0, reason = 0;
        CHECK_STATUS(md_workspace_content_admit((const uint8_t *)query, strlen(query), &options,
                                                (const uint8_t *)file->path, strlen(file->path),
                                                length, &progress, &outcome, &reason),
                     MD_OK);
        if (outcome == MD_CONTENT_STOPPED) break;
        if (outcome == MD_CONTENT_SKIPPED) {
            CHECK(reason == MD_SKIP_HIDDEN || reason == MD_SKIP_NOT_MARKDOWN);
            skipped++;
            continue;
        }
        CHECK(outcome == MD_CONTENT_READ && reason == 0);
        md_content_result result;
        memset(&result, 0, sizeof result);
        result.struct_size = sizeof result;
        CHECK_STATUS(md_workspace_content_search(
                         (const uint8_t *)query, strlen(query), &options,
                         (const uint8_t *)file->path, strlen(file->path),
                         (const uint8_t *)file->content, length, &progress, hits, WORKSPACE_HITS,
                         text, sizeof text, anchors, sizeof anchors, &result),
                     MD_OK);
        CHECK(result.outcome == MD_CONTENT_SEARCHED && result.source_length == length);
        searched++;
        bytes += length;
        for (uint64_t h = 0; h < result.hit_count; h++) {
            const md_content_hit *hit = &hits[h];
            CHECK(hit->flags == (MD_HIT_EXACT | MD_HIT_ANCHOR));
            CHECK((uint64_t)hit->text_offset + hit->text_length <= result.text_length);
            CHECK((uint64_t)hit->anchor_offset + hit->anchor_length <= result.anchor_length);
            CHECK(hit->match_end - hit->match_start == 5 && hit->match_end <= hit->text_length);
            CHECK(memcmp(text + hit->text_offset + hit->match_start, "alpha", 5) == 0);
            CHECK(memcmp(file->content + hit->span_start, "alpha", 5) == 0);
        }
        if (i == 0) {
            /* One line of visible text: the soft break is a space and the
               emphasis markup is gone. */
            const char *snippet = "First line. The alpha plan.";
            CHECK(result.hit_count == 1 && hits[0].line == 4);
            CHECK(hits[0].text_length == strlen(snippet) && hits[0].match_start == 16);
            CHECK(memcmp(text + hits[0].text_offset, snippet, strlen(snippet)) == 0);
            notes = result;
            notes_hit = hits[0];
            memcpy(notes_anchor, anchors + hits[0].anchor_offset, hits[0].anchor_length);
        }
    }
    CHECK(searched == 3 && skipped == 2);
    CHECK(progress.stopped == 0 && progress.files_read == 3 && progress.files_matched == 2);
    CHECK(progress.matches == 3 && progress.bytes_read == bytes);
    CHECK(progress.skipped_hidden == 1 && progress.skipped_not_markdown == 1);

    /* Zero selects the default file budget; a larger file is skipped unread. */
    uint32_t outcome = 0, reason = 0;
    md_content_progress sizes;
    memset(&sizes, 0, sizeof sizes);
    sizes.struct_size = sizeof sizes;
    CHECK(options.max_file_bytes == 0);
    CHECK_STATUS(md_workspace_content_admit((const uint8_t *)query, strlen(query), &options,
                                            (const uint8_t *)"big.md", 6,
                                            MD_CONTENT_DEFAULT_MAX_FILE_BYTES + 1u, &sizes,
                                            &outcome, &reason),
                 MD_OK);
    CHECK(outcome == MD_CONTENT_SKIPPED && reason == MD_SKIP_TOO_LARGE);
    CHECK(sizes.skipped_too_large == 1 && sizes.files_read == 0);

    /* Arguments are checked and failures leave progress unchanged. */
    md_content_progress before = progress;
    md_content_result result;
    memset(&result, 0, sizeof result);
    result.struct_size = sizeof result;
    const char *zeta = workspace_files[5].content;
    CHECK_STATUS(md_workspace_content_search((const uint8_t *)" ", 1, &options,
                                             (const uint8_t *)"zeta.md", 7, (const uint8_t *)zeta,
                                             strlen(zeta), &progress, hits, WORKSPACE_HITS, text,
                                             sizeof text, anchors, sizeof anchors, &result),
                 MD_ERR_INVALID_ARGUMENT);
    CHECK(memcmp(&before, &progress, sizeof progress) == 0 && result.outcome == 0);
    CHECK_STATUS(md_workspace_content_search((const uint8_t *)query, strlen(query), &options,
                                             (const uint8_t *)"zeta.md", 7, (const uint8_t *)zeta,
                                             strlen(zeta), &progress, hits, 1, text, sizeof text,
                                             anchors, sizeof anchors, &result),
                 MD_ERR_BUFFER_TOO_SMALL);
    CHECK(result.hit_count == 2 && memcmp(&before, &progress, sizeof progress) == 0);
    /* A spent budget is a permanent stop. */
    options.max_files = 3;
    CHECK_STATUS(md_workspace_content_admit((const uint8_t *)query, strlen(query), &options,
                                            (const uint8_t *)"z.md", 4, 1, &progress, &outcome,
                                            &reason),
                 MD_OK);
    CHECK(outcome == MD_CONTENT_STOPPED && reason == MD_STOP_FILES);
    CHECK(progress.stopped == MD_STOP_FILES);

    /* Open the unchanged file: equal source hashes mean the span and unit
       apply directly, and the anchor resolves exactly. */
    md_session_options session_options;
    memset(&session_options, 0, sizeof session_options);
    session_options.struct_size = sizeof session_options;
    session_options.model_version = MD_MODEL_V1;
    md_session session = 0;
    CHECK_STATUS(md_session_create((const uint8_t *)"notes", 5, &session_options, &session),
                 MD_OK);
    md_snapshot snapshot = publish(session, workspace_files[0].content, NULL);
    md_snapshot_view view = get_view(snapshot);
    CHECK(view.source_length == notes.source_length && view.source_sha256.length == 64);
    CHECK(memcmp(view.strings + view.source_sha256.offset, notes.source_sha256, 64) == 0);
    CHECK(view.nodes[notes_hit.unit].kind == MD_NODE_PARAGRAPH);
    md_anchor_resolution resolution;
    memset(&resolution, 0, sizeof resolution);
    resolution.struct_size = sizeof resolution;
    CHECK_STATUS(md_snapshot_resolve_anchor(snapshot, notes_anchor, notes_hit.anchor_length,
                                            &resolution),
                 MD_OK);
    CHECK(resolution.quality == MD_ANCHOR_EXACT &&
          resolution.source_offset == notes_hit.span_start);
    CHECK_STATUS(md_snapshot_release(snapshot), MD_OK);
    CHECK_STATUS(md_session_release(session), MD_OK);
}

/* Print the file that provides md_abi_negotiate, so the test can prove the
   staged library (not one from a search path) was loaded. ISO C has no
   function-to-object pointer conversion, so copy the representation. */
static int report_library(void) {
    md_status (*symbol)(uint32_t, md_abi_info *) = md_abi_negotiate;
    void *address = NULL;
    memcpy(&address, &symbol, sizeof address);
    Dl_info found;
    if (!dladdr(address, &found) || found.dli_fname == NULL) {
        fprintf(stderr, "dladdr failed\n");
        return 1;
    }
    printf("library %s\n", found.dli_fname);
    return 0;
}

static int run(uint32_t expected_models) {
    if (report_library() != 0) failures++;
    md_abi_info info;
    memset(&info, 0, sizeof info);
    info.struct_size = sizeof info;
    CHECK_STATUS(md_abi_negotiate(MD_ABI_VERSION, &info), MD_OK);
    CHECK(info.abi_version == MD_ABI_VERSION);
    CHECK(info.pointer_size == sizeof(void *));
    CHECK(info.session_options_size == sizeof(md_session_options));
    CHECK(info.session_status_size == sizeof(md_session_status_info));
    CHECK(info.job_result_size == sizeof(md_job_result));
    CHECK(info.snapshot_view_size == sizeof(md_snapshot_view));
    CHECK(info.node_size == sizeof(md_node));
    CHECK(info.intent_size == sizeof(md_intent));
    CHECK(info.outline_entry_size == sizeof(md_outline_entry));
    CHECK(info.diagnostic_size == sizeof(md_diagnostic));
    CHECK(info.property_size == sizeof(md_property));
    CHECK(info.anchor_resolution_size == sizeof(md_anchor_resolution));
    CHECK(info.max_anchor_bytes == MD_ANCHOR_MAX_BYTES);
    /* Proves the linked library is the build under test (feature set). */
    CHECK(info.model_versions == expected_models);
    CHECK_STATUS(md_abi_negotiate(MD_ABI_VERSION + 1, &info), MD_ERR_VERSION_MISMATCH);
    info.struct_size = sizeof info - 4;
    CHECK_STATUS(md_abi_negotiate(MD_ABI_VERSION, &info), MD_ERR_STRUCT_SIZE);
    CHECK_STATUS(md_abi_negotiate(MD_ABI_VERSION, NULL), MD_ERR_INVALID_ARGUMENT);
    CHECK(strcmp(md_status_name(MD_ERR_INVALID_HANDLE), "invalid_handle") == 0);
    CHECK(strcmp(md_status_name(12345), "unknown") == 0);

    for (uint32_t model = MD_MODEL_V1; model <= MD_MODEL_V2; model++) {
        if (!(info.model_versions & (1u << model))) continue;
        md_session_options options;
        memset(&options, 0, sizeof options);
        options.struct_size = sizeof options;
        options.model_version = model;
        md_session session = 0;
        const char *id = "smoke-doc";
        CHECK_STATUS(md_session_create((const uint8_t *)id, strlen(id), &options, &session), MD_OK);
        CHECK(session != 0);

        const char *first = "# Caf\xc3\xa9\n\nSome *text* and [a link](#caf).\n\n"
                            "## Second\n\n| a | b |\n|:-|-:|\n| 1 | 2 |\n\n![img](pic.png)\n";
        uint64_t generation = 0;
        md_snapshot snapshot = publish(session, first, &generation);
        CHECK(generation == 1);
        md_snapshot_view view = get_view(snapshot);
        CHECK(view.model_version == model);
        CHECK(view.generation == 1);
        CHECK(view.source_length == strlen(first));
        CHECK(memcmp(view.source, first, view.source_length) == 0);
        CHECK(view.link_count == 1 && view.resource_count == 1 && view.alignment_count == 2);
        check_view(&view);
        check_find(snapshot, &view);
        check_read_stats(snapshot, &view);

        /* Anchor on "Second" (absolute offset), then copy values and release. */
        const char *second_heading = strstr(first, "Second");
        uint8_t anchor[MD_ANCHOR_MAX_BYTES];
        size_t anchor_length = 0;
        CHECK_STATUS(md_snapshot_anchor_at(snapshot, (uint64_t)(second_heading - first), NULL, 0,
                                           &anchor_length),
                     MD_ERR_BUFFER_TOO_SMALL);
        CHECK(anchor_length > 0 && anchor_length <= sizeof anchor);
        CHECK_STATUS(md_snapshot_anchor_at(snapshot, (uint64_t)(second_heading - first), anchor,
                                           sizeof anchor, &anchor_length),
                     MD_OK);
        heading_copy headings[4];
        size_t heading_count = copy_headings(&view, headings, 4);
        CHECK(heading_count == 2);
        CHECK(headings[0].level == 1 && strcmp(headings[0].title, "Caf\xc3\xa9") == 0);
        CHECK(headings[1].level == 2 && strcmp(headings[1].title, "Second") == 0);
        CHECK_STATUS(md_snapshot_release(snapshot), MD_OK);
        CHECK_STATUS(md_snapshot_release(snapshot), MD_ERR_INVALID_HANDLE);

        /* Next generation: the anchor relocates past the inserted paragraph. */
        const char *edited = "# Caf\xc3\xa9\n\nInserted paragraph.\n\nSome *text* and [a link](#caf).\n\n"
                             "## Second\n\n| a | b |\n|:-|-:|\n| 1 | 2 |\n\n![img](pic.png)\n";
        md_snapshot next = publish(session, edited, &generation);
        CHECK(generation == 2);
        md_anchor_resolution resolution;
        memset(&resolution, 0, sizeof resolution);
        resolution.struct_size = sizeof resolution;
        CHECK_STATUS(md_snapshot_resolve_anchor(next, anchor, anchor_length, &resolution), MD_OK);
        CHECK(resolution.quality == MD_ANCHOR_EXACT);
        CHECK(resolution.source_offset == (uint64_t)(strstr(edited, "Second") - edited));
        md_snapshot_view next_view = get_view(next);
        CHECK(resolution.node < next_view.node_count);
        CHECK(next_view.nodes[resolution.node].kind == MD_NODE_HEADING);
        CHECK_STATUS(md_snapshot_resolve_anchor(next, (const uint8_t *)"{}", 2, &resolution),
                     MD_ERR_ANCHOR_INVALID);

        /* External change notices and status. */
        uint32_t disposition = 0;
        uint64_t change_generation = 0;
        CHECK_STATUS(md_session_external_change(session, MD_CHANGE_DELETED, MD_REASON_NONE, NULL, 0,
                                                &disposition, &change_generation),
                     MD_OK);
        CHECK(disposition == MD_DISPOSITION_RECORDED && change_generation == 3);
        md_session_status_info status;
        memset(&status, 0, sizeof status);
        status.struct_size = sizeof status;
        CHECK_STATUS(md_session_get_status(session, &status), MD_OK);
        CHECK(status.availability == MD_DELETED && status.published == 2);
        CHECK((status.flags & MD_STATUS_IS_CURRENT) == 0);

        /* Cancellation: a cancelled job publishes nothing. */
        md_job job = 0;
        CHECK_STATUS(md_session_submit(session, (const uint8_t *)"# x", 3, NULL, 0, &job, NULL),
                     MD_OK);
        CHECK_STATUS(md_job_cancel(job), MD_OK);
        md_job_result result;
        memset(&result, 0, sizeof result);
        result.struct_size = sizeof result;
        CHECK_STATUS(md_job_run(job, &result), MD_OK);
        CHECK(result.outcome == MD_OUTCOME_CANCELLED && result.snapshot == 0);
        CHECK_STATUS(md_job_run(job, &result), MD_ERR_INVALID_STATE);
        CHECK_STATUS(md_job_release(job), MD_OK);

        /* The snapshot outlives its session. */
        CHECK_STATUS(md_session_release(session), MD_OK);
        CHECK_STATUS(md_session_release(session), MD_ERR_INVALID_HANDLE);
        CHECK(memcmp(next_view.source, edited, next_view.source_length) == 0);
        check_view(&next_view);
        CHECK_STATUS(md_snapshot_release(next), MD_OK);
    }

    check_workspace_names();
    check_workspace_content();

    /* Repeated lifecycle, then every handle count returns to zero. */
    for (int i = 0; i < 200; i++) {
        md_session_options options;
        memset(&options, 0, sizeof options);
        options.struct_size = sizeof options;
        options.model_version = MD_MODEL_V1;
        md_session session = 0;
        CHECK_STATUS(md_session_create((const uint8_t *)"loop", 4, &options, &session), MD_OK);
        md_snapshot snapshot = publish(session, "# loop\n\nbody\n", NULL);
        md_snapshot_view view = get_view(snapshot);
        CHECK(view.node_count == 4);
        CHECK_STATUS(md_session_release(session), MD_OK);
        CHECK_STATUS(md_snapshot_release(snapshot), MD_OK);
    }
    uint64_t sessions = 1, jobs = 1, snapshots = 1;
    CHECK_STATUS(md_live_handles(&sessions, &jobs, &snapshots), MD_OK);
    CHECK(sessions == 0 && jobs == 0 && snapshots == 0);

    if (failures) {
        fprintf(stderr, "c smoke: %d failure(s)\n", failures);
        return 1;
    }
    printf("c smoke ok (abi %u, models 0x%x, pointer %u bytes)\n", info.abi_version,
           info.model_versions, info.pointer_size);
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "layout") == 0) return layout();
    if (argc == 3 && strcmp(argv[1], "run") == 0) {
        return run((uint32_t)strtoul(argv[2], NULL, 0));
    }
    fprintf(stderr, "usage: smoke layout | smoke run MASK\n");
    return 2;
}
