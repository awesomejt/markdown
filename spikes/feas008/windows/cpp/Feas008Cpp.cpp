// FEAS-008 C++/WinRT WinUI 3 host: the same virtualized document surface as
// the C# host over the same Rust model (feas008.h). A ListView shows display
// fragments; selection, copy, find, jumps, links and images work on the model.
#define NOMINMAX
#include <windows.h>
#include <shellapi.h>
#include <psapi.h>
#include <dwmapi.h>
#undef GetCurrentTime
#include <MddBootstrap.h>
#include <microsoft.ui.xaml.window.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.ApplicationModel.DataTransfer.h>
#include <winrt/Windows.Storage.Streams.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.UI.Text.h>
#include <winrt/Windows.UI.Xaml.Interop.h>
#include <winrt/Microsoft.UI.h>
#include <winrt/Microsoft.UI.Dispatching.h>
#include <winrt/Microsoft.UI.Input.h>
#include <winrt/Microsoft.UI.Text.h>
#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Microsoft.UI.Xaml.h>
#include <winrt/Microsoft.UI.Xaml.Automation.h>
#include <winrt/Microsoft.UI.Xaml.Automation.Peers.h>
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Controls.Primitives.h>
#include <winrt/Microsoft.UI.Xaml.Documents.h>
#include <winrt/Microsoft.UI.Xaml.Input.h>
#include <winrt/Microsoft.UI.Xaml.Interop.h>
#include <winrt/Microsoft.UI.Xaml.Markup.h>
#include <winrt/Microsoft.UI.Xaml.Media.h>
#include <winrt/Microsoft.UI.Xaml.Media.Imaging.h>
#include <winrt/Microsoft.UI.Xaml.XamlTypeInfo.h>
#include <winrt/Windows.Graphics.h>
#include <algorithm>
#include <atomic>
#include <filesystem>
#include <format>
#include <functional>
#include <mutex>
#include <span>
#include <fstream>
#include <map>
#include <optional>
#include <string>
#include <thread>
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include "feas008.h"

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
using namespace Microsoft::UI::Xaml::Documents;
using namespace Microsoft::UI::Xaml::Media;
namespace Automation = Microsoft::UI::Xaml::Automation;
namespace Dispatching = Microsoft::UI::Dispatching;
using Windows::Foundation::IAsyncAction;
using Windows::Foundation::IAsyncOperation;
using Windows::Foundation::IInspectable;
using Microsoft::UI::Xaml::Media::Imaging::BitmapImage;
using Windows::Foundation::Point;
using Windows::Foundation::Rect;

// ---- Clock, presentation and output ----

static long long Qpc() { LARGE_INTEGER v; QueryPerformanceCounter(&v); return v.QuadPart; }
static long long QpcFrequency() { LARGE_INTEGER v; QueryPerformanceFrequency(&v); return v.QuadPart; }
static const long long Frequency = QpcFrequency();
static double Ms(long long from, long long to) { return (to - from) * 1000.0 / Frequency; }
static long long LaunchAt = 0;
static bool FromLauncher = false;
static long long MainAt = 0;

static void Write(std::string const& text)
{
    static std::mutex gate;
    std::lock_guard lock(gate);
    DWORD written = 0;
    WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), text.data(), static_cast<DWORD>(text.size()), &written, nullptr);
}
static void Emit(std::string const& json) { Write("EVENT " + json + "\n"); }

/// Opt-in input trace (FEAS008_TRACE=path) for diagnosing owner-reported
/// keyboard and focus behaviour; off during measurement.
static void Trace(std::string const& line)
{
    static std::wstring path = [] { wchar_t value[1024]{}; return GetEnvironmentVariableW(L"FEAS008_TRACE", value, 1024) ? std::wstring(value) : std::wstring(); }();
    if (path.empty()) return;
    std::ofstream out{ std::filesystem::path(path), std::ios::app };
    out << line << "\n";
}

static std::string Quote(std::string_view value)
{
    std::string out = "\"";
    for (unsigned char c : value)
    {
        switch (c)
        {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20) out += std::format("\\u{:04x}", c);
            else out += static_cast<char>(c);
        }
    }
    return out + "\"";
}
static std::string Bool(bool value) { return value ? "true" : "false"; }

static std::wstring Wide(std::string_view utf8)
{
    if (utf8.empty()) return {};
    auto n = MultiByteToWideChar(CP_UTF8, 0, utf8.data(), static_cast<int>(utf8.size()), nullptr, 0);
    std::wstring out(n, L'\0');
    MultiByteToWideChar(CP_UTF8, 0, utf8.data(), static_cast<int>(utf8.size()), out.data(), n);
    return out;
}
static std::string Narrow(std::wstring_view text)
{
    if (text.empty()) return {};
    auto n = WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), nullptr, 0, nullptr, nullptr);
    std::string out(n, '\0');
    WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), out.data(), n, nullptr, nullptr);
    return out;
}
static int Utf16Count(std::string_view utf8)
{
    return utf8.empty() ? 0 : MultiByteToWideChar(CP_UTF8, 0, utf8.data(), static_cast<int>(utf8.size()), nullptr, 0);
}

static long long PeakWorkingSetKiB()
{
    PROCESS_MEMORY_COUNTERS memory{ sizeof(memory) };
    GetProcessMemoryInfo(GetCurrentProcess(), &memory, sizeof(memory));
    return static_cast<long long>(memory.PeakWorkingSetSize / 1024);
}

// ---- Model ----

struct Pos
{
    uint32_t block = 0, offset = 0;
    auto operator<=>(Pos const&) const = default;
};

struct Model
{
    f8_doc doc{};
    ~Model() { if (doc.handle) f8_free_v1(doc.handle); }
    static std::string_view View(f8_str s) { return { reinterpret_cast<char const*>(s.ptr), static_cast<size_t>(s.len) }; }
    f8_item const& Item(int i) const { return doc.items[i]; }
    f8_block const& Block(uint32_t b) const { return doc.blocks[b]; }
    int ItemCount() const { return static_cast<int>(doc.item_count); }
    uint32_t BlockCount() const { return static_cast<uint32_t>(doc.block_count); }
    std::string_view Text(uint32_t b) const { return View(Block(b).text); }
    std::string_view Fragment(int i) const { auto& it = Item(i); return Text(it.block).substr(it.start, it.end - it.start); }
    Pos Start() const { return {}; }
    Pos End() const { return doc.block_count ? Pos{ BlockCount() - 1, static_cast<uint32_t>(Block(BlockCount() - 1).text.len) } : Pos{}; }
    int ItemFor(Pos p) const { return static_cast<int>(f8_item_for_v1(doc.handle, p.block, p.offset)); }
    uint64_t PlainOffset(Pos p) const { return f8_plain_offset_v1(doc.handle, p.block, p.offset); }
    std::string TextBetween(Pos a, Pos b) const
    {
        f8_str s{};
        if (f8_text_between_v1(doc.handle, a.block, a.offset, b.block, b.offset, &s) != F8_OK) return {};
        std::string out(View(s));
        f8_str_free_v1(s);
        return out;
    }
    std::string_view SourceBetween(Pos a, Pos b) const
    {
        f8_str s{};
        return f8_source_between_v1(doc.handle, a.block, a.offset, b.block, b.offset, &s) == F8_OK ? View(s) : std::string_view{};
    }
    std::vector<f8_match> Find(std::string const& query) const
    {
        f8_match const* matches = nullptr; uint64_t count = 0;
        if (f8_find_v1(doc.handle, reinterpret_cast<uint8_t const*>(query.data()), query.size(), &matches, &count) != F8_OK) return {};
        std::vector<f8_match> out(matches, matches + count);
        f8_matches_free_v1(matches, count);
        return out;
    }
    uint32_t Boundary(uint32_t b, uint32_t offset) const
    {
        auto text = Text(b);
        offset = std::min<uint32_t>(offset, static_cast<uint32_t>(text.size()));
        while (offset > 0 && offset < text.size() && (static_cast<unsigned char>(text[offset]) & 0xC0) == 0x80) offset--;
        return offset;
    }
    uint32_t NextBoundary(uint32_t b, uint32_t offset) const
    {
        auto text = Text(b);
        auto next = offset + 1;
        while (next < text.size() && (static_cast<unsigned char>(text[next]) & 0xC0) == 0x80) next++;
        return std::min<uint32_t>(next, static_cast<uint32_t>(text.size()));
    }
};

struct Config
{
    std::wstring input;
    bool automated = false, control = false;
    std::wstring report;
    uint64_t fragmentBytes = 0;
};

static Config config;
static std::unique_ptr<Model> model;
static double readMs = 0, parseMs = 0;
static long long parsedAt = 0;
static std::thread loader;
static std::exception_ptr loadError;

// ---- Windows adapters: clipboard and image policy ----

namespace NativeClipboard
{
    static UINT MarkdownFormat() { static UINT format = RegisterClipboardFormatW(L"text/markdown"); return format; }
    static bool Open(HWND owner)
    {
        for (int attempt = 0; attempt < 50; attempt++) { if (OpenClipboard(owner)) return true; Sleep(10); }
        return false;
    }
    static HGLOBAL Global(void const* bytes, size_t size, size_t terminator)
    {
        auto memory = GlobalAlloc(GMEM_MOVEABLE, size + terminator);
        if (!memory) throw std::bad_alloc();
        auto target = static_cast<char*>(GlobalLock(memory));
        memcpy(target, bytes, size);
        memset(target + size, 0, terminator);
        GlobalUnlock(memory);
        return memory;
    }
    static void Set(HWND owner, std::wstring const& plain, std::string_view markdown)
    {
        if (!Open(owner)) throw hresult_error(E_ACCESSDENIED, L"clipboard unavailable");
        EmptyClipboard();
        auto text = Global(plain.data(), plain.size() * sizeof(wchar_t), sizeof(wchar_t));
        if (!SetClipboardData(CF_UNICODETEXT, text)) GlobalFree(text);
        auto source = Global(markdown.data(), markdown.size(), 0);
        if (!SetClipboardData(MarkdownFormat(), source)) GlobalFree(source);
        CloseClipboard();
    }
    static std::optional<std::string> Read(UINT format)
    {
        if (!Open(nullptr)) return std::nullopt;
        std::optional<std::string> out;
        if (auto memory = GetClipboardData(format))
        {
            auto size = GlobalSize(memory);
            out.emplace(static_cast<char const*>(GlobalLock(memory)), size);
            GlobalUnlock(memory);
        }
        CloseClipboard();
        return out;
    }
}

namespace ImagePolicy
{
    constexpr uint64_t MaxBytes = 20ull * 1024 * 1024;
    constexpr uint32_t MaxSide = 16384;

    static std::wstring FinalPath(HANDLE handle)
    {
        std::wstring buffer(32768, L'\0');
        auto length = GetFinalPathNameByHandleW(handle, buffer.data(), static_cast<DWORD>(buffer.size()), 0);
        if (length == 0 || length >= buffer.size()) throw std::runtime_error("final path unavailable");
        buffer.resize(length);
        return buffer;
    }

    struct Handle
    {
        HANDLE value;
        ~Handle() { if (value != INVALID_HANDLE_VALUE) CloseHandle(value); }
    };

    /// Same checks as the C# and GTK adapters: containment after following
    /// links through the opened handle, size, magic bytes and PNG dimensions.
    static std::vector<uint8_t> Read(std::wstring const& folder, std::string const& relative)
    {
        Handle directory{ CreateFileW(folder.c_str(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, nullptr) };
        if (directory.value == INVALID_HANDLE_VALUE) throw std::runtime_error("document folder unavailable");
        auto root = FinalPath(directory.value);
        while (!root.empty() && root.back() == L'\\') root.pop_back();
        root += L'\\';
        auto candidate = folder + L"\\" + Wide(relative);
        std::replace(candidate.begin(), candidate.end(), L'/', L'\\');
        Handle file{ CreateFileW(candidate.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr) };
        if (file.value == INVALID_HANDLE_VALUE) throw std::runtime_error(std::format("not found ({})", GetLastError()));
        auto resolved = FinalPath(file.value);
        if (resolved.size() < root.size() || CompareStringOrdinal(resolved.c_str(), static_cast<int>(root.size()), root.c_str(), static_cast<int>(root.size()), TRUE) != CSTR_EQUAL)
            throw std::runtime_error("resolves outside the document folder");
        LARGE_INTEGER size{};
        GetFileSizeEx(file.value, &size);
        if (static_cast<uint64_t>(size.QuadPart) > MaxBytes) throw std::runtime_error("not a regular file within the size limit");
        std::vector<uint8_t> bytes(static_cast<size_t>(size.QuadPart));
        DWORD read = 0;
        if (!bytes.empty() && !ReadFile(file.value, bytes.data(), static_cast<DWORD>(bytes.size()), &read, nullptr)) throw std::runtime_error("read failed");
        auto starts = [&](std::initializer_list<uint8_t> magic, size_t at = 0) {
            return bytes.size() >= at + magic.size() && std::equal(magic.begin(), magic.end(), bytes.begin() + at);
        };
        bool png = starts({ 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a });
        bool known = png || starts({ 0xff, 0xd8, 0xff }) || starts({ 'G', 'I', 'F', '8' }) || (starts({ 'R', 'I', 'F', 'F' }) && starts({ 'W', 'E', 'B', 'P' }, 8));
        if (!known) throw std::runtime_error("content is not an allowed raster format");
        if (png && bytes.size() >= 24)
        {
            auto be = [&](size_t at) { return (uint32_t(bytes[at]) << 24) | (uint32_t(bytes[at + 1]) << 16) | (uint32_t(bytes[at + 2]) << 8) | bytes[at + 3]; };
            auto w = be(16), h = be(20);
            if (w > MaxSide || h > MaxSide) throw std::runtime_error(std::format("{}x{} exceeds the dimension limit", w, h));
        }
        return bytes;
    }
}

// ---- Items source and rows ----

/// Count-only items source: the list asks for indexes, nothing is materialized.
struct IndexIterator : implements<IndexIterator, Interop::IBindableIterator>
{
    uint32_t index = 0, count = 0;
    IndexIterator(uint32_t count) : count(count) {}
    IInspectable Current() { return box_value(static_cast<int32_t>(index)); }
    bool HasCurrent() { return index < count; }
    bool MoveNext() { return ++index < count; }
};

struct IndexList : implements<IndexList, Interop::IBindableVector, Interop::IBindableIterable>
{
    uint32_t count;
    IndexList(uint32_t count) : count(count) {}
    IInspectable GetAt(uint32_t index) { return box_value(static_cast<int32_t>(index)); }
    uint32_t Size() { return count; }
    Interop::IBindableVectorView GetView() { throw hresult_not_implemented(); }
    bool IndexOf(IInspectable const& value, uint32_t& index)
    {
        auto boxed = value.try_as<Windows::Foundation::IReference<int32_t>>();
        if (!boxed || boxed.Value() < 0 || static_cast<uint32_t>(boxed.Value()) >= count) return false;
        index = static_cast<uint32_t>(boxed.Value());
        return true;
    }
    void SetAt(uint32_t, IInspectable const&) { throw hresult_not_implemented(); }
    void InsertAt(uint32_t, IInspectable const&) { throw hresult_not_implemented(); }
    void RemoveAt(uint32_t) { throw hresult_not_implemented(); }
    void Append(IInspectable const&) { throw hresult_not_implemented(); }
    void RemoveAtEnd() { throw hresult_not_implemented(); }
    void Clear() { throw hresult_not_implemented(); }
    Interop::IBindableIterator First() { return make<IndexIterator>(count); }
};

struct Segment { Run element; uint32_t byteStart; std::wstring text; };

/// Row visuals owned per container and moved into its template root.
struct Row
{
    ListViewItem container{ nullptr };
    Grid root;
    RichTextBlock text;
    Image picture;
    TextBlock note;
    std::vector<Segment> segments;
    int item = -1;
    std::wstring shown;
    std::optional<std::string> imagePath;

    Row()
    {
        text.TextWrapping(TextWrapping::Wrap);
        text.IsTextSelectionEnabled(false);
        picture.Stretch(Stretch::None);
        picture.HorizontalAlignment(HorizontalAlignment::Left);
        picture.Visibility(Visibility::Collapsed);
        note.FontSize(12);
        note.Opacity(0.7);
        note.Visibility(Visibility::Collapsed);
        for (int i = 0; i < 3; i++) { RowDefinition r; r.Height(GridLengthHelper::Auto()); root.RowDefinitions().Append(r); }
        Grid::SetRow(text, 1);
        Grid::SetRow(note, 2);
        root.Children().Append(picture);
        root.Children().Append(text);
        root.Children().Append(note);
    }

    void Attach()
    {
        auto host = container.ContentTemplateRoot().try_as<Grid>();
        if (!host) return;
        auto parent = root.Parent();
        if (parent && parent == host) return;
        if (auto old = parent.try_as<Panel>()) { uint32_t at; if (old.Children().IndexOf(root, at)) old.Children().RemoveAt(at); }
        host.Children().Clear();
        host.Children().Append(root);
    }
};

struct ImageState { BitmapImage bitmap{ nullptr }; std::optional<std::string> failure; bool Done() const { return bitmap || failure; } };

enum Kind : uint32_t { KHeading, KParagraph, KListItem, KCode, KTableRow, KRule, KHtml, KMath, KImage };
static char const* KindNames[] = { "heading", "paragraph", "listitem", "code", "tablerow", "rule", "html", "math", "image" };
static char const* IntentNames[] = { "External", "Fragment", "MissingFragment", "LocalDocument", "Blocked" };

// ---- Awaitables over the UI thread ----

struct NextFrame
{
    event_token token{};
    bool await_ready() const noexcept { return false; }
    void await_suspend(std::coroutine_handle<> handle)
    {
        token = CompositionTarget::Rendering([this, handle](auto&&, auto&&) { CompositionTarget::Rendering(token); handle(); });
    }
    void await_resume() const noexcept {}
};

/// Waits for the first frame where `condition` holds, then for the DWM
/// present after the UI thread has committed it. Five seconds without the
/// condition is reported as a timeout.
struct Presented
{
    std::function<bool()> condition;
    Dispatching::DispatcherQueue queue;
    event_token token{};
    long long deadline = 0, paint = 0;
    std::optional<long long> presented;
    bool timedOut = false;
    bool await_ready() const noexcept { return false; }
    void await_suspend(std::coroutine_handle<> handle)
    {
        deadline = Qpc() + 5 * Frequency;
        token = CompositionTarget::Rendering([this, handle](auto&&, auto&&) {
            if (Qpc() > deadline) { CompositionTarget::Rendering(token); timedOut = true; paint = Qpc(); handle(); return; }
            if (!condition()) return;
            CompositionTarget::Rendering(token);
            paint = Qpc();
            queue.TryEnqueue(Dispatching::DispatcherQueuePriority::Low, [this, handle] {
                std::thread([this, handle] {
                    if (SUCCEEDED(DwmFlush())) presented = Qpc();
                    queue.TryEnqueue([handle] { handle(); });
                }).detach();
            });
        });
    }
    struct Result
    {
        long long paint;
        std::optional<long long> presented;
        bool timedOut;
        long long Best() const { return presented.value_or(paint); }
    };
    Result await_resume() const noexcept { return { paint, presented, timedOut }; }
};

struct Delay
{
    Dispatching::DispatcherQueue queue;
    int ms;
    Dispatching::DispatcherQueueTimer timer{ nullptr };
    bool await_ready() const noexcept { return false; }
    void await_suspend(std::coroutine_handle<> handle)
    {
        timer = queue.CreateTimer();
        timer.Interval(std::chrono::milliseconds(ms));
        timer.IsRepeating(false);
        timer.Tick([handle](auto&&, auto&&) { handle(); });
        timer.Start();
    }
    void await_resume() const noexcept {}
};

// ---- Window ----

static bool Down(Windows::System::VirtualKey key)
{
    return (Microsoft::UI::Input::InputKeyboardSource::GetKeyStateForCurrentThread(key) & Windows::UI::Core::CoreVirtualKeyStates::Down) == Windows::UI::Core::CoreVirtualKeyStates::Down;
}

struct DocumentWindow
{
    Window window;
    std::wstring folder;
    Grid root;
    ListView list;
    ListView outline;
    TextBox search;
    TextBlock status;
    ScrollViewer scroll{ nullptr };
    std::unordered_map<void*, std::unique_ptr<Row>> rows;
    std::unordered_set<Row*> bound;
    std::map<std::string, ImageState> images;
    std::vector<std::string> decisions;
    SolidColorBrush highlight{ Windows::UI::Color{ 0xff, 0x99, 0xc9, 0xff } };
    SolidColorBrush quoteBrush{ Microsoft::UI::Colors::Gray() };
    SolidColorBrush codeBrush{ Windows::UI::Color{ 0x18, 0x80, 0x80, 0x80 } };
    FontFamily mono{ L"Cascadia Mono, Consolas" };
    Dispatching::DispatcherQueueTimer autoScroll{ nullptr };
    std::optional<std::pair<Pos, Pos>> selection;
    std::optional<Pos> dragAnchor;
    bool dragging = false;
    Point lastPointer{};
    std::vector<f8_match> matches;
    std::string lastQuery;
    int matchIndex = -1;
    bool suppressOutline = false;
    int timedOut = 0;
    long long windowAt = 0;
    Row* jumped = nullptr;

    Dispatching::DispatcherQueue Queue() { return window.DispatcherQueue(); }

    DocumentWindow()
    {
        auto path = std::filesystem::path(config.input);
        folder = path.parent_path().wstring();
        window.Title(L"FEAS-008 C++/WinRT — " + path.filename().wstring());
        window.AppWindow().Resize(Windows::Graphics::SizeInt32{ 1200, 900 });
        RowDefinition bar; bar.Height(GridLengthHelper::Auto()); root.RowDefinitions().Append(bar);
        RowDefinition body; body.Height(GridLengthHelper::FromValueAndType(1, GridUnitType::Star)); root.RowDefinitions().Append(body);
        ColumnDefinition side; side.Width(GridLengthHelper::FromPixels(260)); root.ColumnDefinitions().Append(side);
        ColumnDefinition main; main.Width(GridLengthHelper::FromValueAndType(1, GridUnitType::Star)); root.ColumnDefinitions().Append(main);
        StackPanel toolbar; toolbar.Orientation(Orientation::Horizontal); toolbar.Margin(ThicknessHelper::FromUniformLength(8));
        search.PlaceholderText(L"Find (Ctrl+F, Enter, F3)"); search.Width(300);
        status.Margin(ThicknessHelper::FromLengths(12, 0, 0, 0)); status.VerticalAlignment(VerticalAlignment::Center);
        toolbar.Children().Append(search); toolbar.Children().Append(status);
        Grid::SetColumnSpan(toolbar, 2);
        root.Children().Append(toolbar);
        Grid::SetRow(outline, 1);
        Grid::SetRow(list, 1); Grid::SetColumn(list, 1);
        root.Children().Append(outline); root.Children().Append(list);
        list.SelectionMode(ListViewSelectionMode::None); list.IsItemClickEnabled(false);
        outline.SelectionMode(ListViewSelectionMode::Single);
        Automation::AutomationProperties::SetName(list, L"Document");
        Automation::AutomationProperties::SetName(outline, L"Outline");
        Automation::AutomationProperties::SetName(search, L"Find");
        window.Content(root);

        list.ItemTemplate(Markup::XamlReader::Load(L"<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><Grid/></DataTemplate>").as<DataTemplate>());
        list.ChoosingItemContainer([this](auto&&, ChoosingItemContainerEventArgs const& args) {
            if (!args.ItemContainer())
            {
                ListViewItem container;
                container.MinHeight(0); container.Padding(ThicknessHelper::FromUniformLength(0));
                container.Margin(ThicknessHelper::FromUniformLength(0)); container.CornerRadius(CornerRadiusHelper::FromUniformRadius(0));
                container.HorizontalContentAlignment(HorizontalAlignment::Stretch);
                auto row = std::make_unique<Row>();
                row->container = container;
                rows[get_abi(container.as<Windows::Foundation::IUnknown>())] = std::move(row);
                args.ItemContainer(container);
            }
        });
        list.ContainerContentChanging([this](auto&&, ContainerContentChangingEventArgs const& args) {
            auto row = RowOf(args.ItemContainer());
            if (!row) return;
            if (args.InRecycleQueue()) { bound.erase(row); row->item = -1; }
            else { row->Attach(); Bind(*row, args.ItemIndex()); bound.insert(row); }
            args.Handled(true);
        });
        list.AddHandler(UIElement::PointerPressedEvent(), box_value(Input::PointerEventHandler([this](auto&&, Input::PointerRoutedEventArgs const& e) { OnPointerPressed(e); })), true);
        list.AddHandler(UIElement::PointerMovedEvent(), box_value(Input::PointerEventHandler([this](auto&&, Input::PointerRoutedEventArgs const& e) { OnPointerMoved(e); })), true);
        list.AddHandler(UIElement::PointerReleasedEvent(), box_value(Input::PointerEventHandler([this](auto&&, Input::PointerRoutedEventArgs const& e) {
            if (!dragging) return;
            list.ReleasePointerCapture(e.Pointer());
            EndDrag();
        })), true);
        list.PointerCaptureLost([this](auto&&, auto&&) { EndDrag(); });
        root.PreviewKeyDown([this](auto&&, Input::KeyRoutedEventArgs const& e) { OnPreviewKeyDown(e); });
        search.KeyDown([this](auto&&, Input::KeyRoutedEventArgs const& e) {
            if (e.Key() == Windows::System::VirtualKey::Enter) { FindNext(!Down(Windows::System::VirtualKey::Shift)); e.Handled(true); }
        });
        outline.SelectionChanged([this](auto&&, auto&&) { if (!suppressOutline && outline.SelectedIndex() >= 0) JumpToBlock(model->doc.outline[outline.SelectedIndex()].block, false); });
        // Preview: the list consumes Enter before KeyDown.
        outline.PreviewKeyDown([this](auto&&, Input::KeyRoutedEventArgs const& e) {
            if (e.Key() == Windows::System::VirtualKey::Enter && outline.SelectedIndex() >= 0) { JumpToBlock(model->doc.outline[outline.SelectedIndex()].block, true); e.Handled(true); }
        });
        outline.DoubleTapped([this](auto&&, auto&&) { if (outline.SelectedIndex() >= 0) JumpToBlock(model->doc.outline[outline.SelectedIndex()].block, true); });
        autoScroll = Queue().CreateTimer();
        autoScroll.Interval(std::chrono::milliseconds(16));
        autoScroll.Tick([this](auto&&, auto&&) { AutoScrollTick(); });
        root.Loaded([this](auto&&, auto&&) { Load(); });
    }

    Row* RowOf(IInspectable const& container)
    {
        if (!container) return nullptr;
        auto found = rows.find(get_abi(container.as<Windows::Foundation::IUnknown>()));
        return found == rows.end() ? nullptr : found->second.get();
    }

    static ScrollViewer FindScrollViewer(DependencyObject const& node)
    {
        for (int i = 0; i < VisualTreeHelper::GetChildrenCount(node); i++)
        {
            auto child = VisualTreeHelper::GetChild(node, i);
            if (auto viewer = child.try_as<ScrollViewer>()) return viewer;
            if (auto found = FindScrollViewer(child)) return found;
        }
        return nullptr;
    }

    fire_and_forget Load()
    {
        windowAt = Qpc();
        if (loader.joinable())
        {
            co_await resume_background();
            loader.join();
            co_await ResumeUi();
        }
        if (loadError || !model) { Fail("model load failed"); co_return; }
        auto titles = single_threaded_vector<IInspectable>();
        for (uint64_t i = 0; i < model->doc.outline_count; i++)
        {
            auto& entry = model->doc.outline[i];
            titles.Append(box_value(hstring(std::wstring((entry.level - 1) * 3, L' ') + Wide(Model::View(entry.title)))));
        }
        outline.ItemsSource(titles);
        list.ItemsSource(make<IndexList>(static_cast<uint32_t>(model->ItemCount())));
        list.UpdateLayout();
        scroll = FindScrollViewer(list);
        if (!scroll) { Fail("ListView has no ScrollViewer"); co_return; }
        auto launch = co_await Presented{ [this] { return std::any_of(bound.begin(), bound.end(), [](Row* r) { return r->item >= 0 && r->text.ActualWidth() > 0; }); }, Queue() };
        auto paint = launch.paint;
        auto presented = launch.presented;
        Emit(std::format("{{\"event\":\"presented\",\"launch_to_presented_ms\":{:.3f}}}", Ms(LaunchAt, presented.value_or(paint))));
        if (config.automated) RunAutomated(paint, presented);
        else if (config.control) ControlLoop();
    }

    struct ResumeUiAwaiter
    {
        Dispatching::DispatcherQueue queue;
        bool await_ready() const noexcept { return false; }
        void await_suspend(std::coroutine_handle<> handle) { queue.TryEnqueue([handle] { handle(); }); }
        void await_resume() const noexcept {}
    };
    ResumeUiAwaiter ResumeUi() { return { Queue() }; }

    void Fail(std::string const& message)
    {
        Emit("{\"event\":\"error\",\"error\":" + Quote(message) + "}");
        if (!config.report.empty()) { std::ofstream out{ std::filesystem::path(config.report) }; out << "{\"host\":\"C++/WinRT\",\"error\":" << Quote(message) << "}"; }
        if (config.automated) Application::Current().Exit();
    }

    // ---- Row binding ----

    void Bind(Row& row, int index)
    {
        auto& item = model->Item(index);
        auto& block = model->Block(item.block);
        auto kind = static_cast<Kind>(block.kind);
        auto bytes = model->Text(item.block);
        row.item = index;
        row.segments.clear();
        row.shown.clear();
        Paragraph paragraph;
        for (auto const& run : std::span(model->doc.runs + block.run_offset, block.run_count))
        {
            auto s = std::max(run.start, item.start), e = std::min(run.end, item.end);
            if (s >= e) continue;
            auto text = Wide(bytes.substr(s, e - s));
            Run element;
            element.Text(text);
            if (run.style & F8_STYLE_BOLD) element.FontWeight(Microsoft::UI::Text::FontWeights::Bold());
            if (run.style & F8_STYLE_ITALIC) element.FontStyle(Windows::UI::Text::FontStyle::Italic);
            if (run.style & F8_STYLE_CODE) element.FontFamily(mono);
            if (run.style & F8_STYLE_STRIKE) element.TextDecorations(Windows::UI::Text::TextDecorations::Strikethrough);
            if (run.style & F8_STYLE_MARKER) element.Foreground(quoteBrush);
            if (run.link != F8_NONE)
            {
                Hyperlink hyperlink;
                hyperlink.Inlines().Append(element);
                hyperlink.Click([this, link = run.link](auto&&, auto&&) { ActivateLink(link); });
                paragraph.Inlines().Append(hyperlink);
            }
            else paragraph.Inlines().Append(element);
            row.segments.push_back({ element, s, text });
            row.shown += text;
        }
        row.text.Blocks().Clear();
        row.text.Blocks().Append(paragraph);
        static const double sizes[] = { 28, 24, 20, 18, 16, 15 };
        bool heading = kind == KHeading;
        row.text.FontSize(heading ? sizes[std::clamp<int>(block.level - 1, 0, 5)] : 15);
        row.text.FontWeight(heading ? Microsoft::UI::Text::FontWeights::SemiBold() : Microsoft::UI::Text::FontWeights::Normal());
        bool isMono = kind == KCode || kind == KMath || kind == KHtml;
        row.text.FontFamily(isMono ? mono : FontFamily::XamlAutoFontFamily());
        row.root.Background(isMono ? Brush(codeBrush) : Brush(nullptr));
        bool first = item.start == 0, last = item.end == block.text.len;
        row.root.Padding(ThicknessHelper::FromLengths(16 + 24.0 * block.indent, first ? (heading ? 14 : 6) : 0, 16, last ? 6 : 0));
        row.root.BorderThickness(ThicknessHelper::FromLengths(block.quote ? 3 : 0, 0, 0, 0));
        row.root.BorderBrush(quoteBrush);
        Automation::AutomationProperties::SetName(row.container, row.shown);
        Automation::AutomationProperties::SetHeadingLevel(row.container, heading && first ? static_cast<Automation::Peers::AutomationHeadingLevel>(block.level) : Automation::Peers::AutomationHeadingLevel::None);
        // The list item's own heading level is not exposed through UIA; its
        // text element's is, so Narrator heading navigation can find it.
        Automation::AutomationProperties::SetHeadingLevel(row.text, Automation::AutomationProperties::GetHeadingLevel(row.container));
        Automation::AutomationProperties::SetItemType(row.container, Wide(KindNames[std::min<uint32_t>(kind, 8)]));
        row.imagePath.reset();
        row.picture.Source(nullptr);
        row.picture.Visibility(Visibility::Collapsed);
        row.note.Visibility(Visibility::Collapsed);
        if (kind == KImage)
        {
            if (block.image_kind == F8_IMAGE_LOCAL)
            {
                row.imagePath = std::string(Model::View(block.image));
                RequestImage(*row.imagePath);
            }
            else ShowNote(row, "Image blocked: " + std::string(Model::View(block.image)));
            UpdatePicture(row);
        }
        ApplyHighlight(row);
    }

    static void ShowNote(Row& row, std::string const& text)
    {
        row.note.Text(Wide(text));
        row.note.Visibility(Visibility::Visible);
        Automation::AutomationProperties::SetHelpText(row.container, Wide(text));
    }

    static std::pair<Pos, Pos> Ordered(Pos a, Pos b) { return a <= b ? std::pair{ a, b } : std::pair{ b, a }; }

    void ApplyHighlight(Row& row)
    {
        row.text.TextHighlighters().Clear();
        if (!selection || row.item < 0) return;
        auto [a, b] = Ordered(selection->first, selection->second);
        auto& item = model->Item(row.item);
        Pos from{ item.block, item.start }, to{ item.block, item.end };
        if (b <= from || a >= to) return;
        auto s = a > from ? a.offset : item.start;
        auto e = b < to ? b.offset : item.end;
        if (s >= e) return;
        auto text = model->Text(item.block);
        TextHighlighter highlighter;
        highlighter.Background(highlight);
        highlighter.Ranges().Append(TextRange{ Utf16Count(text.substr(item.start, s - item.start)), Utf16Count(text.substr(s, e - s)) });
        row.text.TextHighlighters().Append(highlighter);
    }

    void SetSelection(std::optional<std::pair<Pos, Pos>> value)
    {
        selection = value;
        for (auto row : bound) ApplyHighlight(*row);
    }

    // ---- Images ----

    fire_and_forget RequestImage(std::string relative)
    {
        if (images.contains(relative)) co_return;
        images[relative] = {};
        std::vector<uint8_t> bytes;
        std::optional<std::string> failure;
        co_await resume_background();
        try { bytes = ImagePolicy::Read(folder, relative); }
        catch (std::exception const& error) { failure = error.what(); }
        co_await ResumeUi();
        auto& state = images[relative];
        if (failure) state.failure = failure;
        else
        {
            try
            {
                BitmapImage bitmap;
                Windows::Storage::Streams::InMemoryRandomAccessStream stream;
                Windows::Storage::Streams::DataWriter writer(stream);
                writer.WriteBytes(bytes);
                co_await writer.StoreAsync();
                writer.DetachStream();
                stream.Seek(0);
                co_await bitmap.SetSourceAsync(stream);
                if (static_cast<uint32_t>(bitmap.PixelWidth()) > ImagePolicy::MaxSide || static_cast<uint32_t>(bitmap.PixelHeight()) > ImagePolicy::MaxSide)
                    images[relative].failure = "decoded image exceeds the dimension limit";
                else images[relative].bitmap = bitmap;
            }
            catch (hresult_error const& error) { images[relative].failure = Narrow(error.message()); }
        }
        for (auto row : bound) if (row->imagePath == relative) UpdatePicture(*row);
    }

    void UpdatePicture(Row& row)
    {
        if (!row.imagePath) return;
        auto found = images.find(*row.imagePath);
        if (found == images.end()) return;
        if (found->second.bitmap)
        {
            row.picture.Source(found->second.bitmap);
            row.picture.Visibility(Visibility::Visible);
            Automation::AutomationProperties::SetName(row.picture, row.shown);
        }
        else if (found->second.failure) ShowNote(row, "Image refused: " + *found->second.failure);
    }

    // ---- Hit testing and pointer selection ----

    std::optional<Pos> HitTest(Point point)
    {
        Row* best = nullptr;
        double bestDistance = 1e300;
        for (auto row : bound)
        {
            if (row->item < 0 || row->container.ActualHeight() <= 0) continue;
            auto top = row->container.TransformToVisual(list).TransformPoint({ 0, 0 }).Y;
            double h = row->container.ActualHeight();
            double distance = point.Y < top ? top - point.Y : point.Y > top + h ? point.Y - top - h : 0;
            if (distance < bestDistance) { best = row; bestDistance = distance; }
        }
        if (!best) return std::nullopt;
        return PositionInRow(*best, list.TransformToVisual(best->text).TransformPoint(point));
    }

    Pos PositionInRow(Row& row, Point local)
    {
        auto& item = model->Item(row.item);
        if (row.segments.empty() || local.Y < 0) return { item.block, item.start };
        if (local.Y >= row.text.ActualHeight()) return { item.block, item.end };
        float x = static_cast<float>(std::clamp<double>(local.X, 0, std::max(0.0, row.text.ActualWidth() - 1)));
        auto offset = row.text.GetPositionFromPoint({ x, local.Y }).Offset();
        auto* chosen = &row.segments[0];
        auto start = chosen->element.ContentStart().Offset();
        for (auto& segment : row.segments)
        {
            auto at = segment.element.ContentStart().Offset();
            if (at > offset) break;
            chosen = &segment;
            start = at;
        }
        auto chars = std::clamp<int>(offset - start, 0, static_cast<int>(chosen->text.size()));
        if (chars > 0 && chars < static_cast<int>(chosen->text.size()) && IS_LOW_SURROGATE(chosen->text[chars])) chars--;
        return { item.block, chosen->byteStart + static_cast<uint32_t>(Narrow(std::wstring_view(chosen->text).substr(0, chars)).size()) };
    }

    void OnPointerPressed(Input::PointerRoutedEventArgs const& e)
    {
        auto point = e.GetCurrentPoint(list);
        if (!point.Properties().IsLeftButtonPressed() || !model) return;
        auto hit = HitTest(point.Position());
        Trace(std::format("press x={} y={} hit={}", point.Position().X, point.Position().Y, hit ? std::format("{}:{}", hit->block, hit->offset) : std::string("none")));
        if (!hit) return;
        if (Down(Windows::System::VirtualKey::Shift) && selection) { dragAnchor = selection->first; SetSelection(std::pair{ selection->first, *hit }); }
        else { dragAnchor = hit; SetSelection(std::nullopt); }
        dragging = true;
        lastPointer = point.Position();
        list.CapturePointer(e.Pointer());
        // A press moves keyboard focus to the row, so Esc, Ctrl+A/C and the
        // arrow keys act on the document after a click.
        if (auto row = Container(model->ItemFor(*hit))) row->container.Focus(FocusState::Pointer);
    }

    void OnPointerMoved(Input::PointerRoutedEventArgs const& e)
    {
        if (!dragging) return;
        lastPointer = e.GetCurrentPoint(list).Position();
        UpdateDrag();
        if (lastPointer.Y < 0 || lastPointer.Y > list.ActualHeight()) autoScroll.Start(); else autoScroll.Stop();
    }

    void EndDrag() { dragging = false; autoScroll.Stop(); }

    void UpdateDrag()
    {
        if (!dragAnchor) return;
        Point clamped{ lastPointer.X, static_cast<float>(std::clamp<double>(lastPointer.Y, 0, list.ActualHeight() - 1)) };
        auto hit = HitTest(clamped);
        if (!hit) return;
        // Keep the anchor even when the range is momentarily empty.
        if (*hit == *dragAnchor) SetSelection(std::nullopt); else SetSelection(std::pair{ *dragAnchor, *hit });
    }

    void AutoScrollTick()
    {
        if (!dragging) { autoScroll.Stop(); return; }
        double delta = lastPointer.Y < 0 ? lastPointer.Y : lastPointer.Y - list.ActualHeight();
        ScrollBy(std::clamp(delta, -60.0, 60.0));
        list.UpdateLayout();
        UpdateDrag();
    }

    void ScrollBy(double delta)
    {
        auto target = std::clamp(scroll.VerticalOffset() + delta, 0.0, scroll.ScrollableHeight());
        scroll.ChangeView(nullptr, Windows::Foundation::IReference<double>(target), nullptr, true);
    }

    // ---- Keyboard ----

    bool FocusInDocument()
    {
        auto focused = Input::FocusManager::GetFocusedElement(root.XamlRoot()).try_as<DependencyObject>();
        for (auto node = focused; node; node = VisualTreeHelper::GetParent(node))
        {
            if (node == list) return true;
            if (node == search || node == outline) return false;
        }
        return false;
    }

    void OnPreviewKeyDown(Input::KeyRoutedEventArgs const& e)
    {
        using Windows::System::VirtualKey;
        bool ctrl = Down(VirtualKey::Control), shift = Down(VirtualKey::Shift);
        auto key = e.Key();
        auto focused = Input::FocusManager::GetFocusedElement(root.XamlRoot());
        Trace(std::format("key={} ctrl={} shift={} focus={} in_document={} selection={}", static_cast<int>(key), ctrl, shift,
            focused ? Narrow(get_class_name(focused)) : std::string("null"), FocusInDocument(), selection.has_value()));
        if (ctrl && key == VirtualKey::F) { search.Focus(FocusState::Keyboard); search.SelectAll(); e.Handled(true); return; }
        if (key == VirtualKey::F3) { FindNext(!shift); e.Handled(true); return; }
        if (key == VirtualKey::Escape && selection && Input::FocusManager::GetFocusedElement(root.XamlRoot()) != search) { SetSelection(std::nullopt); e.Handled(true); return; }
        if (!FocusInDocument()) return;
        if (ctrl && key == VirtualKey::A) { SelectAll(); e.Handled(true); }
        else if (ctrl && key == VirtualKey::C) { Copy(); e.Handled(true); }
        else if ((shift && (key == VirtualKey::Left || key == VirtualKey::Right || key == VirtualKey::Up || key == VirtualKey::Down))
                 || (shift && ctrl && (key == VirtualKey::Home || key == VirtualKey::End)))
        {
            ExtendSelection(key, ctrl);
            e.Handled(true);
        }
    }

    Pos Caret()
    {
        auto focused = RowOf(Input::FocusManager::GetFocusedElement(root.XamlRoot()));
        int index = focused ? focused->item : -1;
        if (index < 0)
        {
            index = model->ItemCount();
            for (auto row : bound) if (row->item >= 0) index = std::min(index, row->item);
            if (index >= model->ItemCount()) index = 0;
        }
        auto& item = model->Item(index);
        return { item.block, item.start };
    }

    void ExtendSelection(Windows::System::VirtualKey key, bool ctrl)
    {
        using Windows::System::VirtualKey;
        auto anchor = selection ? selection->first : Caret();
        auto active = selection ? selection->second : anchor;
        if (key == VirtualKey::Home && ctrl) active = model->Start();
        else if (key == VirtualKey::End && ctrl) active = model->End();
        else if (key == VirtualKey::Left) active = active.offset > 0 ? Pos{ active.block, model->Boundary(active.block, active.offset - 1) } : active.block > 0 ? Pos{ active.block - 1, static_cast<uint32_t>(model->Block(active.block - 1).text.len) } : active;
        else if (key == VirtualKey::Right) active = active.offset < model->Block(active.block).text.len ? Pos{ active.block, model->NextBoundary(active.block, active.offset) } : active.block + 1 < model->BlockCount() ? Pos{ active.block + 1, 0 } : active;
        else active = ByFragment(active, key == VirtualKey::Up ? -1 : 1);
        SetSelection(std::pair{ anchor, active });
        JumpTo(model->ItemFor(active), true, false);
    }

    Pos ByFragment(Pos p, int step)
    {
        auto index = model->ItemFor(p);
        auto& current = model->Item(index);
        auto target = std::clamp(index + step, 0, model->ItemCount() - 1);
        if (target == index) return step < 0 ? Pos{ current.block, current.start } : Pos{ current.block, current.end };
        auto& item = model->Item(target);
        auto offset = std::min(item.start + (p.offset - current.start), item.end);
        return { item.block, model->Boundary(item.block, offset) };
    }

    void SelectAll() { SetSelection(std::pair{ model->Start(), model->End() }); }

    /// Rendered text as CF_UNICODETEXT and the covered Markdown as text/markdown.
    std::optional<std::pair<std::string, size_t>> Copy()
    {
        if (!selection) return std::nullopt;
        auto [a, b] = Ordered(selection->first, selection->second);
        auto plain = model->TextBetween(a, b);
        auto markdown = model->SourceBetween(a, b);
        HWND hwnd{};
        window.as<IWindowNative>()->get_WindowHandle(&hwnd);
        NativeClipboard::Set(hwnd, Wide(plain), markdown);
        return std::pair{ std::move(plain), markdown.size() };
    }

    // ---- Find, outline and links ----

    std::pair<size_t, double> RunFind(std::string const& query)
    {
        auto started = Qpc();
        matches = model->Find(query);
        lastQuery = query;
        matchIndex = -1;
        return { matches.size(), Ms(started, Qpc()) };
    }

    std::optional<f8_match> FindNext(bool forward, bool jump = true)
    {
        auto query = Narrow(search.Text());
        if (query != lastQuery) RunFind(query);
        if (matches.empty()) { status.Text(lastQuery.empty() ? L"" : L"No matches"); return std::nullopt; }
        int n = static_cast<int>(matches.size());
        matchIndex = matchIndex < 0 ? (forward ? 0 : n - 1) : (matchIndex + (forward ? 1 : n - 1)) % n;
        auto m = matches[matchIndex];
        SetSelection(std::pair{ Pos{ m.block, m.start }, Pos{ m.block, m.end } });
        status.Text(std::format(L"{} of {}", matchIndex + 1, n));
        if (jump) JumpTo(model->ItemFor({ m.block, m.start }), false, false);
        return m;
    }

    IAsyncAction JumpToBlock(uint32_t block, bool focus)
    {
        status.Text(std::format(L"Item {} of {}", model->doc.first_item[block] + 1, model->ItemCount()));
        for (uint64_t i = 0; i < model->doc.outline_count; i++)
            if (model->doc.outline[i].block == block)
            {
                if (outline.SelectedIndex() != static_cast<int>(i)) { suppressOutline = true; outline.SelectedIndex(static_cast<int>(i)); suppressOutline = false; }
                status.Text(std::format(L"Heading {} of {}, item {} of {}", i + 1, model->doc.outline_count, model->doc.first_item[block] + 1, model->ItemCount()));
                break;
            }
        return JumpTo(static_cast<int>(model->doc.first_item[block]), focus, true);
    }

    Row* Container(int item)
    {
        auto row = RowOf(list.ContainerFromIndex(item));
        return row && row->item == item ? row : nullptr;
    }

    /// Scroll an item into view; with `align`, put its top at the viewport top
    /// unless the document end prevents it.
    IAsyncAction JumpTo(int item, bool focus, bool align)
    {
        auto boxed = box_value(item);
        auto alignment = align ? ScrollIntoViewAlignment::Leading : ScrollIntoViewAlignment::Default;
        list.ScrollIntoView(boxed, alignment);
        Row* row = nullptr;
        for (int attempt = 0; attempt < 12; attempt++)
        {
            co_await NextFrame{};
            row = Container(item);
            if (!row) { list.ScrollIntoView(boxed, alignment); continue; }
            double top = row->container.TransformToVisual(scroll).TransformPoint({ 0, 0 }).Y;
            if (align && std::abs(top) > 0.5 && !(top > 0 && scroll.VerticalOffset() >= scroll.ScrollableHeight() - 0.5)) { ScrollBy(top); continue; }
            // Realized but outside the viewport: correct by the measured offset.
            if (!align && top < -0.5) { ScrollBy(top); continue; }
            double overflow = top + std::min(row->container.ActualHeight(), scroll.ViewportHeight()) - scroll.ViewportHeight();
            if (!align && overflow > 0.5) { ScrollBy(overflow); continue; }
            break;
        }
        if (focus && row) row->container.Focus(FocusState::Programmatic);
        jumped = row;
    }

    void ActivateLink(uint32_t index)
    {
        auto& link = model->doc.links[index];
        auto dest = std::string(Model::View(link.dest));
        auto detail = std::string(Model::View(link.detail));
        auto intent = IntentNames[std::min<uint32_t>(link.intent, 4)];
        decisions.push_back(std::format("{{\"dest\":{},\"intent\":\"{}\",\"detail\":{}}}", Quote(dest), intent, Quote(detail)));
        Emit(std::format("{{\"event\":\"link\",\"dest\":{},\"intent\":\"{}\"}}", Quote(dest), intent));
        switch (link.intent)
        {
        case F8_LINK_FRAGMENT: JumpToBlock(link.target_block, true); break;
        case F8_LINK_EXTERNAL:
            if (!config.automated) Windows::System::Launcher::LaunchUriAsync(Windows::Foundation::Uri(Wide(dest)));
            else status.Text(Wide("Would open " + dest));
            break;
        case F8_LINK_LOCAL_DOCUMENT: status.Text(Wide("Local document " + detail + " (opening is not implemented in the spike)")); break;
        default: status.Text(Wide(std::string("Link not followed: ") + intent + " " + detail)); break;
        }
    }

    Row* VisibleRow(int item)
    {
        auto row = Container(item);
        if (!row || row->container.ActualHeight() <= 0 || (!row->shown.empty() && row->text.ActualWidth() <= 0)) return nullptr;
        double top = row->container.TransformToVisual(scroll).TransformPoint({ 0, 0 }).Y;
        return top + row->container.ActualHeight() > 0 && top < scroll.ViewportHeight() ? row : nullptr;
    }

    Presented PresentedWhen(std::function<bool()> condition) { return Presented{ std::move(condition), Queue() }; }

    // ---- Scripted checks (ported from the GTK host) ----

    fire_and_forget RunAutomated(long long paint, std::optional<long long> presented)
    {
        std::string report = std::format(
            "{{\"host\":\"C++/WinRT\",\"input\":{},\"source_bytes\":{},\"blocks\":{},\"items\":{},\"launch_from_launcher\":{},"
            "\"launch_to_main_ms\":{:.3f},\"launch_to_window_loaded_ms\":{:.3f},\"launch_to_parsed_ms\":{:.3f},\"launch_to_paint_ms\":{:.3f},"
            "\"launch_to_presented_ms\":{:.3f},\"presentation_time_available\":{},\"read_ms\":{:.3f},\"parse_ms\":{:.3f},\"realized_rows_at_launch\":{}",
            Quote(Narrow(std::filesystem::path(config.input).filename().wstring())), model->doc.source.len, model->doc.block_count, model->doc.item_count,
            Bool(FromLauncher), Ms(LaunchAt, MainAt), Ms(LaunchAt, windowAt), Ms(LaunchAt, parsedAt), Ms(LaunchAt, paint),
            Ms(LaunchAt, presented.value_or(paint)), Bool(presented.has_value()), readMs, parseMs, RealizedRows());
        try
        {
            co_await Checks(report);
        }
        catch (hresult_error const& error) { report += ",\"error\":" + Quote(Narrow(error.message())); }
        catch (std::exception const& error) { report += ",\"error\":" + Quote(error.what()); }
        std::string links;
        for (auto& d : decisions) links += (links.empty() ? "" : ",") + d;
        report += std::format(",\"presented_timeouts\":{},\"realized_rows_at_end\":{},\"peak_rss_kib\":{},\"link_decisions\":[{}]}}", timedOut, RealizedRows(), PeakWorkingSetKiB(), links);
        if (!config.report.empty()) { std::ofstream out{ std::filesystem::path(config.report), std::ios::binary }; out << report; }
        Write("REPORT " + report + "\n");
        Application::Current().Exit();
    }

    int RealizedRows() { return static_cast<int>(std::count_if(bound.begin(), bound.end(), [](Row* r) { return r->item >= 0; })); }
    int HighlightedRows() { return static_cast<int>(std::count_if(bound.begin(), bound.end(), [](Row* r) { return r->item >= 0 && r->text.TextHighlighters().Size() > 0; })); }

    IAsyncAction WaitPresented(int item, double& ms, Row*& row, long long requested)
    {
        auto done = co_await PresentedWhen([this, item] { return VisibleRow(item) != nullptr; });
        if (done.timedOut) timedOut++;
        ms = Ms(requested, done.Best());
        row = VisibleRow(item);
    }

    IAsyncAction JumpPresented(int item, bool focus, bool align, double& ms, Row*& row)
    {
        auto requested = Qpc();
        co_await JumpTo(item, focus, align);
        co_await WaitPresented(item, ms, row, requested);
    }

    static std::string Shown(Row* row) { return row ? Narrow(row->shown) : std::string{}; }

    IAsyncAction Checks(std::string& report)
    {
        Emit("{\"event\":\"stage\",\"stage\":\"highlight\"}");
        {
            auto& first = model->Item(0);
            auto last = model->Item(std::min(3, model->ItemCount() - 1)).block;
            SetSelection(std::pair{ Pos{ first.block, 0 }, Pos{ last, model->NextBoundary(last, 0) } });
            auto selected = HighlightedRows();
            SetSelection(std::nullopt);
            auto cleared = HighlightedRows();
            auto b = model->Item(std::min(6, model->ItemCount() - 1)).block;
            SetSelection(std::pair{ Pos{ b, 0 }, Pos{ b, model->NextBoundary(b, 0) } });
            auto replaced = HighlightedRows();
            SetSelection(std::nullopt);
            report += std::format(",\"highlight_rows\":{{\"selected\":{},\"after_clear\":{},\"after_reselect_one_block\":{}}}", selected, cleared, replaced);
        }

        Emit("{\"event\":\"stage\",\"stage\":\"copy\"}");
        auto started = Qpc();
        SelectAll();
        auto copied = Copy();
        if (!copied) throw std::runtime_error("copy failed");
        auto& expected = copied->first;
        auto copyMs = Ms(started, Qpc());
        hstring readback;
        for (int attempt = 0;; attempt++)
        {
            try { readback = co_await Windows::ApplicationModel::DataTransfer::Clipboard::GetContent().GetTextAsync(); break; }
            catch (hresult_error const& error) { if (error.code() != CLIPBRD_E_CANT_OPEN || attempt > 50) throw; }
            co_await Delay{ Queue(), 20 };
        }
        auto markdownBack = NativeClipboard::Read(NativeClipboard::MarkdownFormat());
        auto source = Model::View(model->doc.source);
        report += std::format(",\"select_all_copy\":{{\"copy_ms\":{:.3f},\"plain_chars\":{},\"plain_bytes\":{},\"markdown_bytes\":{},\"markdown_equals_source\":{},\"local_readback_equals_model\":{},\"local_markdown_readback_equals_source\":{}}}",
            copyMs, Utf16Count(expected), expected.size(), copied->second, Bool(copied->second == source.size()), Bool(Narrow(readback) == expected),
            Bool(markdownBack && *markdownBack == source));

        Emit("{\"event\":\"stage\",\"stage\":\"samples\"}");
        std::string samples;
        for (double fraction : { 0.1, 0.35, 0.6, 0.85, 1.0 })
        {
            int index = static_cast<int>((model->ItemCount() - 1) * fraction);
            double ms = 0; Row* row = nullptr;
            co_await JumpPresented(index, false, false, ms, row);
            auto& item = model->Item(index);
            auto want = model->Fragment(index);
            auto at = model->PlainOffset({ item.block, item.start });
            samples += std::format("{}{{\"item\":{},\"jump_to_presented_ms\":{:.3f},\"displayed_equals_model\":{},\"copy_contains_at_offset\":{}}}",
                samples.empty() ? "" : ",", index, ms, Bool(Shown(row) == want), Bool(at + want.size() <= expected.size() && std::string_view(expected).substr(at, want.size()) == want));
        }
        report += ",\"samples\":[" + samples + "]";
        SetSelection(std::nullopt);

        Emit("{\"event\":\"stage\",\"stage\":\"find\"}");
        for (uint32_t block = model->BlockCount() * 92 / 100; block < model->BlockCount(); block++)
        {
            auto& b = model->Block(block);
            auto text = model->Text(block);
            auto cont = [&](size_t i) { return (static_cast<unsigned char>(text[i]) & 0xC0) == 0x80; };
            if (b.kind != KParagraph || text.size() <= 60 || cont(20) || cont(44)) continue;
            auto query = std::string(text.substr(20, 24));
            search.Text(Wide(query));
            auto [count, findMs] = RunFind(query);
            auto first = std::find_if(matches.begin(), matches.end(), [&](f8_match const& m) { return m.block >= block; }) - matches.begin();
            matchIndex = first == 0 ? static_cast<int>(matches.size()) - 1 : static_cast<int>(first) - 1;
            auto m = FindNext(true, false);
            if (!m) throw std::runtime_error("no match");
            auto target = model->ItemFor({ m->block, m->start });
            double ms = 0; Row* row = nullptr;
            co_await JumpPresented(target, false, false, ms, row);
            auto shown = Shown(row), lowerQuery = query;
            std::transform(shown.begin(), shown.end(), shown.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
            std::transform(lowerQuery.begin(), lowerQuery.end(), lowerQuery.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
            bool isMatch = selection && selection->first == Pos{ m->block, m->start } && selection->second == Pos{ m->block, m->end };
            report += std::format(",\"find\":{{\"query_bytes\":{},\"matches\":{},\"find_ms\":{:.3f},\"target_block\":{},\"jump_to_presented_ms\":{:.3f},\"presented_row_contains_query\":{},\"selection_is_match\":{}}}",
                query.size(), count, findMs, m->block, ms, Bool(shown.find(lowerQuery) != std::string::npos), Bool(isMatch));
            break;
        }

        Emit("{\"event\":\"stage\",\"stage\":\"outline\"}");
        std::string jumps;
        auto outlines = static_cast<int>(model->doc.outline_count);
        if (outlines > 0)
        {
            for (int entry : { outlines - 1, outlines / 2, 0 })
            {
                auto& heading = model->doc.outline[entry];
                int item = static_cast<int>(model->doc.first_item[heading.block]);
                auto requested = Qpc();
                co_await JumpToBlock(heading.block, true);
                double ms = 0; Row* row = nullptr;
                co_await WaitPresented(item, ms, row, requested);
                auto focused = Input::FocusManager::GetFocusedElement(root.XamlRoot());
                bool isFocused = row && focused && focused == row->container;
                auto top = row ? std::format("{}", row->container.TransformToVisual(scroll).TransformPoint({ 0, 0 }).Y) : std::string("null");
                jumps += std::format("{}{{\"entry\":{},\"block\":{},\"jump_to_presented_ms\":{:.3f},\"heading_presented\":{},\"row_focused\":{},\"row_top_px\":{}}}",
                    jumps.empty() ? "" : ",", entry, heading.block, ms, Bool(Shown(row) == Model::View(heading.title)), Bool(isFocused), top);
            }
        }
        report += ",\"outline_jumps\":[" + jumps + "]";

        Emit("{\"event\":\"stage\",\"stage\":\"links\"}");
        for (uint32_t i = 0; i < std::min<uint64_t>(40, model->doc.link_count); i++)
        {
            ActivateLink(i);
            auto& link = model->doc.links[i];
            if (link.intent == F8_LINK_FRAGMENT)
            {
                double ms = 0; Row* row = nullptr;
                co_await JumpPresented(static_cast<int>(model->doc.first_item[link.target_block]), true, true, ms, row);
                report += std::format(",\"fragment_link\":{{\"dest\":{},\"jump_to_presented_ms\":{:.3f},\"presented\":{}}}", Quote(Model::View(link.dest)), ms, Bool(row != nullptr));
            }
        }

        Emit("{\"event\":\"stage\",\"stage\":\"images\"}");
        std::string imageReports;
        for (uint32_t block = 0, seen = 0; block < model->BlockCount() && seen < 20; block++)
        {
            auto& b = model->Block(block);
            if (b.kind != KImage) continue;
            seen++;
            int item = static_cast<int>(model->doc.first_item[block]);
            auto request = std::string(Model::View(b.image));
            bool local = b.image_kind == F8_IMAGE_LOCAL;
            double ms = 0; Row* row = nullptr;
            co_await JumpPresented(item, false, false, ms, row);
            std::optional<std::string> outcome;
            for (int i = 0; i < 200 && !outcome; i++)
            {
                if (!local) outcome = "blocked by model policy: " + request;
                else if (auto found = images.find(request); found != images.end() && found->second.Done())
                    outcome = found->second.bitmap ? std::format("presented {}x{}", found->second.bitmap.PixelWidth(), found->second.bitmap.PixelHeight()) : "refused: " + *found->second.failure;
                else co_await Delay{ Queue(), 10 };
            }
            co_await WaitPresented(item, ms, row, Qpc());
            bool visible = row && row->picture.Visibility() == Visibility::Visible && row->picture.Source();
            imageReports += std::format("{}{{\"alt\":{},\"request\":{},\"outcome\":{},\"picture_visible\":{}}}", imageReports.empty() ? "" : ",",
                Quote(model->Text(block)), Quote(std::string(local ? "Local(" : "Blocked(") + request + ")"), outcome ? Quote(*outcome) : "null", Bool(visible));
        }
        report += ",\"images\":[" + imageReports + "]";
    }

    // ---- Control loop for the clipboard, UIA and Narrator probes ----

    fire_and_forget ControlLoop()
    {
        Emit("{\"event\":\"ready\"}");
        auto input = GetStdHandle(STD_INPUT_HANDLE);
        std::string buffer;
        while (true)
        {
            co_await resume_background();
            std::string line;
            bool eof = false;
            while (true)
            {
                auto newline = buffer.find('\n');
                if (newline != std::string::npos) { line = buffer.substr(0, newline); buffer.erase(0, newline + 1); break; }
                char chunk[4096]; DWORD read = 0;
                if (!ReadFile(input, chunk, sizeof(chunk), &read, nullptr) || read == 0) { eof = true; break; }
                buffer.append(chunk, read);
            }
            co_await ResumeUi();
            if (eof) break;
            if (!line.empty() && line.back() == '\r') line.pop_back();
            if (!co_await Command(line)) break;
        }
        Application::Current().Exit();
    }

    IAsyncOperation<bool> Command(std::string line)
    {
        std::vector<std::string> parts;
        for (size_t start = 0; start < line.size();)
        {
            auto end = line.find(' ', start);
            if (end == std::string::npos) end = line.size();
            if (end > start) parts.push_back(line.substr(start, end - start));
            start = end + 1;
        }
        auto arg = [&](size_t i, int fallback) { return parts.size() > i ? std::atoi(parts[i].c_str()) : fallback; };
        auto name = parts.empty() ? std::string{} : parts[0];
        if (name == "copy-all")
        {
            SelectAll();
            auto copied = Copy();
            Emit(std::format("{{\"event\":\"copied\",\"plain_chars\":{},\"plain_bytes\":{},\"markdown_bytes\":{}}}", Utf16Count(copied->first), copied->first.size(), copied->second));
        }
        else if (name == "item" || name == "focus")
        {
            double ms = 0; Row* row = nullptr;
            co_await JumpPresented(arg(1, 0), name == "focus", name == "focus", ms, row);
            Emit(std::format("{{\"event\":\"presented\",\"item\":{},\"jump_to_presented_ms\":{:.3f}}}", arg(1, 0), ms));
        }
        else if (name == "outline")
        {
            auto block = model->doc.outline[arg(1, 0)].block;
            int item = static_cast<int>(model->doc.first_item[block]);
            co_await JumpToBlock(block, true);
            double ms = 0; Row* row = nullptr;
            co_await WaitPresented(item, ms, row, Qpc());
            Emit(std::format("{{\"event\":\"presented\",\"item\":{}}}", item));
        }
        else if (name == "texts")
        {
            int from = arg(1, 0), to = std::min(arg(2, from + 1), model->ItemCount());
            std::string texts;
            for (int i = from; i < to; i++) texts += (texts.empty() ? "" : ",") + Quote(model->Fragment(i));
            Emit(std::format("{{\"event\":\"texts\",\"from\":{},\"texts\":[{}],\"items\":{}}}", from, texts, model->ItemCount()));
        }
        else if (name == "select")
        {
            SetSelection(std::pair{ Pos{ static_cast<uint32_t>(arg(1, 0)), static_cast<uint32_t>(arg(2, 0)) }, Pos{ static_cast<uint32_t>(arg(3, 0)), static_cast<uint32_t>(arg(4, 0)) } });
            Emit("{\"event\":\"selected\"}");
        }
        else if (name == "quit") co_return false;
        else Emit("{\"event\":\"error\",\"line\":" + Quote(line) + "}");
        co_return true;
    }
};

// ---- Application ----

struct HostApp : ApplicationT<HostApp, Markup::IXamlMetadataProvider>
{
    XamlTypeInfo::XamlControlsXamlMetaDataProvider provider;
    std::unique_ptr<DocumentWindow> document;
    auto GetXamlType(hstring const& name) { return provider.GetXamlType(name); }
    auto GetXamlType(Windows::UI::Xaml::Interop::TypeName const& type) { return provider.GetXamlType(type); }
    auto GetXmlnsDefinitions() { return provider.GetXmlnsDefinitions(); }

    void OnLaunched(LaunchActivatedEventArgs const&)
    {
        Resources().MergedDictionaries().Append(XamlControlsResources());
        UnhandledException([this](auto&&, UnhandledExceptionEventArgs const& e) {
            e.Handled(true);
            if (document) document->Fail(Narrow(e.Message()));
        });
        document = std::make_unique<DocumentWindow>();
        document->window.Activate();
    }
};

static Application application{ nullptr };

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int)
{
    MainAt = Qpc();
    wchar_t launch[64]{};
    if (GetEnvironmentVariableW(L"FEAS008_LAUNCH_QPC", launch, 64)) { LaunchAt = _wtoll(launch); FromLauncher = true; }
    else
    {
        FILETIME created, exited, kernel, user, now;
        GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user);
        GetSystemTimeAsFileTime(&now);
        auto age100ns = (ULARGE_INTEGER{ now.dwLowDateTime, now.dwHighDateTime }.QuadPart) - (ULARGE_INTEGER{ created.dwLowDateTime, created.dwHighDateTime }.QuadPart);
        LaunchAt = MainAt - static_cast<long long>(age100ns * Frequency / 10'000'000);
    }
    wchar_t fragment[32]{};
    if (GetEnvironmentVariableW(L"FEAS008_FRAGMENT_BYTES", fragment, 32)) config.fragmentBytes = _wtoll(fragment);
    int count = 0;
    auto args = CommandLineToArgvW(GetCommandLineW(), &count);
    for (int i = 1; i < count; i++)
    {
        std::wstring arg = args[i];
        if (arg == L"--automated") config.automated = true;
        else if (arg == L"--control") config.control = true;
        else if (arg == L"--report" && i + 1 < count) config.report = args[++i];
        else config.input = std::filesystem::absolute(arg).wstring();
    }
    LocalFree(args);
    if (config.input.empty()) return 2;
    // Read and parse on a worker while WinUI starts, as the C# and GTK hosts do.
    loader = std::thread([] {
        try
        {
            auto started = Qpc();
            std::ifstream stream{ std::filesystem::path(config.input), std::ios::binary };
            std::string bytes((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
            readMs = Ms(started, Qpc());
            auto loaded = std::make_unique<Model>();
            if (f8_open_v1(reinterpret_cast<uint8_t const*>(bytes.data()), bytes.size(), config.fragmentBytes, &loaded->doc) != F8_OK) throw std::runtime_error("f8_open_v1 failed");
            parseMs = loaded->doc.parse_ns / 1e6;
            parsedAt = Qpc();
            model = std::move(loaded);
        }
        catch (...) { loadError = std::current_exception(); }
    });
    try
    {
        check_hresult(MddBootstrapInitialize2(0x00020005, nullptr, PACKAGE_VERSION{}, MddBootstrapInitializeOptions_None));
        init_apartment(apartment_type::single_threaded);
        Application::Start([](auto const&) { application = make<HostApp>(); });
        if (loader.joinable()) loader.join();
        MddBootstrapShutdown();
        return 0;
    }
    catch (hresult_error const& error)
    {
        Write(std::format("EVENT {{\"event\":\"error\",\"hresult\":{}}}\n", error.code().value));
        return 1;
    }
}
