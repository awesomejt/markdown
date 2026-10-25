import AppKit
import SwiftUI
#if !FEAS006
import Markdown
#endif
import QuartzCore

let processStart = ProcessInfo.processInfo.systemUptime
func now() -> Double { ProcessInfo.processInfo.systemUptime }

// Disposable presentation adapter, not the proposed portable semantic model.
// Links and resources are deliberately inert; no HTML import or network API.
#if !FEAS006
func render(_ node: any Markup) -> NSAttributedString {
    let result = NSMutableAttributedString(string: "")
    if let text = node as? Markdown.Text { result.append(NSAttributedString(string: text.string)) }
    else if let code = node as? CodeBlock { result.append(NSAttributedString(string: code.code)) }
    else if let code = node as? InlineCode { result.append(NSAttributedString(string: code.code)) }
    else if let html = node as? HTMLBlock { result.append(NSAttributedString(string: html.rawHTML)) }
    else if let html = node as? InlineHTML { result.append(NSAttributedString(string: html.rawHTML)) }
    else if node is Markdown.Image { result.append(NSAttributedString(string: "[image omitted]")) }
    else if node is SoftBreak || node is LineBreak { result.append(NSAttributedString(string: "\n")) }
    else {
        if node is ListItem { result.append(NSAttributedString(string: "• ")) }
        for child in node.children { result.append(render(child)) }
    }
    let whole = NSRange(location: 0, length: result.length)
    var font = NSFont.systemFont(ofSize: 16)
    if let heading = node as? Heading { font = .boldSystemFont(ofSize: CGFloat(29 - heading.level * 2)) }
    else if node is CodeBlock || node is InlineCode { font = .monospacedSystemFont(ofSize: 14, weight: .regular) }
    else if node is Strong { font = .boldSystemFont(ofSize: 16) }
    else if node is Emphasis { font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask) }
    // Apply block defaults before inline rendering would overwrite them: only fill
    // unset fonts, except explicit heading/code/strong styles.
    if node is Heading || node is CodeBlock || node is InlineCode || node is Strong || node is Emphasis {
        result.addAttribute(.font, value: font, range: whole)
    } else {
        result.enumerateAttribute(.font, in: whole) { value, range, _ in
            if value == nil { result.addAttribute(.font, value: font, range: range) }
        }
    }
    result.addAttribute(.foregroundColor, value: NSColor.textColor, range: whole)
    if node is Strikethrough { result.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: whole) }
    if node is Markdown.Table.Cell { result.append(NSAttributedString(string: "\t")) }
    if node is Paragraph || node is Heading || node is CodeBlock || node is Markdown.Table.Row || node is ListItem {
        result.append(NSAttributedString(string: "\n\n"))
    }
    return result
}
#endif

final class ProbeTextView: NSTextView {
    var onDraw: (() -> Void)?
    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        onDraw?()
    }
}

final class ProbeTable: NSTableView {
    var onDraw: (() -> Void)?
    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        onDraw?()
    }
}

@MainActor struct Shell: NSViewRepresentable {
    let surface: NSView
    func makeNSView(context: Context) -> NSView { surface }
    func updateNSView(_ view: NSView, context: Context) {}
}

@MainActor final class AppDelegate: NSObject, NSApplicationDelegate, NSTableViewDataSource, NSTableViewDelegate, NSWindowDelegate {
    let args: [String] = {
        if CommandLine.arguments.count >= 3 { return CommandLine.arguments }
        var values = [CommandLine.arguments[0], Bundle.main.object(forInfoDictionaryKey: "ProbeMode") as? String ?? "tk2",
                      Bundle.main.path(forResource: "gfm-tour", ofType: "md") ?? "fixtures/markdown/foundation/gfm-tour.md"]
        if Bundle.main.object(forInfoDictionaryKey: "ProbeSwiftUI") as? Bool == true { values.append("--swiftui") }
        return values
    }()
    var mode = "tk2"
    var shell = "appkit"
    var automated = false
    var resultPath: String?
    var window: NSWindow!
    var scroll = NSScrollView()
    var text: ProbeTextView?
    var table: ProbeTable?
    var blocks: [NSAttributedString] = []
    var heights: [CGFloat] = []
    var joined = NSMutableAttributedString(string: "")
    var headingRanges: [NSRange] = []
    var createdCells = 0
    var metrics: [String: Any] = [:]
    var firstDraw: Double?
    var samples: [Double] = []
    var timer: Timer?
    var lastTick: Double = 0
    var scrollStep = 0
#if FEAS006
    var nativeCancellation: CoreCancellation?
    var loadTimer: Timer?
    var loadTicks = 0
    var lastLoadTick = now()
    var maxLoadGap = 0.0
    var closingDuringLoad = false
    var pendingNativeWork: DispatchWorkItem?
#endif

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard args.count >= 3 else {
            fputs("usage: MacRenderingProbe <tk1|tk2|blocks> <fixture> [--swiftui] [--automated <result.json>]\n", stderr)
            NSApp.terminate(nil); return
        }
        mode = args[1]
        guard ["tk1", "tk2", "blocks"].contains(mode) else { exit(2) }
        shell = args.contains("--swiftui") ? "swiftui" : "appkit"
        if let index = args.firstIndex(of: "--automated"), index + 1 < args.count {
            automated = true; resultPath = args[index + 1]
        }
#if FEAS006
        beginCoreLoad()
#else
        let start = now()
        do {
            let data = try Data(contentsOf: URL(fileURLWithPath: args[2]))
            guard data.count <= 6 * 1024 * 1024, let source = String(data: data, encoding: .utf8) else { exit(2) }
            let readEnd = now()
            let document = Document(parsing: source, options: [.disableSmartOpts])
            let parseEnd = now()
            for child in document.children {
                let block = render(child)
                if child is Heading { headingRanges.append(NSRange(location: joined.length, length: block.length)) }
                joined.append(block)
                // Bound each reusable cell; avoid splitting a UTF-16 surrogate pair.
                let string = block.string as NSString
                var offset = 0
                while offset < string.length {
                    let range = string.rangeOfComposedCharacterSequences(for: NSRange(location: offset, length: min(8192, string.length - offset)))
                    blocks.append(block.attributedSubstring(from: range))
                    offset = NSMaxRange(range)
                }
            }
            metrics = ["mode": mode, "shell": shell, "input_bytes": data.count,
                       "rendered_utf16": joined.length, "blocks": blocks.count,
                       "read_ms": (readEnd - start) * 1000,
                       "parse_ms": (parseEnd - readEnd) * 1000,
                       "adapter_ms": (now() - parseEnd) * 1000]
        } catch { fputs("fixture read failed\n", stderr); exit(2) }
        showSurface()
#endif
    }

    func showSurface() {
        makeMenus()
        if window == nil {
            window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 840, height: 640),
                          styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered, defer: false)
        }
#if FEAS006
        window.title = "Markdown FEAS-006 — Rust / \(mode)"
        window.delegate = self
        metrics["phase"] = "building_ui"
        checkpoint()
#else
        window.title = "Markdown FEAS-004 — \(mode) / \(shell)"
#endif
        window.isReleasedWhenClosed = false
        scroll.hasVerticalScroller = true
        scroll.autoresizingMask = [.width, .height]
        scroll.frame = NSRect(x: 0, y: 0, width: 840, height: 600)
        let setupStart = now()
        if mode == "blocks" {
            let view = ProbeTable(frame: scroll.bounds)
            let column = NSTableColumn(identifier: .init("content"))
            column.width = 800
            view.addTableColumn(column)
            view.headerView = nil
            view.intercellSpacing = .zero
            view.selectionHighlightStyle = .none
            view.dataSource = self; view.delegate = self
            view.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
            view.onDraw = { [weak self] in self?.drew() }
            heights = blocks.map {
                max(30, ceil($0.boundingRect(with: NSSize(width: 780, height: CGFloat.greatestFiniteMagnitude),
                                           options: [.usesLineFragmentOrigin, .usesFontLeading]).height) + 16)
            }
            table = view; scroll.documentView = view
            view.reloadData()
        } else {
            let view = ProbeTextView(usingTextLayoutManager: mode == "tk2")
            configure(view)
            view.frame = scroll.bounds
            view.autoresizingMask = [.width]
            view.isVerticallyResizable = true
            view.isHorizontallyResizable = false
            view.minSize = NSSize(width: 0, height: 600)
            view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
            view.textContainer?.containerSize = NSSize(width: 800, height: CGFloat.greatestFiniteMagnitude)
            view.textContainer?.widthTracksTextView = true
            view.textStorage?.setAttributedString(joined)
            view.onDraw = { [weak self] in self?.drew() }
            text = view; scroll.documentView = view
        }
        metrics["surface_setup_ms"] = (now() - setupStart) * 1000
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 840, height: 640))
        let button = NSButton(title: "Next heading", target: self, action: #selector(nextHeading))
        button.frame = NSRect(x: 10, y: 607, width: 140, height: 28)
        button.autoresizingMask = [.minYMargin]
        let endButton = NSButton(title: "End", target: self, action: #selector(goEnd))
        endButton.frame = NSRect(x: 160, y: 607, width: 80, height: 28)
        endButton.autoresizingMask = [.minYMargin]
        root.addSubview(scroll); root.addSubview(button); root.addSubview(endButton)
        if shell == "swiftui" { window.contentView = NSHostingView(rootView: Shell(surface: root)) }
        else { window.contentView = root }
        window.makeKeyAndOrderFront(nil)
        if let text { window.makeFirstResponder(text) }
        NSApp.activate(ignoringOtherApps: true)
        if automated {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { self.exercise() }
        }
    }

#if FEAS006
    func beginCoreLoad() {
        makeMenus()
        window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 840, height: 640),
                          styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Markdown FEAS-006 — loading Rust snapshot"
        window.delegate = self
        window.isReleasedWhenClosed = false
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 840, height: 640))
        let label = NSTextField(labelWithString: "Loading through Rust C ABI…")
        label.frame = NSRect(x: 20, y: 580, width: 500, height: 30)
        let button = NSButton(title: "Cancel", target: self, action: #selector(cancelCore))
        button.frame = NSRect(x: 20, y: 530, width: 100, height: 30)
        root.addSubview(label); root.addSubview(button); window.contentView = root
        window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
        do { nativeCancellation = try CoreCancellation() }
        catch { metrics["error"] = "cancel token creation"; checkpoint(); NSApp.terminate(nil); return }
        let cancel = nativeCancellation!
        if args.contains("--cancel-before-parse") { _ = cancel.request() }
        metrics = ["mode": mode, "shell": shell, "phase": "native_loading"]
        checkpoint()
        lastLoadTick = now()
        loadTimer = Timer.scheduledTimer(withTimeInterval: 0.005, repeats: true) { _ in
            MainActor.assumeIsolated {
            let current = now(); self.maxLoadGap = max(self.maxLoadGap, current - self.lastLoadTick)
            self.lastLoadTick = current; self.loadTicks += 1
            if let work = self.pendingNativeWork {
                self.pendingNativeWork = nil
                DispatchQueue.global(qos: .userInitiated).async(execute: work)
            }
            if self.args.contains("--cancel-on-entry"), (CoreProbe.active() ?? 0) > 0,
               self.metrics["cancel_requested_after_entry"] == nil {
                self.metrics["cancel_requested_after_entry"] = true
                _ = cancel.request()
            }
            }
        }
        let file = args[2]
        // Start only once the loading window's main run loop has ticked. In
        // particular, a short parse must not finish during AppKit bootstrap
        // before the native-entry cancellation observer can ever execute.
        pendingNativeWork = DispatchWorkItem {
            let start = now()
            do {
                let data = try Data(contentsOf: URL(fileURLWithPath: file))
                let readMS = (now() - start) * 1000
                let document = try Core.parseAndCopy(data, cancellation: cancel)
                cancel.close()
                let live = CoreProbe.live()
                let abi = try Core.abi()
                DispatchQueue.main.async {
                    self.loadTimer?.invalidate(); self.nativeCancellation = nil
                    self.metrics["loading_heartbeat_ticks"] = self.loadTicks
                    self.metrics["loading_max_timer_gap_ms"] = self.maxLoadGap * 1000
                    if self.closingDuringLoad { self.metrics["discarded_after_close"] = true; self.finish(); return }
                    self.metrics["input_bytes"] = data.count
                    self.metrics["read_ms"] = readMS
                    self.metrics["core_parse_model_ms"] = document.coreMS
                    self.metrics["native_call_ms"] = document.nativeCallMS
                    self.metrics["copy_decode_ms"] = document.copyDecodeMS
                    self.metrics["native_text_bytes"] = document.textBytes
                    self.metrics["native_table_bytes"] = document.tableBytes
                    self.metrics["probe_hooks"] = abi.probe_hooks
                    if let live {
                        self.metrics["live_snapshots_before_ui"] = live.0
                        self.metrics["live_tokens_before_ui"] = live.1
                    }
                    self.metrics["phase"] = "appkit_adapter"
                    self.checkpoint()
                    let adapterStart = now()
                    for value in document.blocks {
                        let font: NSFont = value.kind == 1 ? .boldSystemFont(ofSize: CGFloat(29 - value.headingLevel * 2))
                            : value.kind == 2 ? .monospacedSystemFont(ofSize: 14, weight: .regular) : .systemFont(ofSize: 16)
                        let block = NSAttributedString(string: value.text + "\n\n", attributes: [.font: font, .foregroundColor: NSColor.textColor])
                        if value.kind == 1 { self.headingRanges.append(NSRange(location: self.joined.length, length: block.length)) }
                        self.joined.append(block)
                        let string = block.string as NSString
                        var offset = 0
                        while offset < string.length {
                            let range = string.rangeOfComposedCharacterSequences(for: NSRange(location: offset, length: min(8192, string.length - offset)))
                            self.blocks.append(block.attributedSubstring(from: range)); offset = NSMaxRange(range)
                        }
                    }
                    self.metrics["rendered_utf16"] = self.joined.length
                    self.metrics["blocks"] = self.blocks.count
                    self.metrics["adapter_ms"] = (now() - adapterStart) * 1000
                    self.showSurface()
                }
            } catch {
                cancel.close()
                let live = CoreProbe.live()
                DispatchQueue.main.async {
                    self.loadTimer?.invalidate(); self.nativeCancellation = nil
                    self.metrics["phase"] = "native_error"
                    if case CoreFailure.status(let status) = error { self.metrics["core_status"] = status }
                    else { self.metrics["error"] = "Host/core conversion failed" }
                    if let live {
                        self.metrics["live_snapshots_before_ui"] = live.0
                        self.metrics["live_tokens_before_ui"] = live.1
                    }
                    self.metrics["loading_heartbeat_ticks"] = self.loadTicks
                    self.finish()
                }
            }
        }
    }
    @objc func cancelCore() { _ = nativeCancellation?.request() }
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if nativeCancellation != nil { closingDuringLoad = true; cancelCore(); return false }
        return true
    }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if nativeCancellation != nil { closingDuringLoad = true; cancelCore(); return .terminateCancel }
        return .terminateNow
    }
#endif

    func configure(_ view: NSTextView) {
        view.isEditable = false; view.isSelectable = true
        view.isRichText = true
        view.textContainerInset = NSSize(width: 16, height: 8)
        view.setAccessibilityLabel("Markdown document text")
    }
    func drew() {
        if firstDraw == nil { firstDraw = now(); metrics["entry_to_first_draw_ms"] = (firstDraw! - processStart) * 1000 }
    }
    func numberOfRows(in tableView: NSTableView) -> Int { blocks.count }
    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat { heights[row] }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let id = NSUserInterfaceItemIdentifier("block")
        let view: ProbeTextView
        if let reused = tableView.makeView(withIdentifier: id, owner: self) as? ProbeTextView { view = reused }
        else { view = ProbeTextView(usingTextLayoutManager: true); view.identifier = id; createdCells += 1; configure(view) }
        view.textContainer?.containerSize = NSSize(width: 780, height: CGFloat.greatestFiniteMagnitude)
        view.textStorage?.setAttributedString(blocks[row])
        return view
    }
    @objc func nextHeading() {
        guard let text, !headingRanges.isEmpty else { return }
        let range = headingRanges.first(where: { $0.location > text.selectedRange().location }) ?? headingRanges[0]
        text.setSelectedRange(range); text.scrollRangeToVisible(range)
    }
    @objc func goEnd() {
        if let text { text.scrollRangeToVisible(NSRange(location: max(0, joined.length - 1), length: min(1, joined.length))) }
        else if let table, !blocks.isEmpty { table.scrollRowToVisible(blocks.count - 1) }
    }
    func exercise() {
        metrics["first_draw_observed"] = firstDraw != nil
        if let text {
            let range = (joined.string as NSString).rangeOfComposedCharacterSequences(for: NSRange(location: 0, length: min(256, joined.length)))
            text.setSelectedRange(range)
            let pasteboard = NSPasteboard.withUniqueName()
            let types = text.writablePasteboardTypes
            let wrote = text.writeSelection(to: pasteboard, types: types)
            let copied = pasteboard.string(forType: .string) ?? pasteboard.string(forType: .init("NSStringPboardType"))
            metrics["writable_pasteboard_types"] = types.map(\.rawValue)
            metrics["selection_copy_matches"] = wrote && copied == (joined.string as NSString).substring(with: range)
            metrics["selection_write_succeeded"] = wrote
            metrics["selection_actual_utf16"] = text.selectedRange().length
            metrics["selection_requested_utf16"] = range.length
            metrics["clipboard_utf16"] = copied.map { ($0 as NSString).length } ?? -1
            metrics["surface_text_matches"] = text.string == joined.string
            pasteboard.releaseGlobally()
            metrics["ax_role"] = text.accessibilityRole()?.rawValue ?? "nil"
            metrics["ax_utf16"] = text.accessibilityNumberOfCharacters()
            metrics["textkit2_active"] = text.textLayoutManager != nil
            metrics["continuous_selection"] = true
            let navStart = now(); nextHeading(); metrics["heading_navigation_ms"] = (now() - navStart) * 1000
        } else {
            metrics["continuous_selection"] = false
            metrics["ax_role"] = table?.accessibilityRole()?.rawValue ?? "nil"
        }
        checkpoint()
        let start = now(); goEnd(); metrics["end_request_ms"] = (now() - start) * 1000
        checkpoint()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            if let text = self.text {
                let end = max(0, self.joined.length - 1)
                let actual = text.characterIndexForInsertion(at: NSPoint(x: text.visibleRect.maxX - 20, y: text.visibleRect.maxY - 4))
                self.metrics["end_visible_by_hit_test"] = actual >= end
            } else if let table = self.table {
                self.metrics["end_visible_by_row"] = table.rows(in: table.visibleRect).contains(self.blocks.count - 1)
            }
            self.lastTick = now()
            self.timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { _ in
                MainActor.assumeIsolated { self.tick() }
            }
        }
    }
    func tick() {
        let current = now(); samples.append((current - lastTick) * 1000); lastTick = current
        scrollStep += 1
        let height = max(0, (scroll.documentView?.bounds.height ?? 0) - scroll.contentView.bounds.height)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: height * CGFloat(scrollStep % 60) / 59))
        scroll.reflectScrolledClipView(scroll.contentView)
        if scrollStep >= 60 { timer?.invalidate(); finish() }
    }
    func finish() {
        metrics["timer_intervals_ms"] = samples
        metrics["created_cells"] = createdCells
        metrics["elapsed_ms"] = (now() - processStart) * 1000
        metrics["backing_scale"] = window.backingScaleFactor
        metrics["screen_points"] = [window.screen?.frame.width ?? 0, window.screen?.frame.height ?? 0]
        if let screen = window.screen, let id = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID,
           let mode = CGDisplayCopyDisplayMode(id) {
            metrics["screen_pixels"] = [mode.pixelWidth, mode.pixelHeight]
            metrics["refresh_hz"] = mode.refreshRate
        }
        metrics["window_visible"] = window.isVisible
        metrics["completed"] = true
        checkpoint()
        NSApp.terminate(nil)
    }
    func checkpoint() {
        if let path = resultPath, let data = try? JSONSerialization.data(withJSONObject: metrics, options: [.prettyPrinted, .sortedKeys]) {
            try? data.write(to: URL(fileURLWithPath: path), options: .atomic)
        }
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func makeMenus() {
        let menu = NSMenu()
        let appItem = NSMenuItem(); menu.addItem(appItem)
        let appMenu = NSMenu(); appItem.submenu = appMenu
        appMenu.addItem(withTitle: "Quit Probe", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let editItem = NSMenuItem(); menu.addItem(editItem)
        let edit = NSMenu(title: "Edit"); editItem.submenu = edit
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        NSApp.mainMenu = menu
    }
}

MainActor.assumeIsolated {
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)
    let delegate = AppDelegate()
    app.delegate = delegate
    withExtendedLifetime(delegate) { app.run() }
}
