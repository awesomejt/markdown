/*
 * Current-host package smoke for the markdown_core C ABI. Compiled against
 * core/include/markdown_core.h and linked to the built shared library by
 * tests/c_abi.rs. Contains no Rust: it consumes the library the way a native
 * host would.
 *
 *   smoke layout   print sizeof/offsetof for every ABI struct field
 *   smoke run      exercise negotiation, sessions, bulk views, anchors and release
 *
 * SPDX-License-Identifier: Apache-2.0
 */
#include "markdown_core.h"

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
    X(md_anchor_resolution, offset) X(md_anchor_resolution, source_offset)

#define STRUCTS(X)                                                                     \
    X(md_str) X(md_abi_info) X(md_session_options) X(md_session_status_info)           \
    X(md_job_result) X(md_node) X(md_intent) X(md_outline_entry) X(md_diagnostic)      \
    X(md_property) X(md_snapshot_view) X(md_anchor_resolution)

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

static int run(void) {
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
    CHECK(info.model_versions & (1u << MD_MODEL_V1));
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
    if (argc == 2 && strcmp(argv[1], "run") == 0) return run();
    fprintf(stderr, "usage: smoke layout|run\n");
    return 2;
}
