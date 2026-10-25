// C++ caller of the same versioned value-only ABI used by the managed host.
#define NOMINMAX
#include <windows.h>
#include <psapi.h>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <vector>
#include "md_core.h"

template<typename T> T symbol(HMODULE module, char const* name) {
    auto address = GetProcAddress(module, name);
    if (!address) throw std::runtime_error("Missing ABI export");
    return reinterpret_cast<T>(address);
}
struct ModuleOwner { HMODULE value; ~ModuleOwner() { if (value) FreeLibrary(value); } };
struct SnapshotOwner {
    uint64_t handle;
    decltype(&md_snapshot_free_v1) release;
    ~SnapshotOwner() { if (handle) release(handle); }
};

int wmain(int argc, wchar_t** argv)
{
    if (argc < 3) return 2;
    try {
        auto dll = std::filesystem::absolute(argv[1]).wstring();
        auto module = LoadLibraryExW(dll.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
        if (!module) throw std::runtime_error("Cannot load local core DLL");
        ModuleOwner library{module};
        if (std::wstring(argv[2]) == L"--debug-break") {
            symbol<decltype(&md_probe_break_v1)>(module, "md_probe_break_v1")();
            return 0;
        }
        auto getAbi = symbol<decltype(&md_get_abi_v1)>(module, "md_get_abi_v1");
        auto parse = symbol<decltype(&md_parse_v1)>(module, "md_parse_v1");
        auto release = symbol<decltype(&md_snapshot_free_v1)>(module, "md_snapshot_free_v1");
        md_abi_info abi{};
        if (getAbi(1, &abi) != MD_OK || abi.pointer_bytes != sizeof(void*) || abi.options_bytes != sizeof(md_options)
            || abi.block_bytes != sizeof(md_block) || abi.snapshot_bytes != sizeof(md_snapshot))
            throw std::runtime_error("ABI negotiation failed");
        std::ifstream stream{std::filesystem::path(argv[2]),std::ios::binary};
        if (!stream) throw std::runtime_error("Cannot read host input");
        std::stringstream bytes; bytes << stream.rdbuf(); auto input = bytes.str();
        int iterations = argc > 3 ? std::stoi(argv[3]) : 3;
        std::cout << "{\"host\":\"C++/MSVC\",\"abi\":" << abi.version << ",\"rows\":[";
        for (int i = 0; i < iterations; ++i) {
            md_options options{1,sizeof(md_options),16*1024*1024,64*1024*1024,1000000,0}; md_snapshot result{};
            auto start = std::chrono::steady_clock::now();
            if (parse(reinterpret_cast<uint8_t const*>(input.data()), input.size(), &options, &result) != MD_OK)
                throw std::runtime_error("Native parse failed");
            SnapshotOwner owner{result.handle,release};
            auto callMs = std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count();
            start = std::chrono::steady_clock::now();
            std::string payload;
            if (result.text_len) payload.assign(reinterpret_cast<char const*>(result.text),result.text_len);
            std::vector<md_block> blocks;
            if (result.block_count) blocks.assign(result.blocks,result.blocks+result.block_count);
            for (auto const& block:blocks)
                if (block.source_start > block.source_end || block.source_end > input.size()
                    || block.text_offset > payload.size() || block.text_len > payload.size()-block.text_offset)
                    throw std::runtime_error("Native value bounds failed");
            auto copyMs = std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count();
            auto coreMs = result.core_ns / 1e6;
            if (release(result.handle) != MD_OK)
                throw std::runtime_error("Native ownership failure");
            owner.handle=0;
            if (release(result.handle) != MD_INVALID_HANDLE) throw std::runtime_error("Double release accepted");
            PROCESS_MEMORY_COUNTERS_EX memory{};memory.cb=sizeof(memory);
            GetProcessMemoryInfo(GetCurrentProcess(),reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&memory),sizeof(memory));
            if (i) std::cout << ',';
            std::cout << "{\"core_parse_model_ms\":" << coreMs << ",\"native_call_ms\":" << callMs
                << ",\"bulk_copy_ms\":" << copyMs << ",\"blocks\":" << blocks.size()
                << ",\"text_bytes\":" << payload.size() << ",\"private_bytes\":" << memory.PrivateUsage << '}';
        }
        std::cout << "]}" << std::endl;
        return 0;
    } catch (std::exception const& error) { std::cerr << error.what(); return 1; }
}
