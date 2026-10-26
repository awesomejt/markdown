// Compile/link smoke only; not a simulator launch or device/runtime proof.
import JavaScriptCore
import WebKit

public func frameworkSmoke() -> Bool {
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    return JSContext() != nil
}
