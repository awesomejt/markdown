import AppKit
import Foundation
import Testing

@testable import MarkdownRender

/// Persistent settings (MAC-002D): the versioned schema, the upgrade from
/// MAC-001F's unversioned settings, newer versions, validation of stored
/// values and Restore Defaults. Real user defaults across launches are the
/// preferences check's job (tools/check.py), which can delete its suite
/// through cfprefsd; a test-made suite leaves an empty property list.
@Suite(.serialized)
@MainActor
struct PreferencesTests {
  private typealias Preferences = ReaderPreferences

  @Test func newStoreHasDefaultsAndTheCurrentVersion() {
    let store = MemoryPreferenceStore()
    let preferences = Preferences(store: store)
    #expect(preferences.migration == .current)
    #expect(store.values as NSDictionary == [Preferences.versionKey: 2] as NSDictionary)
    #expect(preferences.isDefault)
    #expect(preferences.appearance == .system && preferences.textScale == 1)
    #expect(preferences.showsSidebar && preferences.reopensWindows)
  }

  @Test func upgradesVersionOneSettings() {
    let store = MemoryPreferenceStore([
      Preferences.appearanceKey: "dark", Preferences.textScaleKey: 1.2,
    ])
    let preferences = Preferences(store: store)
    #expect(preferences.migration == .upgraded(from: 1))
    #expect(Preferences.storedVersion(store) == 2)
    #expect(store.object(forKey: Preferences.textScaleKey) as? Double == 1.15)
    #expect(preferences.appearance == .dark && preferences.textScale == 1.15)
    #expect(preferences.showsSidebar && preferences.reopensWindows)
    // Opening it again changes nothing.
    let again = Preferences(store: store)
    #expect(again.migration == .current)
    #expect(store.values.count == 3)
  }

  @Test func upgradeDropsMalformedSettings() {
    let store = MemoryPreferenceStore([
      Preferences.appearanceKey: "sepia", Preferences.textScaleKey: "big",
      Preferences.showsSidebarKey: "maybe", Preferences.reopensWindowsKey: 1,
      "unrelated": "kept",
    ])
    let preferences = Preferences(store: store)
    #expect(preferences.migration == .upgraded(from: 1))
    #expect(
      store.values as NSDictionary
        == [Preferences.versionKey: 2, "unrelated": "kept"] as NSDictionary)
    #expect(preferences.isDefault)
  }

  @Test func malformedVersionsCountAsVersionOne() {
    for version: Any in ["2", 0, -3, 2.5, true] {
      let store = MemoryPreferenceStore([
        Preferences.versionKey: version, Preferences.textScaleKey: Double.infinity,
      ])
      let preferences = Preferences(store: store)
      #expect(preferences.migration == .upgraded(from: 1), "version \(version)")
      #expect(Preferences.storedVersion(store) == 2)
      #expect(store.object(forKey: Preferences.textScaleKey) == nil)
    }
  }

  @Test func newerVersionsAreLeftAsTheyAre() {
    let values: [String: Any] = [
      Preferences.versionKey: 3, Preferences.appearanceKey: "light",
      Preferences.textScaleKey: 1.2, Preferences.showsSidebarKey: false,
      "laterSetting": ["a": 1],
    ]
    let store = MemoryPreferenceStore(values)
    let preferences = Preferences(store: store)
    #expect(preferences.migration == .newer(3))
    #expect(store.values as NSDictionary == values as NSDictionary)
    // Known settings are read, validated and changed; nothing else is.
    #expect(preferences.appearance == .light && preferences.textScale == 1.15)
    #expect(!preferences.showsSidebar && preferences.reopensWindows)
    preferences.reopensWindows = false
    #expect(Preferences.storedVersion(store) == 3)
    #expect(store.object(forKey: "laterSetting") != nil)
  }

  @Test func readsValidateHandEditedValues() {
    let store = MemoryPreferenceStore([Preferences.versionKey: 2])
    let preferences = Preferences(store: store)
    store.set("purple", forKey: Preferences.appearanceKey)
    store.set(9.0, forKey: Preferences.textScaleKey)
    store.set("no", forKey: Preferences.showsSidebarKey)
    store.set(0, forKey: Preferences.reopensWindowsKey)
    #expect(preferences.appearance == .system)
    #expect(preferences.textScale == TextScale.steps.last)
    #expect(preferences.showsSidebar && preferences.reopensWindows)
    store.set(true, forKey: Preferences.textScaleKey)
    #expect(preferences.textScale == 1)
  }

  @Test func changesNotifyOnceAndRestoreDefaultsClearsSettings() {
    let store = MemoryPreferenceStore()
    let preferences = Preferences(store: store)
    var changes = 0
    preferences.onChange = { changes += 1 }
    preferences.appearance = .dark
    preferences.appearance = .dark
    preferences.textScale = 1.3
    preferences.textScale = 1.31
    preferences.showsSidebar = false
    preferences.showsSidebar = false
    preferences.reopensWindows = false
    #expect(changes == 4)
    #expect(store.object(forKey: Preferences.showsSidebarKey) as? Bool == false)
    #expect(!preferences.isDefault)
    preferences.restoreDefaults()
    #expect(changes == 5)
    #expect(preferences.isDefault)
    #expect(store.values as NSDictionary == [Preferences.versionKey: 2] as NSDictionary)
    preferences.restoreDefaults()
    #expect(changes == 5)
  }
}
