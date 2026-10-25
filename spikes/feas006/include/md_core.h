#ifndef FEAS006_MD_CORE_H
#define FEAS006_MD_CORE_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Experimental ABI v1: native-endian fixed-width POD, natural C alignment.
   Caller supplies readable input and valid aligned structs; no arbitrary-pointer
   validation is promised. Snapshot pointers are immutable until handle release.
   Never free with the host allocator, retain pointers after free, or race reads
   against release. Handles are process/library-instance local, not persistent IDs.
   No paths, UI values, parser arenas or borrowed input pointers cross this API. */
enum md_status { MD_OK=0, MD_BAD_ARGUMENT=1, MD_VERSION_MISMATCH=2,
 MD_INVALID_UTF8=3, MD_INPUT_LIMIT=4, MD_CANCELLED=5, MD_PANIC=6,
 MD_INVALID_HANDLE=7, MD_OUTPUT_LIMIT=8 };
typedef struct { uint32_t version,pointer_bytes,options_bytes,block_bytes,snapshot_bytes,probe_hooks; } md_abi_info;
typedef struct { uint32_t version,struct_bytes; uint64_t max_input_bytes,max_output_bytes,max_blocks,cancel_handle; } md_options;
typedef struct { uint64_t id; uint32_t kind,heading_level; uint64_t source_start,source_end,text_offset,text_len; } md_block;
typedef struct { uint32_t version,struct_bytes; uint64_t handle; const uint8_t* text; uint64_t text_len;
 const md_block* blocks; uint64_t block_count,core_ns; } md_snapshot;
uint32_t md_get_abi_v1(uint32_t,md_abi_info*);
uint32_t md_parse_v1(const uint8_t*,uint64_t,const md_options*,md_snapshot*);
uint32_t md_snapshot_free_v1(uint64_t);
uint32_t md_cancel_create_v1(uint64_t*);
uint32_t md_cancel_request_v1(uint64_t);
uint32_t md_cancel_free_v1(uint64_t);
/* Only present in the explicitly enabled probe-hooks build. No production use. */
uint32_t md_probe_live_v1(uint64_t*,uint64_t*);
uint32_t md_probe_panic_v1(void);
uint64_t md_probe_active_v1(void);
void md_probe_break_v1(void);
#ifdef __cplusplus
}
#endif
#endif
