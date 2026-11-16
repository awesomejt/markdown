using Markdown.Windows.Document;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace Markdown.Windows;

internal sealed class RecentItemsWindow : Window
{
    private readonly RecentsStore _recents;
    private readonly Action<RecentEntry> _openEntry;
    private readonly Action<RecentEntry, string> _relocateAndOpen;
    private readonly Action<RecentEntry> _revokeEntry;
    private readonly StackPanel _foldersPanel = new() { Spacing = 8 };
    private readonly StackPanel _filesPanel = new() { Spacing = 8 };
    private readonly TextBlock _emptyLabel = new()
    {
        Text = "No recent files or folders yet.",
        Opacity = 0.7,
        Visibility = Visibility.Collapsed,
    };
    private CancellationTokenSource? _checkCts;

    internal RecentItemsWindow(
        RecentsStore recents,
        Action<RecentEntry> openEntry,
        Action<RecentEntry, string> relocateAndOpen,
        Action<RecentEntry> revokeEntry)
    {
        _recents = recents;
        _openEntry = openEntry;
        _relocateAndOpen = relocateAndOpen;
        _revokeEntry = revokeEntry;

        Title = "Recent Items";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(620, 520));

        _recents.Changed += OnRecentsChanged;
        Closed += (_, _) =>
        {
            _recents.Changed -= OnRecentsChanged;
            _checkCts?.Cancel();
            _checkCts?.Dispose();
            _checkCts = null;
        };

        var root = new Grid
        {
            Padding = new Thickness(20),
            RowSpacing = 12,
        };
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition());
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var header = new TextBlock
        {
            Text = "Recent Workspaces and Documents",
            FontSize = 18,
        };
        AutomationProperties.SetHeadingLevel(header, Microsoft.UI.Xaml.Automation.Peers.AutomationHeadingLevel.Level1);
        root.Children.Add(header);

        var scroll = new ScrollViewer();
        var body = new StackPanel { Spacing = 16 };
        body.Children.Add(_emptyLabel);
        body.Children.Add(CreateSection("Recent Folders", _foldersPanel));
        body.Children.Add(CreateSection("Recent Files", _filesPanel));
        scroll.Content = body;
        Grid.SetRow(scroll, 1);
        root.Children.Add(scroll);

        var footer = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right,
            Spacing = 8,
        };
        var refreshBtn = new Button { Content = "Refresh Status" };
        refreshBtn.Click += (_, _) => Refresh();
        var clearBtn = new Button { Content = "Clear All" };
        clearBtn.Click += (_, _) => _recents.Clear();
        footer.Children.Add(refreshBtn);
        footer.Children.Add(clearBtn);
        Grid.SetRow(footer, 2);
        root.Children.Add(footer);

        Content = root;
        Refresh();
    }

    private void OnRecentsChanged()
    {
        _ = DispatcherQueue?.TryEnqueue(Refresh);
    }

    private static StackPanel CreateSection(string title, StackPanel itemsPanel)
    {
        var section = new StackPanel { Spacing = 8 };
        var heading = new TextBlock
        {
            Text = title,
            FontSize = 14,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        AutomationProperties.SetHeadingLevel(heading, Microsoft.UI.Xaml.Automation.Peers.AutomationHeadingLevel.Level2);
        section.Children.Add(heading);
        section.Children.Add(itemsPanel);
        return section;
    }

    internal void Refresh()
    {
        _checkCts?.Cancel();
        _checkCts?.Dispose();
        var cts = new CancellationTokenSource();
        _checkCts = cts;

        _foldersPanel.Children.Clear();
        _filesPanel.Children.Clear();

        var folders = _recents.Folders;
        var files = _recents.Files;
        _emptyLabel.Visibility = folders.Count == 0 && files.Count == 0 ? Visibility.Visible : Visibility.Collapsed;

        foreach (var entry in folders)
        {
            _foldersPanel.Children.Add(BuildRow(entry, cts.Token));
        }
        foreach (var entry in files)
        {
            _filesPanel.Children.Add(BuildRow(entry, cts.Token));
        }
    }

    private Grid BuildRow(RecentEntry entry, CancellationToken token)
    {
        var row = new Grid
        {
            ColumnSpacing = 8,
            Padding = new Thickness(8),
            CornerRadius = new CornerRadius(4),
        };
        row.ColumnDefinitions.Add(new ColumnDefinition());
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        var infoStack = new StackPanel { Spacing = 2 };
        var nameText = new TextBlock
        {
            Text = entry.DisplayName,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        var statusText = new TextBlock
        {
            Text = "Checking…",
            FontSize = 12,
            Opacity = 0.75,
        };
        infoStack.Children.Add(nameText);
        infoStack.Children.Add(statusText);
        row.Children.Add(infoStack);

        var actions = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var openBtn = new Button { Content = "Open" };
        openBtn.Click += (_, _) => _openEntry(entry);
        actions.Children.Add(openBtn);

        var recoverBtn = new Button
        {
            Content = "Locate…",
            Visibility = Visibility.Collapsed,
        };
        recoverBtn.Click += (_, _) =>
        {
            var picked = entry.Kind == RecentKind.Folder
                ? DocumentPicker.PickFolder(this)
                : DocumentPicker.PickFile(this);
            if (!string.IsNullOrWhiteSpace(picked))
            {
                _relocateAndOpen(entry, picked);
            }
        };
        actions.Children.Add(recoverBtn);

        var revokeBtn = new Button { Content = "Revoke" };
        revokeBtn.Click += (_, _) => _revokeEntry(entry);
        actions.Children.Add(revokeBtn);

        var removeBtn = new Button { Content = "Remove" };
        removeBtn.Click += (_, _) => _recents.Remove(entry.Kind, entry.Path);
        actions.Children.Add(removeBtn);

        Grid.SetColumn(actions, 2);
        row.Children.Add(actions);

        _ = Task.Run(() => RecentsStore.CheckAvailability(entry), token).ContinueWith(t =>
        {
            if (token.IsCancellationRequested || t.IsFaulted || t.IsCanceled)
            {
                return;
            }
            var avail = t.Result;
            _ = DispatcherQueue?.TryEnqueue(() =>
            {
                if (token.IsCancellationRequested)
                {
                    return;
                }
                statusText.Text = avail.Label();
                if (avail.IsRecoverable())
                {
                    recoverBtn.Content = avail is RecentAvailability.PermissionDenied or RecentAvailability.AccessRevoked
                        ? "Grant Access…"
                        : "Locate…";
                    recoverBtn.Visibility = Visibility.Visible;
                }
                else
                {
                    recoverBtn.Visibility = Visibility.Collapsed;
                }
            });
        }, CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);

        return row;
    }
}
