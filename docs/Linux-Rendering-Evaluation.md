# Linux Native Rendering Evaluation

## Status and recommendation

FEAS-005 is complete on the available Linux workstation. Use **GTK 4 with
gtk-rs for the Linux application shell and native widgets**, but do not use one
monolithic `GtkTextView` as the production renderer for unbounded documents.

The next Linux renderer spike should use a virtualized block model—most likely
`GtkListView` rows backed by semantic blocks—with bounded `GtkTextView` or Pango
content inside visible rows. Continuous cross-block selection and a coherent
accessible reading order are the two explicit proof obligations for that
design. Keep a single `GtkTextView` as a simple path for small documents only
if the two paths can share semantics and behavior.

This recommendation selects a Linux UI direction, not a complete production
stack. GATE-001 still owns the baseline distro/runtime, package formats, and
the exact renderer composition.

## Prototype

The disposable [`spikes/feas005`](../spikes/feas005/README.md) app uses
pulldown-cmark events to populate a read-only native `GtkTextBuffer` with text
tags for headings and common inline/block styles. A `GtkTextView` presents the
buffer in a `GtkScrolledWindow`. Automated mode waits for a real frame-clock
tick, requests an end-scroll, creates and reads back a selection, records the
native accessibility role and scale factor, then exits.

The prototype is intentionally incomplete as a renderer: tables use tab/newline
text, list numbering is simplified, images are alt text, raw HTML is inert
text, and specialized math/diagram widgets are absent. Those limitations keep
the spike focused on GTK's document-surface behavior rather than duplicating
CORE-002/CORE-003.

## Named environment

- CachyOS rolling, Linux `7.3.0-4-cachyos`, x86-64.
- AMD Ryzen 9 5950X, 16 cores/32 logical CPUs.
- KDE Plasma Wayland session using the Breeze GTK theme.
- Three enabled 2560×1440 outputs at compositor scale 1.
- GTK 4.24.1, Pango 1.58.2, gtk-rs 0.11.5, Rust 1.99.1.
- A small-document smoke test also passed with `GDK_BACKEND=x11` through
  XWayland. No GNOME session was available.

## Results

Times below are medians of three fresh process launches. “Parse + buffer”
includes parsing and all tagged `GtkTextBuffer` insertions. “First frame” is
measured from process start to the first GTK frame-clock callback.

| Input | Parse + buffer | First frame | Peak RSS |
| --- | ---: | ---: | ---: |
| prose, 1 MiB | 96.5 ms | 298.8 ms | 254 MiB |
| prose, 5 MiB | 496.5 ms | 727.3 ms | 289 MiB |
| delimiter storm, 1 MiB | 175.8 ms | 361.4 ms | 251 MiB |
| one long line, 1 MiB | 10.4 ms | 226.3 ms | 255 MiB at timeout |

The 1 MiB ordinary document met the initial “useful content within 500 ms”
hypothesis on this machine. The 5 MiB document did not meet that 1 MiB
hypothesis—which was not its stated budget—but its memory and navigation
behavior are unacceptable for a general unbounded renderer.

`GtkTextView` lays out large content incrementally, so first paint stayed well
below full-document layout cost. That same behavior made far navigation
unreliable in this design:

- Small GFM content scrolled to the final iterator on both Wayland and XWayland.
- The 1 MiB prose, 5 MiB prose, and delimiter-storm cases had not reached the
  end after the 120-frame observation limit (about 2.3–2.7 seconds).
- Requesting the end of a one-line 1 MiB paragraph blocked the GTK main thread;
  the process was terminated after ten seconds with exit 124.

This is direct evidence against a single, wrapping `GtkTextView` for every
document. It also reinforces a source requirement: extremely long logical
lines need a bounded presentation or non-wrapping specialized fallback.

Exact results and caveats are in
[`results-2026-10-24.yaml`](../spikes/feas005/results-2026-10-24.yaml).

## Native behavior

### Selection and text

Every one of the eight frozen semantic documents launched and allowed the
probe to select and read back 32 rendered characters. `GtkTextView` supplies
native pointer/keyboard selection and clipboard actions, a substantial
advantage over a custom drawing surface.

That advantage becomes a design constraint for virtualization. `GtkListView`
creates/recycles widgets only for visible items, so GTK does not provide
continuous text selection across separate rows. A block renderer must prove a
document-level selection model, copy behavior, keyboard extension, and visual
highlighting before it can replace the simple text view.

### Accessibility

The live AT-SPI bus exposed the probe as GTK 4.24.1 with a window and a
`text box` document object. That object implemented AT-SPI Accessible,
Component, Text, EditableText, and Action interfaces. Its character count for
the GFM tour was 2,103 and `GetText(0, 80)` returned the rendered content.

This verifies an inspectable native text surface, not screen-reader quality.
The read-only view still advertises the general EditableText interface, so
state/announcements must be checked with Orca on the supported desktop matrix.
Tables, code, images, links, and headings also need semantic roles beyond the
flat text produced by this spike.

### Scaling and desktop environments

The measured compositor scale and GTK widget scale factor were both 1. GTK
rendered successfully through native Wayland and XWayland backends. Forced
`GDK_SCALE`/`GDK_DPI_SCALE` values did not change the widget scale under this
Wayland session, so fractional/high-DPI behavior is unproven rather than
passed.

The Breeze theme emitted two non-fatal negative slider-size warnings on
Wayland. Scrolling still functioned. The production matrix must add current
GNOME, KDE, and at least one X11 environment at the accepted GTK baseline.

## Packaging evidence

The stripped release probe is 815,104 bytes. Its ELF has no RPATH/RUNPATH and
directly requires GTK 4, Pango, GIO, GObject, GLib, libgcc, and libc shared
libraries. The host's GTK installation brings a much larger transitive native
dependency set, as expected.

The crate is compiled against GTK 4.10 APIs even though the test host provides
4.24.1. That is a provisional compatibility floor only. Flatpak, AppImage, and
distro-native packages were not produced; GATE-001 must select package formats
after baseline distributions are named. libadwaita is not required by the
probe and should remain optional unless the project intentionally chooses a
GNOME-specific presentation.

## Production direction

1. Use GTK 4/gtk-rs for the window, commands, pickers/portals, settings,
   outline, search UI, and ordinary native controls.
2. Feed a versioned semantic block model to a virtualized document surface.
3. Start by testing `GtkListView` because GTK creates only visible row widgets
   and recycles them for large models.
4. Keep each text layout bounded by block/section size; route pathological long
   lines to a horizontally scrolling or otherwise bounded presenter.
5. Use specialized native presenters for tables, code, images, and later math.
6. Treat cross-block selection, accessible reading order, focus/navigation,
   link activation, scroll anchoring, and print/export as acceptance tests, not
   implementation details.

If a virtualized GTK composition cannot meet selection and accessibility
requirements, the fallback is a custom Pango-backed document widget. That
would require implementing selection, hit testing, focus, AT-SPI text ranges,
and incremental layout, so it is a higher-cost option rather than the default.

## Follow-up checks

- Add the virtualized block proof during LNX-001 before declaring the Linux
  document surface stable.
- Run Orca and keyboard-only scenarios on GNOME Wayland and KDE Wayland.
- Test accepted minimum GTK versions and X11 fallback distributions.
- Test fractional scaling, multiple scale factors, RTL/CJK/emoji, input method
  interaction, and font substitution.
- Produce Flatpak and selected distro package prototypes at GATE-001/CORE-001.
- Preserve this spike's long-line and far-scroll cases as regressions.

## Primary references

- [GTK TextView documentation and accessibility role](https://docs.gtk.org/gtk4/class.TextView.html)
- [GTK ListView documentation](https://docs.gtk.org/gtk4/class.ListView.html)
- [GTK list recycling model](https://docs.gtk.org/gtk4/section-list-widget.html)
- [GTK accessibility architecture](https://docs.gtk.org/gtk4/section-accessibility.html)
- [gtk-rs 0.11 TextView bindings](https://gtk-rs.org/gtk4-rs/stable/0.11/docs/gtk4/struct.TextView.html)
