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
    private static PickerAutomationStep? s_activeStep;

    internal static void EnqueueAutomationCancel() =>
        s_automationQueue.Enqueue(new PickerAutomationStep(PickerAutomationAction.Cancel));

    internal static void EnqueueAutomationSelect(string fullPath) =>
        s_automationQueue.Enqueue(new PickerAutomationStep(PickerAutomationAction.SelectPath, fullPath));

    public static string? PickFile(Window ownerWindow)
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(ownerWindow);
        return PickFileForHwnd(hwnd);
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
}
