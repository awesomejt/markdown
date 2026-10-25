// FEAS-003: C++/WinRT host smoke. Literal source, not a Markdown renderer.
#include <windows.h>
#include <shellapi.h>
#include <psapi.h>
#undef GetCurrentTime
#include <MddBootstrap.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.UI.Text.h>
#include <winrt/Windows.UI.Xaml.Interop.h>
#include <winrt/Microsoft.UI.Dispatching.h>
#include <winrt/Microsoft.UI.Xaml.h>
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Documents.h>
#include <winrt/Microsoft.UI.Xaml.Media.h>
#include <winrt/Microsoft.UI.Xaml.Markup.h>
#include <winrt/Microsoft.UI.Xaml.XamlTypeInfo.h>
#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Windows.Graphics.h>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <sstream>

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
using namespace Microsoft::UI::Xaml::Documents;
using namespace Microsoft::UI::Xaml::Media;
static auto started = std::chrono::steady_clock::now();
static std::wstring inputPath, outputPath;
static Application application{nullptr};
static void checkpoint(char const* stage) { if (!outputPath.empty()) { std::ofstream out{std::filesystem::path(outputPath)}; out << "{\"phase\":\"" << stage << "\"}"; } }
static double elapsed() { return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count(); }

struct NativeApp : ApplicationT<NativeApp, Microsoft::UI::Xaml::Markup::IXamlMetadataProvider>
{
    NativeApp() { checkpoint("app-constructor"); }
    Microsoft::UI::Xaml::XamlTypeInfo::XamlControlsXamlMetaDataProvider provider;
    auto GetXamlType(hstring const& name) { return provider.GetXamlType(name); }
    auto GetXamlType(Windows::UI::Xaml::Interop::TypeName const& type) { return provider.GetXamlType(type); }
    auto GetXmlnsDefinitions() { return provider.GetXmlnsDefinitions(); }
    Window window{nullptr};
    RichTextBlock text{nullptr};
    Microsoft::UI::Dispatching::DispatcherQueueTimer timer{nullptr};
    std::wstring content;
    event_token renderToken{};
    double firstRender = 0;
    void OnLaunched(LaunchActivatedEventArgs const&)
    {
        checkpoint("on-launched");
        Resources().MergedDictionaries().Append(XamlControlsResources());
        std::ifstream stream{std::filesystem::path(inputPath), std::ios::binary};
        std::stringstream bytes; bytes << stream.rdbuf();
        if (!stream && bytes.str().empty()) throw hresult_error(E_FAIL, L"Cannot read native probe input");
        content = to_hstring(bytes.str()).c_str();
        window = Window(); window.Title(L"FEAS-003 C++/WinRT literal host smoke");
        window.AppWindow().Resize(Windows::Graphics::SizeInt32{1100, 850});
        checkpoint("window-created");
        text = RichTextBlock(); text.IsTextSelectionEnabled(true); text.TextWrapping(TextWrapping::Wrap); text.FontSize(16);
        Paragraph paragraph; Run run; run.Text(content); paragraph.Inlines().Append(run); text.Blocks().Append(paragraph);
        ScrollViewer scroll; scroll.Content(text); window.Content(scroll);
        renderToken = CompositionTarget::Rendering([this](auto const&, auto const&) {
            firstRender = elapsed(); CompositionTarget::Rendering(renderToken);
            if (outputPath.empty()) return;
            timer = window.DispatcherQueue().CreateTimer();
            timer.Interval(std::chrono::milliseconds(200)); timer.IsRepeating(false);
            timer.Tick([this](auto const&, auto const&) {
                text.SelectAll(); auto selected = std::wstring(text.SelectedText());
                // Native paragraph selection may add a terminal CR/LF.
                auto normalize = [](std::wstring value) {
                    std::wstring clean; for (auto ch : value) if (ch != L'\r') clean += ch;
                    while (!clean.empty() && clean.back() == L'\n') clean.pop_back(); return clean;
                };
                PROCESS_MEMORY_COUNTERS memory{sizeof(memory)};
                GetProcessMemoryInfo(GetCurrentProcess(), &memory, sizeof(memory));
                std::ofstream out{std::filesystem::path(outputPath)};
                out << "{\"host\":\"C++/WinRT literal source\",\"entry_to_render_callback_ms\":" << firstRender
                    << ",\"selection_matches\":" << (normalize(selected) == normalize(content) ? "true" : "false")
                    << ",\"peak_working_set_bytes\":" << memory.PeakWorkingSetSize
                    << ",\"dpi_scale\":" << text.XamlRoot().RasterizationScale() << "}";
                out.close(); window.Close();
            });
            timer.Start();
        });
        checkpoint("before-activate"); window.Activate(); checkpoint("activated");
    }
};

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int)
{
    started = std::chrono::steady_clock::now();
    int count = 0; auto args = CommandLineToArgvW(GetCommandLineW(), &count);
    if (count < 2) return 2;
    inputPath = args[1]; if (count > 2) outputPath = args[2]; LocalFree(args);
    wchar_t debugFlag[2];
    if (GetEnvironmentVariableW(L"FEAS003_DEBUG_BREAK", debugFlag, 2)) __debugbreak();
    try {
        PACKAGE_VERSION minimum{};
        check_hresult(MddBootstrapInitialize2(0x00020005, nullptr, minimum, MddBootstrapInitializeOptions_None));
        checkpoint("bootstrapped");
        init_apartment(apartment_type::single_threaded);
        Application::Start([](auto const&) { application = make<NativeApp>(); });
        MddBootstrapShutdown();
        return 0;
    } catch (hresult_error const& error) {
        if (!outputPath.empty()) { std::ofstream out{std::filesystem::path(outputPath)}; out << "{\"hresult\":" << error.code().value << "}"; }
        return 1;
    }
}
