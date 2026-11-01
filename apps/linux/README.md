# Linux application

LNX-001A provides the native GTK 4 shell, written in Rust with gtk-rs: an
empty reader window with a menu bar, File → New Window/Close Window/Quit and
Help → About Markdown. Ctrl+N, Ctrl+W and Ctrl+Q are the standard
accelerators. The window shows a native, accessible "No document open" label
and uses the desktop's GTK theme.

The shell behaves like a normal GTK application, which differs from the macOS
shell in two places:

- Closing the last window ends the application. Linux desktops have no Dock
  that keeps a windowless app running.
- The application is single-instance through D-Bus, under the application ID
  `io.github.awesomejt.markdown` (the macOS bundle ID). Launching it again
  opens another window in the running process.

Since LNX-001B the app links the model-v2 core library through a UI-free Rust
adapter, `apps/linux/core` ([integration report](../../docs/Linux-Core-Integration.md)),
and refuses to start against an incompatible core. There is no document
loader, file association or renderer yet. Those begin in LNX-001C/001D, and
LNX-001D1 must prove AT-SPI and Orca
behaviour before the renderer is deepened (D-017). The accepted baseline is
x86-64 KDE Plasma on Wayland (D-018); libadwaita is not used, and other
desktops and architectures are not claimed.

## Build and launch

Install Rust 1.99.1 (the root `rust-toolchain.toml`), the GTK 4 development
files (`gtk4` on Arch, `libgtk-4-dev` on Debian and Ubuntu) and `pkg-config`.
The app is its own Cargo workspace with its own `Cargo.lock`. The root
workspace and its dependency policy cover only the portable core, and gtk-rs
is not a core dependency. The app does not depend on the core's Rust crates
either: it links the core's C ABI library from a staged package.

From the repository root, stage the core package for the configuration you
build, then build and launch:

```sh
python3 apps/linux/tools/package_core.py --configuration debug
cargo build --manifest-path apps/linux/Cargo.toml --workspace --locked --target-dir build/linux/target
build/linux/target/debug/markdown-linux
```

For an optimized build, stage with `--configuration release` and add
`--release`; the output is at `build/linux/target/release/`. A build refuses
a core package of the other configuration. Re-stage after changing core
sources. Build output and packages stay under the ignored `build/linux/`; the
binaries find `libmarkdown_ffi.so` through a `RUNPATH` relative to their own
directory, so keep the `build/linux/` layout together. This is a local
development binary, not a package: there is no desktop entry, icon, Flatpak
or Arch package yet, and nothing is installed or registered. The binary name
`markdown-linux` is a development name; packaging (LNX-001I and later) picks
the installed name.

## Checks

Run in a graphical session (Wayland or X11):

```sh
python3 apps/linux/tools/check.py
```

It generates the performance fixtures, stages the debug and release core
packages, then runs `cargo fmt --check`, Clippy with warnings denied over the
workspace, and the
[dependency/license policy](../../docs/Dependencies.md#linux-app-graph-lnx-001a)
for the Linux graph. Then, for debug and then release, it builds the
workspace with `--locked`, runs the 15 core consumer tests against the
matching core package (negotiation, typed errors, owned v2 values, parse
failures, copy budget, queued and running cancellation, bounded admission,
close, non-blocking `close_then` and drop, availability, repeated lifecycle and every corpus and
performance profile), re-verifies the package hash and runs each binary's two
smoke modes as child processes, with a 30-second timeout each:

- `--smoke-test` checks the application ID, one mapped, resizable window with
  a menu bar, the empty-state label (text, accessible role, mapped), every
  menu item and its accelerator against a list written separately from the
  app's own, New Window, Close Window with the closed window released, About
  (transient for the window, with the program name and version) and Quit
  with two windows open.
- `--smoke-test=last-window` closes the only window and expects the
  application to exit.
- The commands smoke also parses a heading off the main thread through the
  core adapter, checks the published model-v2 outline and the package
  configuration, and expects zero live native handles afterwards.

Last, it runs the release measurement probe once over the 1 MiB prose
profile; the [integration report](../../docs/Linux-Core-Integration.md#reproduce)
has the full measurement commands.

Each run must print its pass marker, then report that the GTK main loop ended,
and exit zero. A smoke run uses `NON_UNIQUE`, so it never hands its checks to
a reader already running on the desktop. Commands are activated as actions;
this proves the actions and the accelerators installed for them, not physical
key presses. Run the manual check below as well. This is shell acceptance, not
renderer, screen-reader, startup or memory acceptance.

CI runs the path-filtered `linux-native` job on `ubuntu-24.04`, which installs
`libgtk-4-dev` (GTK 4.14) and runs the check under `dbus-run-session` and
`xvfb-run` with `GDK_BACKEND=x11`, because hosted runners have no display or
session bus. That proves the
build and smoke on GTK's X11 backend; Wayland and KDE evidence comes from the
named host below. A selected native job must succeed for `Checks / complete`;
only an unselected job may be skipped. Changes under `apps/linux/` select only
Linux host work; shared, tool and workflow changes select every host.

## Manual shell check

1. Launch the binary above and confirm the Markdown window and its menu bar
   appear, with no document loaded.
2. Press Ctrl+N for a second window and Ctrl+W to close it. Resize, maximize
   and minimize the remaining window with the window controls.
3. Open Help → About Markdown and close it. Launch the binary again from a
   terminal and confirm a second window opens in the running app.
4. Press Ctrl+Q. Relaunch, close the window with Ctrl+W and confirm the
   process exits.

Inspect the window, menu bar and empty-state label with an AT-SPI tool such as
Accerciser. Orca reading, heading navigation and large documents are later
native checks (LNX-001D1, LNX-001G). Do not change user accessibility settings
as part of this smoke.

## LNX-001A local evidence

Checked on 2026-10-31: AMD Ryzen 9 5950X (x86-64), CachyOS, kernel 7.3.0, KDE
Plasma 6.8.2 on Wayland (KWin 6.8.2), GTK 4.24.1, GLib 2.90.1, Pango 1.58.2,
gtk-rs (gtk4 0.11.5), Rust 1.99.1, Python 3.14.8.

- `python3 apps/linux/tools/check.py` passed: format, Clippy, the Linux
  license policy (65 packages) and both smoke modes in debug and release on
  the Wayland backend.
- Both smoke modes also passed on the X11 backend through Xwayland
  (`GDK_BACKEND=x11`), the backend CI uses.
- A deliberately wrong Close Window accelerator made the smoke fail with
  `accelerator for Close Window`.
- A live AT-SPI inspection of the release binary showed an application
  `markdown-linux` with a `frame` "Markdown" (active, showing), a `label`
  "No document open" and a `menu bar` with focusable `menu item`s File and
  Help.
- A second launch exited 0 and the first process then had two Markdown
  frames.

Physical key presses, the window controls, the About dialog's appearance and
Orca were not exercised in this pass; the manual check above covers them.

## LNX-001B local evidence

Checked on 2026-10-31 on the host above (CachyOS, kernel 7.3.0, KDE Plasma
6.8.2 Wayland, GTK 4.24.1, glibc 2.44, Rust 1.99.1, Python 3.14.8):

- `python3 apps/linux/tools/check.py` passed on Wayland, and under
  `GDK_BACKEND=x11 dbus-run-session -- xvfb-run -a` as CI runs it: package
  staging and audit (24 exports), format, Clippy, the license policy, 15
  consumer tests in debug and in release, both smoke modes with the core
  check in each configuration, and the release probe.
- A debug build pointed at the release package (`MARKDOWN_CORE_PACKAGE`)
  failed in the build script with a configuration mismatch.
- The release app's `RUNPATH` is `$ORIGIN`-relative only, and the packaged
  library contains no checkout or home path.
- Measurements, host limits and the dense 8 MiB peak are in the
  [integration report](../../docs/Linux-Core-Integration.md#measurements).
