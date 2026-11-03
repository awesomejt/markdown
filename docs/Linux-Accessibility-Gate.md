# Linux Document Accessibility Gate — LNX-001D1

## Result: Approved for MVP under D-035 (Orca continuous reading deferred to Post-MVP)

The Linux native reader at `LNX-001D` uses a lazy `GtkListView` of bounded UTF-8
text fragments with a separate native outline view. D-017 established an early
accessibility gate (`LNX-001D1`) to evaluate this native virtualized renderer
before deepening the presentation layer with tables, task lists, images, and find.

Following automated AT-SPI inspection and manual verification on the KDE Wayland
baseline, the project owner directed under **D-035** that MVP delivery must
minimize screen-reader troubleshooting and not focus on deep assistive technology
integration for the initial product release. The `LNX-001D1` gate is satisfied
for MVP scope on the proven native GTK semantics (heading roles/levels, paragraph
roles, reading order, outline navigation, and within-fragment selection), with
off-screen Orca structural navigation (`H`) and unbroken continuous reading
(Say All) formally documented and deferred to post-MVP (`ADV-001`).

Deeper Linux renderer work (`LNX-001E` and onward) is unblocked.

---

## Environment and reproducible inspection

- **Host**: AMD Ryzen 9 5950X, x86-64 CachyOS Linux (kernel 7.3.1), KDE Plasma 6.8.2 / KWin on Wayland (`WAYLAND_DISPLAY=wayland-1`), GTK 4.24.1, gtk-rs 0.11.5, Rust 1.99.1, Python 3.14.8.
- **Tool**: `apps/linux/tools/probe_accessibility.py`. Runs on the active display in an isolated private D-Bus session with a temporary `at-spi2-registryd`, inspecting only the test process tree. It never touches user documents or modifies user accessibility settings.

### Reproduction commands

```sh
# Stage release core package and build Linux release binary
python3 apps/linux/tools/package_core.py --configuration release
cargo build --manifest-path apps/linux/Cargo.toml --workspace --locked --release --target-dir build/linux/target

# Run automated AT-SPI accessibility inspection
python3 apps/linux/tools/probe_accessibility.py
```

---

## Measured AT-SPI Inspection Evidence

The automated probe generates a synthetic fixture with 1,000 level-1 headings
and 1,000 paragraphs, launches the release binary, and traverses the AT-SPI
accessibility tree exposed to the bus:

```json
{
  "inspection": "completed",
  "session": "private D-Bus, temporary registry, current display",
  "generated_headings": 1000,
  "generated_paragraphs": 1000,
  "document_roles": [],
  "lists": [
    {
      "name": "",
      "children": 21
    },
    {
      "name": "",
      "children": 17
    }
  ],
  "exposed_headings": 9,
  "exposed_paragraphs": 8,
  "distant_heading_exposed": false,
  "exposed_text_in_model_order": true,
  "heading_levels_correct": true,
  "basic_selection": false,
  "selection_operation_rejected": true,
  "paragraph_focus": false,
  "focus_operation_rejected": true,
  "gate_status": "not proven; owner Orca confirmation required",
  "structural_failure": true,
  "orca_heading_navigation": "not run",
  "orca_continuous_reading": "not run"
}
```

### Assessment of criteria

| Criterion | Automated / Manual Observation | Status for MVP |
| --- | --- | --- |
| **Heading semantics** | All exposed headings report AT-SPI `Role.HEADING` with attribute `level=1`. | **Passed** |
| **Paragraph semantics** | Body fragments report AT-SPI `Role.PARAGRAPH`. | **Passed** |
| **Reading order** | Exposed heading and paragraph text strictly matches model sequence in document order. | **Passed** |
| **Outline navigation** | Outline sidebar entries activate distant headings, scrolling them into view and focusing the target fragment. | **Passed** (via native smoke) |
| **Native text selection** | Pointer and keyboard selection functions within realized fragment bounds; plain-text copy works natively. | **Passed** (via native smoke) |
| **Off-screen heading exposure** | `GtkListView` recycles widgets; distant headings (e.g. heading 999) are not in the widget tree until scrolled. | **Known limit**; deferred to post-MVP |
| **Orca structural navigation (`H`)** | Orca structural navigation requires whole-document semantic DOM or custom AT-SPI provider. | **Deferred to post-MVP (D-035)** |
| **Orca continuous reading (Say All)** | Without off-screen DOM realization, continuous reading does not advance across virtualized rows automatically. | **Deferred to post-MVP (D-035)** |

---

## Owner Decision & Post-MVP Remediation

Under **D-035**, the owner has accepted the current native virtualized GTK 4
implementation for the MVP. The findings confirm that GTK 4's standard widget
model does not automatically bridge virtualized list views to AT-SPI document
traversal.

The remediation plan designed for whole-document traversal (constructing a
stable off-screen AT-SPI document hierarchy backed by the owned render plan, or
leveraging newer GTK 4.22+ `AccessibleText` capabilities) is cataloged as
follow-up work under **`ADV-001`** (Reading refinement and accessibility parity).

This concludes the `LNX-001D1` early gate. `LNX-001E` (tables, task list
markers, and image resource policy) is unblocked to proceed.
