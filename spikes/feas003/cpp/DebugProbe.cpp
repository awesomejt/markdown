// Source/PDB debugging smoke using Windows debugging APIs. No UI automation.
#define NOMINMAX
#include <windows.h>
#include <dbghelp.h>
#include <filesystem>
#include <fstream>
#include <string>
#include <iostream>

int wmain(int argc, wchar_t** argv)
{
    if (argc != 4) return 2;
    std::wstring command = L"\"" + std::wstring(argv[1]) + L"\" \"" + argv[2] + L"\"";
#ifdef FEAS006
    command += L" --debug-break";
#endif
    STARTUPINFOW startup{sizeof(startup)}; PROCESS_INFORMATION process{};
    wchar_t previous[32768]; auto length = GetEnvironmentVariableW(L"FEAS003_DEBUG_BREAK", previous, 32768);
    SetEnvironmentVariableW(L"FEAS003_DEBUG_BREAK", L"1");
    BOOL created = CreateProcessW(argv[1], command.data(), nullptr, nullptr, FALSE,
        DEBUG_ONLY_THIS_PROCESS, nullptr, nullptr, &startup, &process);
    SetEnvironmentVariableW(L"FEAS003_DEBUG_BREAK", length > 0 && length < 32768 ? previous : nullptr);
    if (!created) { std::cerr << "CreateProcess failed: " << GetLastError(); return 3; }
    SymSetOptions(SYMOPT_LOAD_LINES | SYMOPT_UNDNAME);
    SymInitialize(process.hProcess, nullptr, FALSE);
    bool found = false, exited = false;
    std::string source; DWORD line = 0;
    auto deadline = GetTickCount64() + 15000;
    while (!exited && GetTickCount64() < deadline)
    {
        DEBUG_EVENT event{};
        if (!WaitForDebugEvent(&event, 1000)) continue;
        DWORD continuation = DBG_CONTINUE;
        switch (event.dwDebugEventCode)
        {
        case CREATE_PROCESS_DEBUG_EVENT:
            SymLoadModuleExW(process.hProcess, event.u.CreateProcessInfo.hFile, argv[1], nullptr,
                reinterpret_cast<DWORD64>(event.u.CreateProcessInfo.lpBaseOfImage), 0, nullptr, 0);
            if (event.u.CreateProcessInfo.hFile) CloseHandle(event.u.CreateProcessInfo.hFile);
            // The debug-event process/thread handles are owned separately.
            if (event.u.CreateProcessInfo.hProcess != process.hProcess) CloseHandle(event.u.CreateProcessInfo.hProcess);
            if (event.u.CreateProcessInfo.hThread != process.hThread) CloseHandle(event.u.CreateProcessInfo.hThread);
            break;
        case CREATE_THREAD_DEBUG_EVENT: CloseHandle(event.u.CreateThread.hThread); break;
        case LOAD_DLL_DEBUG_EVENT:
#ifdef FEAS006
            if (event.u.LoadDll.hFile) {
                wchar_t file[32768];
                DWORD length = GetFinalPathNameByHandleW(event.u.LoadDll.hFile,file,32768,FILE_NAME_NORMALIZED);
                if (length && length < 32768 && std::wstring(file).find(L"feas006_core.dll") != std::wstring::npos)
                    SymLoadModuleExW(process.hProcess,event.u.LoadDll.hFile,file,nullptr,
                        reinterpret_cast<DWORD64>(event.u.LoadDll.lpBaseOfDll),0,nullptr,0);
            }
#endif
            if (event.u.LoadDll.hFile) CloseHandle(event.u.LoadDll.hFile);
            break;
        case EXCEPTION_DEBUG_EVENT:
            if (event.u.Exception.ExceptionRecord.ExceptionCode == EXCEPTION_BREAKPOINT)
            {
                IMAGEHLP_LINE64 location{sizeof(location)}; DWORD displacement = 0;
                auto address = reinterpret_cast<DWORD64>(event.u.Exception.ExceptionRecord.ExceptionAddress);
                if (SymGetLineFromAddr64(process.hProcess, address, &displacement, &location)
#ifdef FEAS006
                    && std::string(location.FileName).find("lib.rs") != std::string::npos)
#else
                    && std::string(location.FileName).find("NativeProbe.cpp") != std::string::npos)
#endif
                {
#ifdef FEAS006
                    source = "rust/src/lib.rs";
#else
                    source = "NativeProbe.cpp";
#endif
                    line = location.LineNumber; found = true;
                    // Stop before bootstrap/window creation; do not interrupt a manual UI check.
                    TerminateProcess(process.hProcess, 0);
                }
            }
            else continuation = DBG_EXCEPTION_NOT_HANDLED;
            break;
        case EXIT_PROCESS_DEBUG_EVENT: exited = true; break;
        }
        ContinueDebugEvent(event.dwProcessId, event.dwThreadId, continuation);
    }
    if (!exited) TerminateProcess(process.hProcess, 1);
    SymCleanup(process.hProcess);
    CloseHandle(process.hThread); CloseHandle(process.hProcess);
    std::ofstream out{std::filesystem::path(argv[3])};
    out << "{\"source_breakpoint\":" << (found ? "true" : "false")
        << ",\"source\":\"" << source << "\",\"line\":" << line
        << ",\"engine\":\"Win32 debugging API and DbgHelp PDB lines\"}";
    std::cout << (found ? "Native source breakpoint resolved" : "Source breakpoint was not resolved") << "\n";
    return found ? 0 : 1;
}
