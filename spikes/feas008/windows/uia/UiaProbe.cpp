// FEAS-008 UIA3 inspection of a running Windows host, the client API Narrator
// uses. Prints one JSON object: the document list's patterns, how long its
// first-child and sibling walk take, each exposed row (name, control type,
// heading level, position and size of set, item type), and whether the
// ItemContainer pattern finds an item that is not realized.
// Usage: UiaProbe.exe PID [MAX_ROWS] [ITEM_WALK_LIMIT]
#define NOMINMAX
#include <windows.h>
#include <uiautomation.h>
#include <chrono>
#include <cstdio>
#include <string>

template <class T> struct Com
{
    T* p = nullptr;
    ~Com() { if (p) p->Release(); }
    T** operator&() { return &p; }
    T* operator->() const { return p; }
    explicit operator bool() const { return p != nullptr; }
};

static std::string Json(BSTR value)
{
    std::string out = "\"";
    if (value)
        for (wchar_t const* c = value; *c; c++)
        {
            if (*c == L'"' || *c == L'\\') { out += '\\'; out += static_cast<char>(*c); }
            else if (*c < 0x20) { char b[8]; sprintf_s(b, "\\u%04x", *c); out += b; }
            else if (*c < 0x80) out += static_cast<char>(*c);
            else
            {
                char b[8]{}; int n = WideCharToMultiByte(CP_UTF8, 0, c, IS_HIGH_SURROGATE(*c) && c[1] ? 2 : 1, b, 8, nullptr, nullptr);
                out.append(b, n);
                if (IS_HIGH_SURROGATE(*c) && c[1]) c++;
            }
        }
    return out + "\"";
}

static int IntProperty(IUIAutomationElement* e, PROPERTYID id)
{
    VARIANT v; VariantInit(&v);
    int out = -1;
    if (SUCCEEDED(e->GetCurrentPropertyValue(id, &v)) && v.vt == VT_I4) out = v.lVal;
    VariantClear(&v);
    return out;
}

static std::string Row(IUIAutomationTreeWalker* walker, IUIAutomationElement* e, int maxName)
{
    BSTR name = nullptr, type = nullptr, itemType = nullptr;
    e->get_CurrentName(&name);
    e->get_CurrentLocalizedControlType(&type);
    e->get_CurrentItemType(&itemType);
    std::wstring shortName = name ? std::wstring(name).substr(0, maxName) : L"";
    BSTR clipped = SysAllocString(shortName.c_str());
    std::string out = "{\"name\":" + Json(clipped) + ",\"type\":" + Json(type) + ",\"item_type\":" + Json(itemType)
        + ",\"heading_level\":" + std::to_string(IntProperty(e, UIA_HeadingLevelPropertyId) - HeadingLevel_None)
        + ",\"position\":" + std::to_string(IntProperty(e, UIA_PositionInSetPropertyId))
        + ",\"size\":" + std::to_string(IntProperty(e, UIA_SizeOfSetPropertyId));
    Com<IUIAutomationElement> child;
    walker->GetFirstChildElement(e, &child);
    out += ",\"child_heading_level\":" + std::to_string(child ? IntProperty(child.p, UIA_HeadingLevelPropertyId) - HeadingLevel_None : -1) + "}";
    SysFreeString(clipped); SysFreeString(name); SysFreeString(type); SysFreeString(itemType);
    return out;
}

int wmain(int argc, wchar_t** argv)
{
    if (argc < 2) return 2;
    DWORD pid = wcstoul(argv[1], nullptr, 10);
    int maxRows = argc > 2 ? _wtoi(argv[2]) : 60;
    int walkLimit = argc > 3 ? _wtoi(argv[3]) : 200000;
    CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    Com<IUIAutomation> uia;
    if (FAILED(CoCreateInstance(CLSID_CUIAutomation8, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&uia)))) return 3;
    struct Find { DWORD pid; HWND hwnd; } find{ pid, nullptr };
    EnumWindows([](HWND h, LPARAM l) -> BOOL {
        auto f = reinterpret_cast<Find*>(l); DWORD owner = 0; GetWindowThreadProcessId(h, &owner);
        if (owner == f->pid && IsWindowVisible(h)) { f->hwnd = h; return FALSE; }
        return TRUE; }, reinterpret_cast<LPARAM>(&find));
    if (!find.hwnd) { printf("{\"error\":\"window not found\"}\n"); return 4; }
    Com<IUIAutomationElement> window;
    uia->ElementFromHandle(find.hwnd, &window);
    VARIANT name; VariantInit(&name); name.vt = VT_BSTR; name.bstrVal = SysAllocString(L"Document");
    Com<IUIAutomationCondition> byName;
    uia->CreatePropertyCondition(UIA_NamePropertyId, name, &byName);
    VariantClear(&name);
    // Enter the list from the focused row, as a screen reader does. A
    // whole-window descendant search exceeds the UIA provider timeout on
    // 121k-item documents, so it is only the fallback.
    Com<IUIAutomationTreeWalker> walker;
    uia->get_ControlViewWalker(&walker);
    Com<IUIAutomationElement> list;
    auto searchStarted = std::chrono::steady_clock::now();
    std::string entry = "focused_row";
    {
        Com<IUIAutomationElement> focused;
        int focusedPid = 0;
        if (SUCCEEDED(uia->GetFocusedElement(&focused)) && focused && SUCCEEDED(focused->get_CurrentProcessId(&focusedPid)) && focusedPid == static_cast<int>(pid))
            walker->GetParentElement(focused.p, &list);
    }
    if (!list) { entry = "descendant_search"; window->FindFirst(TreeScope_Descendants, byName.p, &list); }
    auto searchMs = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - searchStarted).count();
    if (!list) { printf("{\"error\":\"document list not found\",\"find_document_ms\":%.1f}\n", searchMs); return 5; }
    BSTR listName = nullptr;
    list->get_CurrentName(&listName);
    std::string out = "{\"entry\":\"" + entry + "\",\"list_name\":" + Json(listName) + ",\"find_document_ms\":" + std::to_string(searchMs) + ",";
    SysFreeString(listName);
    int scroll = 0, container = 0;
    { Com<IUnknown> p; list->GetCurrentPattern(UIA_ScrollPatternId, &p); scroll = !!p; }
    { Com<IUnknown> p; list->GetCurrentPattern(UIA_ItemContainerPatternId, &p); container = !!p; }
    out += "\"scroll_pattern\":" + std::string(scroll ? "true" : "false") + ",\"item_container_pattern\":" + (container ? "true" : "false");

    auto started = std::chrono::steady_clock::now();
    Com<IUIAutomationElement> first;
    walker->GetFirstChildElement(list.p, &first);
    auto firstMs = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count();
    out += ",\"first_child_ms\":" + std::to_string(firstMs) + ",\"rows\":[";
    IUIAutomationElement* current = first.p;
    if (current) current->AddRef();
    int rows = 0;
    started = std::chrono::steady_clock::now();
    while (current && rows < maxRows)
    {
        out += (rows ? "," : "") + Row(walker.p, current, 60);
        IUIAutomationElement* next = nullptr;
        walker->GetNextSiblingElement(current, &next);
        current->Release();
        current = next;
        rows++;
    }
    if (current) current->Release();
    auto walkMs = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count();
    out += "],\"walk_ms\":" + std::to_string(walkMs);

    // Narrator's heading and virtualized navigation depend on finding items
    // that are not realized. Ask the list for the last item by position.
    if (container)
    {
        Com<IUIAutomationItemContainerPattern> items;
        list->GetCurrentPatternAs(UIA_ItemContainerPatternId, IID_PPV_ARGS(&items));
        VARIANT empty; VariantInit(&empty);
        Com<IUIAutomationElement> found;
        started = std::chrono::steady_clock::now();
        // Iterate with a null property: each call returns the next item after `start`.
        IUIAutomationElement* start = nullptr;
        int seen = 0, virtualized = 0;
        for (; seen < walkLimit; seen++)
        {
            if (std::chrono::steady_clock::now() - started > std::chrono::seconds(30)) break;
            IUIAutomationElement* item = nullptr;
            if (FAILED(items->FindItemByProperty(start, 0, empty, &item)) || !item) break;
            Com<IUIAutomationVirtualizedItemPattern> v;
            if (SUCCEEDED(item->GetCurrentPatternAs(UIA_VirtualizedItemPatternId, IID_PPV_ARGS(&v))) && v) virtualized++;
            if (start) start->Release();
            start = item;
        }
        if (start) start->Release();
        auto findMs = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count();
        out += ",\"item_container_items\":" + std::to_string(seen) + ",\"virtualized_items\":" + std::to_string(virtualized)
            + ",\"item_container_walk_ms\":" + std::to_string(findMs);
    }
    printf("%s}\n", out.c_str());
    CoUninitialize();
    return 0;
}
