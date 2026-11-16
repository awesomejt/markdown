using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Markdown.Windows.Document;
using Microsoft.UI.Xaml;

namespace Markdown.Windows;

internal enum PickerAutomationAction
{
    Cancel,
    SelectPath,
}

internal sealed record PickerAutomationStep(
    PickerAutomationAction Action,
    string? Path = null);

internal static unsafe class DocumentPicker
{
    private const uint OFN_READONLY = 0x00000001;
    private const uint OFN_HIDEREADONLY = 0x00000004;
    private const uint OFN_NOCHANGEDIR = 0x00000008;
    private const uint OFN_ENABLEHOOK = 0x00000020;
    private const uint OFN_PATHMUSTEXIST = 0x00000800;
    private const uint OFN_FILEMUSTEXIST = 0x00001000;
    private const uint OFN_EXPLORER = 0x00080000;
    private const uint OFN_DONTADDTORECENT = 0x02000000;

    private const uint WM_INITDIALOG = 0x0110;
    private const uint WM_COMMAND = 0x0111;

    private const int IDOK = 1;
    private const int IDCANCEL = 2;

    // Double-NUL terminated Win32 filter string:
    // "Markdown and text (*.md;*.markdown;*.mdown;*.mkd;*.txt)\0*.md;*.markdown;*.mdown;*.mkd;*.txt\0All files (*.*)\0*.*\0\0"
    internal const string FilterSpec =
        "Markdown and text (*.md;*.markdown;*.mdown;*.mkd;*.txt)\0*.md;*.markdown;*.mdown;*.mkd;*.txt\0All files (*.*)\0*.*\0\0";

    private static readonly Queue<PickerAutomationStep> s_automationQueue = new();
    private static readonly Queue<PickerAutomationStep> s_folderAutomationQueue = new();
    private static PickerAutomationStep? s_activeStep;

    internal static void EnqueueAutomationCancel() =>
        s_automationQueue.Enqueue(new PickerAutomationStep(PickerAutomationAction.Cancel));

    internal static void EnqueueAutomationSelect(string fullPath) =>
        s_automationQueue.Enqueue(new PickerAutomationStep(PickerAutomationAction.SelectPath, fullPath));

    internal static void EnqueueFolderAutomationCancel() =>
        s_folderAutomationQueue.Enqueue(new PickerAutomationStep(PickerAutomationAction.Cancel));

    internal static void EnqueueFolderAutomationSelect(string fullPath) =>
        s_folderAutomationQueue.Enqueue(new PickerAutomationStep(PickerAutomationAction.SelectPath, fullPath));

    public static string? PickFile(Window ownerWindow)
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(ownerWindow);
        return PickFileForHwnd(hwnd);
    }

    public static string? PickFolder(Window ownerWindow)
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(ownerWindow);
        return PickFolderForHwnd(hwnd);
    }

    public static string? PickFolderForHwnd(nint ownerHwnd)
    {
        if (s_folderAutomationQueue.Count > 0)
        {
            var step = s_folderAutomationQueue.Dequeue();
            return step.Action == PickerAutomationAction.SelectPath ? step.Path : null;
        }

        const uint CLSCTX_INPROC_SERVER = 0x1;
        const uint FOS_PICKFOLDERS = 0x00000020;
        const uint FOS_FORCEFILESYSTEM = 0x00000040;
        const uint FOS_NOCHANGEDIR = 0x00000008;
        const uint FOS_PATHMUSTEXIST = 0x00000800;
        const uint FOS_DONTADDTORECENT = 0x02000000;
        const uint SIGDN_FILESYSPATH = 0x80058000;

        var clsid = new Guid("DC1C5A9C-E88A-4dde-A5A1-60F82A20AEF7");
        var iid = new Guid("d57c7288-d4ad-4768-be02-9d969532d960");
        void* dialogPtr = null;
        var hr = CoCreateInstance(&clsid, null, CLSCTX_INPROC_SERVER, &iid, &dialogPtr);
        if (hr < 0 || dialogPtr is null)
        {
            return null;
        }

        try
        {
            var vtable = *(void***)dialogPtr;

            // IFileDialog::GetOptions (slot 10) & SetOptions (slot 9)
            uint options = 0;
            var getOptions = (delegate* unmanaged[Stdcall]<void*, uint*, int>)vtable[10];
            if (getOptions(dialogPtr, &options) >= 0)
            {
                options |= FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR | FOS_PATHMUSTEXIST | FOS_DONTADDTORECENT;
                var setOptions = (delegate* unmanaged[Stdcall]<void*, uint, int>)vtable[9];
                _ = setOptions(dialogPtr, options);
            }

            // IFileDialog::SetTitle (slot 17)
            fixed (char* titlePtr = "Open Folder Workspace\0")
            {
                var setTitle = (delegate* unmanaged[Stdcall]<void*, char*, int>)vtable[17];
                _ = setTitle(dialogPtr, titlePtr);
            }

            // IModalWindow::Show (slot 3)
            var show = (delegate* unmanaged[Stdcall]<void*, nint, int>)vtable[3];
            if (show(dialogPtr, ownerHwnd) != 0)
            {
                return null;
            }

            // IFileDialog::GetResult (slot 20)
            void* itemPtr = null;
            var getResult = (delegate* unmanaged[Stdcall]<void*, void**, int>)vtable[20];
            if (getResult(dialogPtr, &itemPtr) < 0 || itemPtr is null)
            {
                return null;
            }

            try
            {
                var itemVtable = *(void***)itemPtr;
                char* pathPtr = null;
                var getDisplayName = (delegate* unmanaged[Stdcall]<void*, uint, char**, int>)itemVtable[5];
                if (getDisplayName(itemPtr, SIGDN_FILESYSPATH, &pathPtr) < 0 || pathPtr is null)
                {
                    return null;
                }

                try
                {
                    return new string(pathPtr);
                }
                finally
                {
                    CoTaskMemFree(pathPtr);
                }
            }
            finally
            {
                var itemVtable = *(void***)itemPtr;
                var releaseItem = (delegate* unmanaged[Stdcall]<void*, uint>)itemVtable[2];
                _ = releaseItem(itemPtr);
            }
        }
        finally
        {
            var vtable = *(void***)dialogPtr;
            var releaseDialog = (delegate* unmanaged[Stdcall]<void*, uint>)vtable[2];
            _ = releaseDialog(dialogPtr);
        }
    }

    public static string? PickFileForHwnd(nint ownerHwnd)
    {
        const int maxChars = 32768;
        var fileBuffer = new char[maxChars];
        var filterChars = FilterSpec.ToCharArray();
        var titleChars = "Open Markdown Document\0".ToCharArray();

        s_activeStep = s_automationQueue.Count > 0 ? s_automationQueue.Dequeue() : null;
        if (s_activeStep is { Action: PickerAutomationAction.SelectPath, Path: { Length: > 0 } initialPath })
        {
            initialPath.AsSpan().CopyTo(fileBuffer);
            fileBuffer[initialPath.Length] = '\0';
        }

        try
        {
            fixed (char* filePtr = fileBuffer)
            fixed (char* filterPtr = filterChars)
            fixed (char* titlePtr = titleChars)
            {
                var ofn = new OPENFILENAMEW
                {
                    lStructSize = (uint)sizeof(OPENFILENAMEW),
                    hwndOwner = ownerHwnd,
                    lpstrFilter = filterPtr,
                    nFilterIndex = 1,
                    lpstrFile = filePtr,
                    nMaxFile = (uint)maxChars,
                    lpstrTitle = titlePtr,
                    Flags = OFN_EXPLORER |
                            OFN_PATHMUSTEXIST |
                            OFN_FILEMUSTEXIST |
                            OFN_HIDEREADONLY |
                            OFN_NOCHANGEDIR |
                            OFN_DONTADDTORECENT,
                };

                if (s_activeStep is not null)
                {
                    ofn.Flags = (ofn.Flags & ~OFN_EXPLORER) | OFN_ENABLEHOOK;
                    ofn.lpfnHook = &DialogHookProc;
                }

                var ok = GetOpenFileNameW(&ofn);
                if (!ok)
                {
                    return null;
                }

                var len = 0;
                while (len < maxChars && filePtr[len] != '\0')
                {
                    len++;
                }
                if (len == 0)
                {
                    return null;
                }

                return new string(filePtr, 0, len);
            }
        }
        finally
        {
            s_activeStep = null;
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nuint DialogHookProc(nint hdlg, uint uiMsg, nuint wParam, nint lParam)
    {
        if (uiMsg == WM_INITDIALOG)
        {
            var step = s_activeStep;
            s_activeStep = null;
            if (step is not null)
            {
                if (step.Action == PickerAutomationAction.Cancel)
                {
                    _ = PostMessageW(hdlg, WM_COMMAND, IDCANCEL, 0);
                }
                else if (step.Action == PickerAutomationAction.SelectPath)
                {
                    _ = PostMessageW(hdlg, WM_COMMAND, IDOK, 0);
                }
            }
            return 1;
        }
        return 0;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct OPENFILENAMEW
    {
        public uint lStructSize;
        public nint hwndOwner;
        public nint hInstance;
        public char* lpstrFilter;
        public char* lpstrCustomFilter;
        public uint nMaxCustFilter;
        public uint nFilterIndex;
        public char* lpstrFile;
        public uint nMaxFile;
        public char* lpstrFileTitle;
        public uint nMaxFileTitle;
        public char* lpstrInitialDir;
        public char* lpstrTitle;
        public uint Flags;
        public ushort nFileOffset;
        public ushort nFileExtension;
        public char* lpstrDefExt;
        public nint lCustData;
        public delegate* unmanaged[Stdcall]<nint, uint, nuint, nint, nuint> lpfnHook;
        public char* lpTemplateName;
        public void* pvReserved;
        public uint dwReserved;
        public uint FlagsEx;
    }

    [DllImport("comdlg32.dll", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetOpenFileNameW(OPENFILENAMEW* lpofn);

    [DllImport("user32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostMessageW(nint hWnd, uint Msg, nuint wParam, nint lParam);

    [DllImport("ole32.dll", ExactSpelling = true)]
    private static extern int CoCreateInstance(
        Guid* rclsid,
        void* pUnkOuter,
        uint dwClsContext,
        Guid* riid,
        void** ppv);

    [DllImport("ole32.dll", ExactSpelling = true)]
    private static extern void CoTaskMemFree(void* pv);
}
