#include <stddef.h>
#include <stdint.h>

extern void md_fuzz_parser(const uint8_t *data, size_t size);
extern void md_fuzz_model_decoder(const uint8_t *data, size_t size);
extern void md_fuzz_ffi_session_find_anchor(const uint8_t *data, size_t size);
extern void md_fuzz_ffi_workspace(const uint8_t *data, size_t size);

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
#if defined(FUZZ_TARGET_PARSER)
    md_fuzz_parser(data, size);
#elif defined(FUZZ_TARGET_MODEL_DECODER)
    md_fuzz_model_decoder(data, size);
#elif defined(FUZZ_TARGET_FFI_SESSION)
    md_fuzz_ffi_session_find_anchor(data, size);
#elif defined(FUZZ_TARGET_FFI_WORKSPACE)
    md_fuzz_ffi_workspace(data, size);
#else
#error "Define one of FUZZ_TARGET_PARSER, FUZZ_TARGET_MODEL_DECODER, FUZZ_TARGET_FFI_SESSION, FUZZ_TARGET_FFI_WORKSPACE"
#endif
    return 0;
}
