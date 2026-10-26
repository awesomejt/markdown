# Virtualized Renderer Evaluation — FEAS-008 Linux

## Status and recommendation

This covers the Linux half of FEAS-008, completed on 2026-10-25. The Windows half
(WinUI 3 in C# JIT, Native AOT and C++/WinRT, plus a WebView2 baseline) is
still to run, so FEAS-008 is not complete.

**Recommendation for GATE-001 (not accepted):** use a native GTK 4
`GtkListView` over semantic blocks as the Linux document surface, not a
WebView. On the named host it presents 1 MiB, 5 MiB and long-line documents
3.3 to 9.6 times faster than WebKitGTK when warm, with a third to a half of
the memory. It meets every functional acceptance item except screen-reader
structural navigation and continuous reading past the screen. Those gaps need
a design in LNX-001 before the surface is called stable.

The cost is ownership. GTK does not provide document-wide selection, copy,
highlight, keyboard row movement or heading alignment for list rows, so the
app implements all of them. The native renderer is about 1,480 lines against
about 340 for the WebView host, excluding automation, plus a shared 955-line
model. WebKit gives selection, find and full-tree accessibility without that
code, but it needed its own resource and navigation policy layer.

## What was built

[`spikes/feas008`](../spikes/feas008/README.md) contains three crates:

- **docmodel** turns pulldown-cmark output into owned values: blocks of
  rendered text with styled runs, block source spans, outline and slugs,
  display fragments, find, and link and image policy. It is linked in-process
  because FEAS-006 already proved the C ABI. CORE-002 replaces it.
- **native** is a gtk-rs app. A `GtkListView` with a lazy count-only model
  shows display fragments; only rows near the viewport exist as widgets.
  Selection, copy, find, outline jumps, links and image loading all operate
  on the model, so they reach rows that were never realized.
- **webview** renders the same model to HTML in WebKitGTK 6. Page scripts are
  disabled and a strict CSP is set. Images load through a `mdv-res:` scheme
  that reuses the native containment checks, and `decide-policy` gates
  navigation through the same link policy.

## Named environment

CachyOS (kernel 7.3.0-4-cachyos, glibc 2.44), x86-64 Ryzen 9 5950X, 64 GiB,
RTX 4080, KDE Plasma Wayland at scale 1, 1200×900 window. GTK 4.24.1, Pango
1.58.2, gtk-rs 0.11.5, WebKitGTK 2.54.2, Orca 51.1, Rust 1.99.1.

## D-012 launch to presented content

Each run is a fresh process. The clock starts from `CLOCK_MONOTONIC` just
before spawn and stops at the compositor presentation time of the first frame
containing document rows. Presentation time comes from GDK frame timings; when
the compositor reports none, paint time is used. Five cold and five warm runs
were made per input.

Cold runs evict every file the app maps or opens, plus theme, font-cache and
shader-cache files, with `posix_fadvise(DONTNEED)`. About 284 MiB in about
1,080 files drops to at most 0.2 MiB resident. This is not a full
`drop_caches`, since shared pages mapped by other processes stay.

| Input | Engine | Cold median (min–max, σ) | Warm median (min–max, σ) | Warm peak RSS |
| --- | --- | ---: | ---: | ---: |
| prose 1 MiB | native | 528 ms (523–538, 7) | 289 ms (288–303, 6) | 207 MiB |
| prose 1 MiB | WebView | 1735 ms (1718–13203, 5115) | 952 ms (934–970, 16) | 482 MiB |
| prose 5 MiB | native | 551 ms (545–563, 7) | 302 ms (287–303, 7) | 264 MiB |
| prose 5 MiB | WebView | 3428 ms (3403–3462, 22) | 2898 ms (2872–2911, 18) | 820 MiB |
| long lines 1 MiB | native | 623 ms (618–638, 8) | 373 ms (372–375, 1) | 201 MiB |
| long lines 1 MiB | WebView | 1822 ms (1819–1846, 13) | 1291 ms (1271–1297, 10) | 415 MiB |

WebView memory includes its web and network processes. The 13.2-second
WebView value was the first cold run of the session. An earlier native sweep
had one similar 8.1-second first cold run that did not recur in two later
sweeps; both look like first-launch cache rebuilds and are recorded rather
than removed. The WebView 5 MiB runs never received compositor presentation
times, so those are paint times. The WebView "presented" point is the first
frame after an injected double `requestAnimationFrame` message, which only
approximates WebKit's first content.

Off-screen jumps (find, outline and sampled rows) presented in a median of
38 ms, with a maximum of 44 ms in prose and 137 ms in long-line input. Native
parsing took 15 ms for 1 MiB and 58 ms for 5 MiB, overlapping GTK startup.

**Budget proposal for GATE-001**, Linux on this class of hardware: warm at most
400 ms and cold at most 700 ms for all three inputs, and off-screen jumps at
most 150 ms. The native renderer meets them; its least headroom is 7 percent,
on warm long-line input (373 ms). The WebView misses all of them.

## The design constraint that matters

`GtkListView` keeps about 205 rows realized however tall they are, so a launch
or a far jump lays out about 205 rows. Two consequences were measured:

- **Wrapped labels are expensive by default.** Without `width-chars`, GTK
  spent about 18 times the raw Pango cost measuring each wrapped label's
  minimum width. A 512-byte-fragment paragraph took about 440 ms per jump.
  Setting `width-chars`/`max-width-chars` to 40 removed that cost.
  Accessibility, `natural-wrap-mode` and `max-width-chars` alone made no
  material difference.
- **Fragment size bounds the work.** 1 MiB of unbreakable text took 2.0, 6.4
  and 23.7 seconds at 512-, 1,024- and 2,048-byte fragments, and timed out at
  4 KiB. The spike cuts fragments at a newline or space within 1 KiB, cuts
  unbreakable runs at 512 bytes and wraps those by character. Continuation
  rows drop their padding, but a visible early line break remains at each
  fragment edge.

Production should keep both limits as tested invariants, and could align
fragment edges to measured line breaks to remove the seams.

## Functional results

### Selection and copy

Selection is a model range, not widget state. A capture-phase drag on the list
hit-tests Pango layouts into model positions and auto-scrolls past the edges.
Shift+click extends; a click or Esc clears. Keyboard selection uses
Shift+Left/Right, Shift+Up/Down and Shift+Ctrl+Home/End. Highlighting is
generated into each visible row's markup.

Copy places rendered text as `text/plain` and the selected blocks' Markdown as
`text/markdown`. After the owner pressed Ctrl+A and Ctrl+C on the 5 MiB
document, `wl-paste` in a separate process read back:

- `text/markdown` byte-identical to the source (5,242,880 bytes)
- `text/plain` byte-identical to the model text (5,170,055 bytes)

In every automated run, five sampled rows across the document displayed
exactly the model fragment found at the matching offset of the copy.

### Find, outline and links

Find searches the model, taking about 8 ms for 5 MiB. It scrolls to an
off-screen match, selects it and presents it. Outline entries and `#fragment`
links jump, move focus, and align the heading to the viewport top unless the
document end prevents it. Clicking or arrowing in the outline moves the
document; Enter or double-click also moves focus into it.

Link intents follow the model's policy:

| Link | Behavior |
| --- | --- |
| `#fragment` | Jumps to the heading |
| Missing fragment | Ignored |
| `https:` and `mailto:` | Handed to the system with `gtk::UriLauncher` |
| Relative `.md` | Local-document intent; opening is not implemented in the spike |
| `javascript:`, `file:`, `data:`, escaping paths | Blocked |

The owner exercised these by pointer, and the decisions were logged.

### Images under policy

The model blocks remote, escaping, absolute, SVG and `data:` images before any
I/O. The GTK adapter then resolves the request against the document folder,
canonicalizes it, and checks containment, size, magic bytes and header
dimensions. It decodes off the main thread. A symlink leaving the folder, a
text file named `.png`, a PNG claiming 20000×20000 and a missing file were
refused there; the valid PNG presented. The WebView reached the same outcomes
only by routing images through its custom scheme. WebKit would otherwise
follow the symlink.

### Accessibility

AT-SPI was inspected on the live bus with GTK 4.24.1:

- The document list is named. Its rows are list items whose label is the
  fragment text and whose description is the block role.
- Heading labels carry the heading role and a `level` attribute; images are
  named by their alt text.
- Only the about 30 on-screen rows are exposed. At the top, middle (item
  60,000) and end of the 5 MiB document, they were a contiguous run of model
  fragments in reading order.

The owner confirmed with Orca that rows are read in order while moving with
plain Down. Two gaps remain:

- Orca's **H** heading navigation does nothing, because Orca offers structural
  navigation only in documents it treats as browsable.
- Whether **Say All** continues past the visible screen was not confirmed.

The WebView exposes the whole DOM, so neither gap applies there, but its Orca
behavior was not compared because the owner stopped Orca.

### Owner-found defects, all fixed during the pass

- A drag lost its anchor when its first update produced an empty range.
- A row label's gesture took the pointer sequence after a few pixels.
- GtkLabel kept merged highlight attributes, so clearing and reselecting
  failed visually. Highlighting moved into markup.
- Plain Up/Down did not move between rows.
- Heading jumps scrolled minimally instead of aligning to the top.
- The outline had no visible selection.

Each fix has an automated or traced check in the spike.

## Native compared with WebView

| Aspect | Native `GtkListView` | WebKitGTK |
| --- | --- | --- |
| Warm launch, 5 MiB | 302 ms | 2,898 ms |
| Peak RSS, 5 MiB | 264 MiB | 820 MiB, across three processes |
| Selection and copy | App-owned. Exact; adds a `text/markdown` flavor | Built in. Real keypress copy works; synthetic copy was unreliable under Wayland |
| Find | App-owned model search, 8 ms | Built in, 328 ms to first result on 5 MiB |
| Screen reader | On-screen rows only; no heading navigation | Full DOM tree; not compared with Orca |
| Resource policy | Adapter checks before any read | Required a custom scheme, CSP and a navigation gate |
| Renderer code | About 1,480 lines | About 340 lines |
| Guardrail fit | Native presentation | A general-purpose WebView, which AGENTS.md excludes by default |

## Open items for LNX-001 and GATE-001

- Screen-reader structure: expose heading navigation (for example a
  document-role container with navigable children, or app-level shortcuts) and
  confirm Say All across the whole document with Orca on KDE and GNOME.
- Fragment seams, tables (still tab-separated text), code without horizontal
  scrolling, and math and diagram presenters.
- GNOME, X11, fractional scaling, RTL, IME and font substitution.
- Repeated-session memory, which was not measured on Linux.
- The WebView Orca comparison, if GATE-001 wants it before deciding.

## Reproduction

The commands are in the [spike README](../spikes/feas008/README.md). Full
values are in
[`results-linux-2026-10-25.yaml`](../spikes/feas008/results-linux-2026-10-25.yaml).
