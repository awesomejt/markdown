import AppKit

/// Where settings are kept: user defaults, or memory for the development
/// checks, which must never read or write the user's settings.
public protocol PreferenceStore: AnyObject {
  func object(forKey key: String) -> Any?
  func set(_ value: Any?, forKey key: String)
  func removeObject(forKey key: String)
}

extension UserDefaults: PreferenceStore {}

public final class MemoryPreferenceStore: PreferenceStore {
  public private(set) var values: [String: Any]
  public init(_ values: [String: Any] = [:]) { self.values = values }
  public func object(forKey key: String) -> Any? { values[key] }
  public func set(_ value: Any?, forKey key: String) { values[key] = value }
  public func removeObject(forKey key: String) { values[key] = nil }
}

/// App-wide reading settings, kept in user defaults (MAC-001F, MAC-002D):
/// appearance, text size, whether new windows show the sidebar and whether
/// windows reopen at launch.
///
/// The stored settings are versioned. Version 1 is MAC-001F's unversioned
/// `appearance` and `textScale`; opening it upgrades it to version 2 in
/// place, keeping valid values, snapping a text size to its nearest step and
/// dropping malformed ones. A newer version, written by a later build, is
/// left as it is: its known settings are read and changed but its version
/// and other keys are never rewritten. A missing setting means its default,
/// so Restore Defaults removes them, and every read validates the stored
/// value, so a hand-edited one falls back to its default.
@MainActor
public final class ReaderPreferences {
  public enum Appearance: String, CaseIterable, Sendable {
    case system, light, dark

    public var title: String {
      switch self {
      case .system: "System"
      case .light: "Light"
      case .dark: "Dark"
      }
    }

    /// The application appearance; nil follows the system setting.
    public var appearance: NSAppearance? {
      switch self {
      case .system: nil
      case .light: NSAppearance(named: .aqua)
      case .dark: NSAppearance(named: .darkAqua)
      }
    }
  }

  /// What opening the store did to its version.
  public enum Migration: Equatable, Sendable {
    /// Already the current version, or a new empty store.
    case current
    /// Upgraded in place from this version.
    case upgraded(from: Int)
    /// A later version, left as it is.
    case newer(Int)
  }

  public static let schemaVersion = 2
  public static let versionKey = "preferencesVersion"
  public static let appearanceKey = "appearance"
  public static let textScaleKey = "textScale"
  public static let showsSidebarKey = "showsSidebar"
  public static let reopensWindowsKey = "reopensWindows"
  /// Every setting Restore Defaults clears; the version stays.
  public static let settingKeys = [appearanceKey, textScaleKey, showsSidebarKey, reopensWindowsKey]

  public let store: PreferenceStore
  public let migration: Migration
  /// Called once after any setting changes.
  public var onChange: (() -> Void)?

  public init(store: PreferenceStore) {
    self.store = store
    migration = Self.migrate(store)
  }

  /// The stored version: absent (MAC-001F) or malformed counts as 1.
  public static func storedVersion(_ store: PreferenceStore) -> Int {
    guard let number = store.object(forKey: versionKey) as? NSNumber,
      CFGetTypeID(number) == CFNumberGetTypeID(), !CFNumberIsFloatType(number),
      number.intValue >= 1
    else { return 1 }
    return number.intValue
  }

  private static func migrate(_ store: PreferenceStore) -> Migration {
    let version = storedVersion(store)
    if version > schemaVersion { return .newer(version) }
    if version == schemaVersion { return .current }
    let isNew = settingKeys.allSatisfy { store.object(forKey: $0) == nil }
    // Version 1 to 2: normalize the MAC-001F settings; the others are new.
    if let value = store.object(forKey: appearanceKey),
      (value as? String).flatMap(Appearance.init) == nil
    {
      store.removeObject(forKey: appearanceKey)
    }
    if let value = store.object(forKey: textScaleKey) {
      if let scale = number(value) {
        store.set(Double(TextScale.clamped(scale)), forKey: textScaleKey)
      } else {
        store.removeObject(forKey: textScaleKey)
      }
    }
    for key in [showsSidebarKey, reopensWindowsKey] where bool(store.object(forKey: key)) == nil {
      store.removeObject(forKey: key)
    }
    store.set(schemaVersion, forKey: versionKey)
    return isNew ? .current : .upgraded(from: version)
  }

  /// A finite number, not a Boolean or a string.
  private static func number(_ value: Any?) -> CGFloat? {
    guard let number = value as? NSNumber, CFGetTypeID(number) == CFNumberGetTypeID() else {
      return nil
    }
    let double = number.doubleValue
    return double.isFinite ? CGFloat(double) : nil
  }

  /// A stored Boolean (a property-list Boolean, not a number or string).
  private static func bool(_ value: Any?) -> Bool? {
    guard let number = value as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else {
      return nil
    }
    return number.boolValue
  }

  public var appearance: Appearance {
    get {
      (store.object(forKey: Self.appearanceKey) as? String).flatMap(Appearance.init) ?? .system
    }
    set {
      guard newValue != appearance else { return }
      store.set(newValue.rawValue, forKey: Self.appearanceKey)
      onChange?()
    }
  }

  public var textScale: CGFloat {
    get {
      Self.number(store.object(forKey: Self.textScaleKey)).map(TextScale.clamped)
        ?? TextScale.standard
    }
    set {
      let value = TextScale.clamped(newValue)
      guard value != textScale else { return }
      store.set(Double(value), forKey: Self.textScaleKey)
      onChange?()
    }
  }

  /// Whether a new window shows its sidebar; a restored one keeps its own.
  public var showsSidebar: Bool {
    get { Self.bool(store.object(forKey: Self.showsSidebarKey)) ?? true }
    set {
      guard newValue != showsSidebar else { return }
      store.set(newValue, forKey: Self.showsSidebarKey)
      onChange?()
    }
  }

  /// Whether windows are saved at quit and reopened at launch. macOS's
  /// "Close windows when quitting an application" setting still applies.
  public var reopensWindows: Bool {
    get { Self.bool(store.object(forKey: Self.reopensWindowsKey)) ?? true }
    set {
      guard newValue != reopensWindows else { return }
      store.set(newValue, forKey: Self.reopensWindowsKey)
      onChange?()
    }
  }

  /// Whether every setting has its default value.
  public var isDefault: Bool {
    appearance == .system && textScale == TextScale.standard && showsSidebar && reopensWindows
  }

  /// Settings → Restore Defaults: clear every setting, keeping the version.
  public func restoreDefaults() {
    guard Self.settingKeys.contains(where: { store.object(forKey: $0) != nil }) else { return }
    for key in Self.settingKeys { store.removeObject(forKey: key) }
    onChange?()
  }
}
