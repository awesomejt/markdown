using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Markdown.Windows;

internal static class Program
{
    internal static bool Smoke;
    internal static int Result = 1;

    [STAThread]
    private static int Main(string[] args)
    {
        Smoke = args.SequenceEqual(new[] { "--smoke" });
        if (args.Length != 0 && !Smoke) return 2;
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(initialization =>
        {
            SynchronizationContext.SetSynchronizationContext(
                new Microsoft.UI.Dispatching.DispatcherQueueSynchronizationContext(
                    Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread()));
            _ = new ReaderApp();
        });
        return Smoke ? Result : 0;
    }
}

public sealed partial class ReaderApp : Application
{
    private readonly List<ReaderWindow> windows = new();
    public ReaderApp() => InitializeComponent();

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var window = NewWindow();
        if (Program.Smoke) ((FrameworkElement)window.Content).Loaded += (_, _) => RunSmoke(window);
    }

    internal ReaderWindow NewWindow()
    {
        var window = new ReaderWindow(this);
        windows.Add(window);
        window.Closed += (_, _) => windows.Remove(window);
        window.Activate();
        return window;
    }

    internal void CloseAll()
    {
        foreach (var window in windows.ToArray()) window.Close();
    }

    private void RunSmoke(ReaderWindow first)
    {
        // Exercise the same command delegates used by menus and accelerators.
        try
        {
            if (windows.Count != 1 || first.Title != "Markdown" ||
                AutomationProperties.GetName(first.Welcome) != "Markdown reader")
                throw new InvalidOperationException("Initial shell contract failed.");
            first.NewCommand();
            if (windows.Count != 2) throw new InvalidOperationException("New Window failed.");
            windows[1].CloseCommand();
            if (windows.Count != 1) throw new InvalidOperationException("Close failed.");
            first.NewCommand();
            Program.Result = 0;
            first.ExitCommand();
            if (windows.Count != 0) Program.Result = 1;
        }
        catch
        {
            Program.Result = 1;
            CloseAll();
            Exit();
        }
    }
}

internal sealed class ReaderWindow : Window
{
    internal Action NewCommand { get; }
    internal Action CloseCommand { get; }
    internal Action ExitCommand { get; }
    internal TextBlock Welcome { get; }

    internal ReaderWindow(ReaderApp app)
    {
        Title = "Markdown";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(960, 700));
        NewCommand = () => app.NewWindow();
        CloseCommand = Close;
        ExitCommand = app.CloseAll;

        var root = new Grid();
        var file = new MenuBarItem { Title = "File", AccessKey = "F" };
        file.Items.Add(Command(root, "New Window", "Ctrl+N", VirtualKey.N, VirtualKeyModifiers.Control, NewCommand));
        file.Items.Add(Command(root, "Close Window", "Ctrl+W", VirtualKey.W, VirtualKeyModifiers.Control, CloseCommand));
        file.Items.Add(new MenuFlyoutSeparator());
        file.Items.Add(Command(root, "Exit", "Ctrl+Shift+W", VirtualKey.W,
            VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, ExitCommand));
        var menu = new MenuBar();
        menu.Items.Add(file);
        Welcome = new TextBlock
        {
            Text = "Markdown\n\nA native Markdown reader.\nFile opening will be available in a later increment.",
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(32),
            FontSize = 20
        };
        AutomationProperties.SetName(Welcome, "Markdown reader");
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition());
        root.Children.Add(menu);
        Grid.SetRow(Welcome, 1);
        root.Children.Add(Welcome);
        Content = root;
    }

    private static MenuFlyoutItem Command(Grid root, string text, string shortcut, VirtualKey key,
        VirtualKeyModifiers modifiers, Action invoke)
    {
        var item = new MenuFlyoutItem { Text = text, KeyboardAcceleratorTextOverride = shortcut };
        item.Click += (_, _) => invoke();
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = modifiers };
        accelerator.Invoked += (_, args) => { args.Handled = true; invoke(); };
        root.KeyboardAccelerators.Add(accelerator);
        return item;
    }
}
