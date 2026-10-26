# Virtualized Renderer Evaluation — FEAS-008 Windows

## Status and recommendation

This covers the Windows half of FEAS-008, completed on 2026-10-26. The
[Linux half](Virtualized-Renderer-Linux-Evaluation.md) was completed on
2026-10-25. Together they complete FEAS-008. Nothing here is an accepted
baseline: GATE-001 decides.

**Document surface (not accepted):** use a native WinUI 3 `ListView` over
semantic blocks as the Windows document surface, not a WebView. On the named
host, the native hosts presented 1 MiB, 5 MiB and long-line documents 2.4 to
12.4 times faster than WebView2 when warm. They used a fifth to a quarter of
the memory. Every functional acceptance item passed, including Narrator
heading navigation and continuous reading past the screen. Two accessibility
defects remain for WIN-001 (see [Accessibility](#accessibility)).

**Windows host language (not accepted):** use C# with Native AOT. Keep
C++/WinRT as the fallback, and don't ship the C# JIT build:

- C# Native AOT and C++/WinRT are within about 15 ms of each other on every
  input, cold and warm, with the same memory.
- C# JIT is about 150–170 ms slower on every warm launch. It misses the
  400 ms warm budget proposed on Linux for both prose inputs.
- The two languages needed about the same amount of code. C# kept the
  managed tooling and the existing FEAS-003/006 code, but Native AOT added
  the projection pitfalls listed under
  [Native AOT findings](#native-aot-findings).

## What was built

[`spikes/feas008`](../spikes/feas008/README.md) gained a Windows half:

- **ffi** is a disposable C ABI over the unchanged FEAS-008 block model
  ([`feas008.h`](../spikes/feas008/ffi/feas008.h)). The model stays in Rust
  memory. Hosts read fixed-layout values through pointers and decode fragment
  text only for realized rows. FEAS-006 already tested a hardened boundary;
  this one only carries the model. CORE-002 replaces both.
- **csharp** is one WinUI 3 source tree, built twice: a default
  framework-dependent JIT app (`jit/`) and a Native AOT publish (`aot/`).
- **cpp** is the same surface in C++/WinRT, compiled with MSVC against the
  pinned C++/WinRT projection.
- **webview** renders the same model to HTML in WebView2, published with
  Native AOT like the fastest C# host.

Both native hosts use a `ListView` with a count-only items source. Only
containers near the viewport exist. Each container owns its row visuals: a
`RichTextBlock` of styled `Run` and `Hyperlink` elements, an `Image` and a
status note. The visuals move into a one-`Grid` item template on
`ContainerContentChanging`. Selection, copy, find, outline and link jumps,
and image loading all work on the model, so they reach rows that were never
realized. The clipboard and the image policy are Windows adapters: Win32
clipboard formats and handle-based path checks.

## Named environment

Windows 11 Pro 10.0.26300.9711, x86-64 Ryzen 9 5950X, 64 GiB, RTX 4080. Three
2560×1440 displays at 150%, with windows on the primary. The window is
1200×900 physical pixels (800×600 effective) with a 260-pixel outline.
Windows App SDK 2.5.1 (WinUI 2.3.9, framework-dependent, unpackaged), .NET
10.0.13, C++/WinRT 3.0.260818.1, MSVC 14.51, WebView2 runtime 155.0.4283.62,
Rust 1.99.1 and pulldown-cmark 0.13.5.

## D-012 launch to presented content

Each run is a fresh process. The launcher reads `QueryPerformanceCounter`
just before `CreateProcess` and passes it in. The host waits for the first
`CompositionTarget.Rendering` tick in which a realized row's text is laid out.
At low dispatcher priority, after the UI thread has committed that frame, a
worker calls `DwmFlush`. The time when it returns is "presented". For
WebView2, the first frame is signalled by an injected double
`requestAnimationFrame`, then measured the same way.

Cold runs were made from an elevated process. Before each run it emptied the
system file-cache working set and flushed the modified page list. It then
purged the standby list (`MemoryPurgeStandbyList`) and paused for 2 seconds.
Warm runs follow one discarded launch. Every timed run also ran the full
automated checks after presentation, and every one passed.

| Input | Host | Cold median (min–max, σ) | Warm median (min–max, σ) | Warm peak working set |
| --- | --- | ---: | ---: | ---: |
| prose 1 MiB | C# JIT | 509 ms (488–640, 63) | 429 ms (413–435, 9) | 177 MiB |
| prose 1 MiB | C# Native AOT | 347 ms (329–393, 25) | 273 ms (254–277, 10) | 130 MiB |
| prose 1 MiB | C++/WinRT | 342 ms (327–375, 18) | 279 ms (264–283, 9) | 129 MiB |
| prose 1 MiB | WebView2 | 1110 ms (1098–1233, 56) | 947 ms (940–965, 10) | 574 MiB |
| prose 5 MiB | C# JIT | 510 ms (500–523, 8) | 445 ms (441–459, 9) | 281 MiB |
| prose 5 MiB | C# Native AOT | 337 ms (326–346, 10) | 273 ms (270–286, 6) | 238 MiB |
| prose 5 MiB | C++/WinRT | 356 ms (342–358, 7) | 283 ms (268–290, 8) | 231 MiB |
| prose 5 MiB | WebView2 | 3388 ms (3373–3554, 77) | 3381 ms (3342–3444, 38) | 1385 MiB |
| long lines 1 MiB | C# JIT | 469 ms (465–477, 4) | 401 ms (401–418, 8) | 157 MiB |
| long lines 1 MiB | C# Native AOT | 309 ms (300–315, 7) | 240 ms (238–254, 7) | 112 MiB |
| long lines 1 MiB | C++/WinRT | 314 ms (310–318, 3) | 255 ms (238–256, 8) | 112 MiB |
| long lines 1 MiB | WebView2 | 737 ms (735–748, 5) | 582 ms (576–596, 8) | 463 MiB |

WebView2 memory includes its browser, renderer, GPU and utility processes.
Two first-of-input cold runs were slower: C# JIT 1 MiB at 640 ms and WebView2
1 MiB at 1233 ms. Both are included above.

Where the time goes, from warm medians:

- **Reaching `Main`** took about 61 ms for JIT, 17 ms for AOT and 11 ms for
  C++.
- **Window and XAML startup** reached the window's `Loaded` at about 300 ms
  for JIT and 205–213 ms for AOT and C++. This is most of the launch, and it
  doesn't depend on the document.
- **Parsing** runs on a worker during startup and stayed off the critical
  path: about 21–23 ms for 1 MiB and 103–111 ms for 5 MiB.
- **First rows and present** took about 60–75 ms after `Loaded`.

Off-screen jumps (find, outline, fragment link and five sampled rows per run)
presented in a median of 49–55 ms, with a maximum of 68 ms, in every native
host and input. WebView2 jumps had a median of 65–84 ms and a maximum of
115 ms.

**Budget check:** Linux proposed warm at most 400 ms, cold at most 700 ms
and jumps at most 150 ms. C# Native AOT and C++/WinRT meet all three on
Windows with at least 30% headroom. C# JIT meets cold and jumps but misses
warm for 1 MiB prose (429 ms) and 5 MiB prose (445 ms), and is borderline
for long lines (401 ms). WebView2 misses warm and cold for 1 MiB and 5 MiB
prose, and warm for long lines.

## What the Windows list does differently

- **It realizes few rows.** `ListView` realized about 15 rows at launch for
  prose and 5 for character-wrapped long lines, and about 32–48 after jumps.
  `GtkListView` realized about 205. The 512-byte fragment limit is therefore
  not under the same pressure on Windows. Keep it anyway, because selection,
  copy and accessibility work in fragments.
- **Wrapping needs no tuning.** `TextWrapping.Wrap` breaks inside words
  without the extra setting GTK needed. Nothing like GTK's label
  minimum-width cost appeared.
- **Containers must keep their content.** Marking a container prepared in
  `ChoosingItemContainer`, or replacing `ListViewItem.Content`, left some
  recycled rows unmeasured (19 px tall, zero-width text). The working pattern
  is a one-`Grid` item template into which each container moves its own
  visuals.
- **The presented check must require laid-out text.** An early run before
  that fix timed empty containers and reported plausible launch times. It was
  discarded. The presented condition now requires a non-zero text width, and
  the sampled-row checks compare displayed text with the model.

## Functional results

### Selection and copy

As on Linux, selection is a model range, not control state. A pointer press
hit-tests `RichTextBlock.GetPositionFromPoint` into a model position. The
position is mapped through each `Run`'s content offset, so `Hyperlink` element
boundaries don't shift it. Dragging past an edge auto-scrolls; Shift+click
extends; Esc clears. Keyboard selection uses Shift+Left/Right, Shift+Up/Down
and Shift+Ctrl+Home/End.

Highlighting uses `TextHighlighter`, whose ranges index plain characters
across `Run` and `Hyperlink` boundaries. A screenshot confirmed exact
placement across bold, link and code runs and a two-byte character.

Copy writes the Win32 clipboard directly:

- `CF_UNICODETEXT` holds the rendered text.
- A registered `text/markdown` format holds the source bytes.

A `DataPackage` custom format backed by a stream failed with
`CLIPBRD_E_BAD_DATA` after `Clipboard.Flush`. After the owner pressed Ctrl+A
and Ctrl+C on the 5 MiB document, a separate process (`clipboard_check.py`)
read back:

- `text/markdown` byte-identical to the source (5,242,880 bytes)
- `CF_UNICODETEXT` byte-identical to the model text (5,170,055 bytes)

In every automated run, five sampled rows across the document displayed
exactly the model fragment found at the matching offset of the copy.

### Find, outline and links

Find searches the model in about 13–17 ms for 5 MiB. It then scrolls to the
off-screen match, selects it and presents it in about 30–35 ms; F3 and
Shift+F3 move between matches. Outline entries and `#fragment` links jump,
move focus and align the heading to the viewport top unless the document end
prevents it. The status bar reports the heading and item number. Link intents
follow the model's policy, as on Linux; https and mailto links go to
`Launcher.LaunchUriAsync`.

### Images under policy

The model blocks remote, escaping, absolute, SVG and `data:` images before any
I/O. The Windows adapter then opens the document folder and the candidate file
and compares their `GetFinalPathNameByHandle` paths. This follows every link
and junction, then checks size, magic bytes and PNG header dimensions. The
symlink leaving the folder, the text file named `.png`, the PNG claiming
20000×20000 and the missing file were refused; the valid PNG presented. The
WebView2 baseline serves images through `WebResourceRequested` with the same
checks, and reached the same outcomes.

### Accessibility

UIA3 inspection (`uia_probe.py` with the native `UiaProbe.exe` client) of the
5 MiB document in both native hosts showed:

- At the top, middle (item 60,681) and end, the exposed list items are a
  contiguous run of model fragments in reading order. The list reports the
  true set size of 121,362 items.
- Rows are list items named with their text. Their item type is the block
  role.
- The list supports the `ItemContainer` pattern. It reaches unrealized items
  as virtualized items at about 0.35 ms each.
- The heading level is **not exposed** when set on the `ListViewItem`
  container. It is exposed when set on the row's `RichTextBlock`, and both
  hosts now do that.

The owner confirmed with Narrator in the C# AOT host that:

- Down reads the rows in order.
- In scan mode, H and Shift+H move between headings, including off-screen
  ones.
- Continuous reading carries on past the visible rows until Ctrl is pressed.

That covers both gaps left open on Linux with Orca. Two defects remain for
WIN-001:

- **Each row is read twice:** first as the list item's name, then as its text
  element. Expose the text in one place, keeping the heading level where
  Narrator finds it.
- **Whole-tree UIA searches time out at 5 MiB.** A descendant search of the
  window for the document list exceeded the 20-second UIA provider timeout.
  Entering from the focused row takes about 10 ms, and that is how Narrator
  behaved in practice. Tools that walk the whole tree, such as Accessibility
  Insights or a "list all headings" command, may still stall. A lighter peer
  for unrealized items needs investigating.

The owner did not check the C++/WinRT host with Narrator. UIA shows the same
tree as in the C# host. The managed UIA client in Windows PowerShell can't
read `HeadingLevel` or `PositionInSet`, so it isn't usable for this
inspection.

### Owner-found defects, all fixed during the pass

- Esc cleared the selection only when keyboard focus was already in the
  document.
- A click on a row didn't move keyboard focus, so arrows and Ctrl+A/C did
  nothing afterwards.
- Enter in the outline never reached the handler, because the list consumes
  it before `KeyDown`. The handler now uses `PreviewKeyDown`.
- Narrator heading navigation found no headings, because the heading level
  was set only on the container.

One owner Ctrl+A in the C++ host produced no selection. It was not reproduced
on the owner's retry with an input trace, or under synthetic input. The
likely causes are Narrator scan mode or a click that never reached a row.
It is recorded, not dismissed.

## Native AOT findings

- `VisualTreeHelper` children can be projected as a plain `DependencyObject`,
  so `is ScrollViewer` failed. A QueryInterface cast
  (`WinRT.CastExtensions.As`) works.
- Setting `PublishAot` in a project applies AOT feature switches to plain
  `dotnet build` output too, such as disabled dynamic code and EventSource.
  The JIT host is therefore a separate project.
- WebView2 works under Native AOT. An early null `CoreWebView2` was caused by
  orphaned `msedgewebview2` processes from a killed run holding the user-data
  folder. The harness now stops them after each WebView2 run.
- The AOT linker finds MSVC through `vswhere` on `PATH`; the builder adds it.

## Native compared with WebView2

| Aspect | Native `ListView` (C# AOT) | WebView2 |
| --- | --- | --- |
| Warm launch, 5 MiB | 273 ms | 3,381 ms |
| Peak memory, 5 MiB | 238 MiB | 1,385 MiB across five processes |
| Selection and copy | App-owned. Exact, with a `text/markdown` flavor | Built in. Whole-body selection of 4.7 M characters took 160 ms |
| Find | App-owned model search, 13–17 ms | Built in |
| Screen reader | Rows in order, H navigation and continuous reading. Each row read twice | Full DOM. Not checked with Narrator |
| Resource policy | Adapter checks before any read | Needed a virtual host, `WebResourceRequested`, CSP and a navigation gate |
| Scripts | None | CSP blocks page scripts. Script execution must stay on for host probes |
| Host code | About 1,130 lines (C#) or about 1,160 (C++) | About 230 lines |
| Guardrail fit | Native presentation | A general-purpose WebView, which AGENTS.md excludes by default |

## C# compared with C++/WinRT

| Aspect | C# Native AOT | C++/WinRT | C# JIT |
| --- | --- | --- | --- |
| Warm 1/5 MiB/long lines | 273/273/240 ms | 279/283/255 ms | 429/445/401 ms |
| Cold 1/5 MiB/long lines | 347/337/309 ms | 342/356/314 ms | 509/510/469 ms |
| Peak memory, 5 MiB | 238 MiB | 231 MiB | 281 MiB |
| Renderer code | About 1,130 lines plus 323 of checks | About 1,160 lines plus 270 of checks | Shared with AOT |
| Pitfalls | Projection casts, AOT feature switches | Manual identity keys, coroutine awaiters | Launch cost |
| Tooling | Managed debugging, reuse of FEAS-003/006 C# | MSVC/PDB, generated projection headers | As AOT |

## Open items for WIN-001 and GATE-001

- Fix the duplicated Narrator reading, and make whole-tree UIA queries stay
  within the provider timeout at 5 MiB.
- Check the C++/WinRT host with Narrator, if it remains a candidate.
- Fragment seams, tables (still tab-separated), code without horizontal
  scrolling, and math and diagram presenters.
- Mixed DPI, RTL, IME, high contrast and touch.
- Repeated-session memory, which was not measured on Windows.
- Clean-machine, packaged (MSIX) and self-contained launch, which may change
  cold numbers.

## Reproduction

The commands are in the [spike README](../spikes/feas008/README.md). Full
values are in
[`results-windows-2026-10-26.yaml`](../spikes/feas008/results-windows-2026-10-26.yaml).
