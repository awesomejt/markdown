#ifndef FEAS008_H
#define FEAS008_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Disposable FEAS-008 ABI v1 over the spike block model, for the Windows C#
   and C++/WinRT hosts. Every pointer in f8_doc, and every borrowed f8_str, is
   immutable and valid until f8_free_v1(handle). Owned strings and match arrays
   are released with their own free calls. Handles are not validated beyond a
   null check. All text is UTF-8; offsets are byte offsets into block text.
   No paths, UI values or parser lifetimes cross this API. CORE-002 replaces it. */
enum f8_status { F8_OK = 0, F8_BAD_ARGUMENT = 1, F8_INVALID_UTF8 = 2, F8_PANIC = 3 };
enum f8_kind { F8_HEADING = 0, F8_PARAGRAPH, F8_LIST_ITEM, F8_CODE, F8_TABLE_ROW,
  F8_RULE, F8_HTML, F8_MATH, F8_IMAGE };
enum f8_image { F8_IMAGE_NONE = 0, F8_IMAGE_LOCAL = 1, F8_IMAGE_BLOCKED = 2 };
enum f8_intent { F8_LINK_EXTERNAL = 0, F8_LINK_FRAGMENT, F8_LINK_MISSING_FRAGMENT,
  F8_LINK_LOCAL_DOCUMENT, F8_LINK_BLOCKED };
#define F8_STYLE_BOLD 1
#define F8_STYLE_ITALIC 2
#define F8_STYLE_CODE 4
#define F8_STYLE_STRIKE 8
#define F8_STYLE_MARKER 16
#define F8_NONE 0xffffffffu

typedef struct { const uint8_t* ptr; uint64_t len; } f8_str;
typedef struct { uint32_t block, start, end; } f8_item;
/* link indexes f8_doc.links, or F8_NONE. */
typedef struct { uint32_t start, end, link, style; } f8_run;
/* image is the normalized relative path (F8_IMAGE_LOCAL) or the refusal
   reason (F8_IMAGE_BLOCKED). The host still checks containment. */
typedef struct {
  uint32_t kind, level, indent, quote;
  f8_str text, sep_before, slug, image;
  uint32_t image_kind, run_count;
  uint64_t run_offset, source_start, source_end;
} f8_block;
typedef struct { uint32_t block, level; f8_str title; } f8_outline;
/* target_block is set for F8_LINK_FRAGMENT; detail is the slug, normalized
   path or refusal reason. */
typedef struct { uint32_t intent, target_block; f8_str dest, detail; } f8_link;
typedef struct { uint32_t block, start, end; } f8_match;
typedef struct {
  uint32_t version, struct_bytes;
  uint64_t handle;
  const f8_block* blocks; uint64_t block_count;
  const f8_run* runs; uint64_t run_count;
  const f8_item* items; uint64_t item_count;
  const uint32_t* first_item; /* block_count entries */
  const f8_outline* outline; uint64_t outline_count;
  const f8_link* links; uint64_t link_count;
  f8_str source;
  uint64_t parse_ns;
} f8_doc;

/* fragment_bytes 0 selects the default (1024). */
uint32_t f8_open_v1(const uint8_t* utf8, uint64_t len, uint64_t fragment_bytes, f8_doc* out);
uint32_t f8_free_v1(uint64_t handle);
/* Rendered text between two positions, with block separators. Owned. */
uint32_t f8_text_between_v1(uint64_t handle, uint32_t a_block, uint32_t a_offset,
  uint32_t b_block, uint32_t b_offset, f8_str* out);
/* Markdown source of the covered blocks; the whole source for a whole-document range. Borrowed. */
uint32_t f8_source_between_v1(uint64_t handle, uint32_t a_block, uint32_t a_offset,
  uint32_t b_block, uint32_t b_offset, f8_str* out);
uint64_t f8_plain_offset_v1(uint64_t handle, uint32_t block, uint32_t offset);
uint32_t f8_item_for_v1(uint64_t handle, uint32_t block, uint32_t offset);
/* ASCII case-insensitive search. Owned array. */
uint32_t f8_find_v1(uint64_t handle, const uint8_t* query, uint64_t len,
  const f8_match** out, uint64_t* count);
/* HTML for the WebView baseline with the same inert-HTML, link and image policy. Owned. */
uint32_t f8_html_v1(const uint8_t* utf8, uint64_t len, f8_str* out);
void f8_str_free_v1(f8_str value);
void f8_matches_free_v1(const f8_match* matches, uint64_t count);
#ifdef __cplusplus
}
#endif
#endif
