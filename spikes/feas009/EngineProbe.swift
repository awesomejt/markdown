// Disposable compiler screen. No document renderer or production security boundary.
import AppKit
import JavaScriptCore
import WebKit

func emit(_ value: Any) {
    do {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        print(String(decoding: data, as: UTF8.self))
    } catch { fputs("JSON serialization failed\n", stderr); exit(1) }
}

let arguments = CommandLine.arguments
guard arguments.count == 4 else {
    fputs("EngineProbe jsc|webkit bundle.js probe.js\n", stderr); exit(2)
}
let bundle = try String(contentsOfFile: arguments[2], encoding: .utf8)
let probe = try String(contentsOfFile: arguments[3], encoding: .utf8)
let started = ProcessInfo.processInfo.systemUptime

if arguments[1] == "jsc" {
    guard let context = JSContext() else { exit(1) }
    context.exceptionHandler = { _, exception in
        fputs("JavaScript exception: \(exception?.toString() ?? "unknown")\n", stderr)
    }
    let clock: @convention(block) () -> Double = { ProcessInfo.processInfo.systemUptime * 1000 }
    context.setObject(clock, forKeyedSubscript: "clockNow" as NSString)
    context.evaluateScript(bundle)
    guard context.exception == nil else { exit(1) }
    guard let output = context.evaluateScript(probe)?.toString(), context.exception == nil,
          let data = output.data(using: .utf8),
          var result = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { exit(1) }
    result["host_elapsed_ms"] = (ProcessInfo.processInfo.systemUptime - started) * 1000
    emit(result)
} else if arguments[1] == "webkit" {
    final class Driver: NSObject, WKNavigationDelegate {
        let view: WKWebView
        let source: String
        var timer: Timer?
        init(bundle: String, source: String) {
            self.source = source
            let configuration = WKWebViewConfiguration()
            configuration.websiteDataStore = .nonPersistent()
            configuration.userContentController.addUserScript(WKUserScript(
                source: bundle, injectionTime: .atDocumentEnd, forMainFrameOnly: true))
            view = WKWebView(frame: NSRect(x: 0, y: 0, width: 900, height: 700), configuration: configuration)
            super.init()
            view.navigationDelegate = self
        }
        func start() {
            timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: false) { _ in
                emit(["error": "WebKit probe timeout"]); exit(1)
            }
            // All executable code is trusted/pinned. Source cases are passed as JS strings.
            // CSP denies network resources; this is a test harness, not an SVG sanitizer.
            view.loadHTMLString("""
            <!doctype html><meta http-equiv="Content-Security-Policy"
            content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'">
            <body><div id="output"></div></body>
            """, baseURL: nil)
        }
        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            webView.callAsyncJavaScript(source, arguments: [:], in: nil, in: .page) { result in
                self.timer?.invalidate()
                switch result {
                case .success(let value):
                    guard var output = value as? [String: Any] else {
                        emit(["error": "non-object result"]); exit(1)
                    }
                    output["host_elapsed_ms"] = (ProcessInfo.processInfo.systemUptime - started) * 1000
                    emit(output); exit(0)
                case .failure(let error): emit(["error": error.localizedDescription]); exit(1)
                }
            }
        }
        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            emit(["error": error.localizedDescription]); exit(1)
        }
    }
    let app = NSApplication.shared
    app.setActivationPolicy(.prohibited)
    let driver = Driver(bundle: bundle, source: probe)
    driver.start()
    app.run()
    withExtendedLifetime(driver) {}
} else { exit(2) }
