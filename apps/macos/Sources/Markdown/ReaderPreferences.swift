import AppKit
import MarkdownRender

/// Where settings are kept: user defaults, or memory for the smoke, which
/// must never write the user's settings.
protocol PreferenceStore: AnyObject {
  func object(forKey key: String) -> Any?
  func set(_ value: Any?, forKey key: String)
}

extension UserDefaults: PreferenceStore {}

final class MemoryPreferenceStore: PreferenceStore {
  private var values: [String: Any] = [:]
  func object(forKey key: String) -> Any? { values[key] }
  func set(_ value: Any?, forKey key: String) { values[key] = value }
}

/// App-wide reading settings (MAC-001F): appearance and text size, kept in
/// user defaults. Stored values are validated on every read, so a missing or
/// hand-edited value falls back to the system appearance and actual size.
@MainActor
final class ReaderPreferences {
  enum Appearance: String, CaseIterable {
    case system, light, dark

    var title: String {
      switch self {
      case .system: "System"
      case .light: "Light"
      case .dark: "Dark"
      }
    }

    /// The application appearance; nil follows the system setting.
    var appearance: NSAppearance? {
      switch self {
      case .system: nil
      case .light: NSAppearance(named: .aqua)
      case .dark: NSAppearance(named: .darkAqua)
      }
    }
  }

  static let appearanceKey = "appearance"
  static let textScaleKey = "textScale"

  let store: PreferenceStore
  /// Called after either setting changes.
  var onChange: (() -> Void)?

  init(store: PreferenceStore) {
    self.store = store
  }

  var appearance: Appearance {
    get {
      (store.object(forKey: Self.appearanceKey) as? String).flatMap(Appearance.init) ?? .system
    }
    set {
      guard newValue != appearance else { return }
      store.set(newValue.rawValue, forKey: Self.appearanceKey)
      onChange?()
    }
  }

  var textScale: CGFloat {
    get {
      let stored = store.object(forKey: Self.textScaleKey) as? Double
      return TextScale.clamped(stored.map { CGFloat($0) } ?? TextScale.standard)
    }
    set {
      let value = TextScale.clamped(newValue)
      guard value != textScale else { return }
      store.set(Double(value), forKey: Self.textScaleKey)
      onChange?()
    }
  }
}
