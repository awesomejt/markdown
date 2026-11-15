import Foundation
import MarkdownDocument

/// A way back from a document or folder that can't be opened (MAC-002A),
/// offered as buttons under the window's error.
enum RecoveryAction: Int, CaseIterable, Sendable {
  /// Choose the item at its new location.
  case locate
  /// Choose the item (or its folder) again, which grants access again.
  case grantAccess
  /// Read it again, as after reconnecting a disk.
  case tryAgain
  case removeRecent

  var title: String {
    switch self {
    case .locate: "Locate…"
    case .grantAccess: "Grant Access…"
    case .tryAgain: "Try Again"
    case .removeRecent: "Remove from Recents"
    }
  }

  var identifier: String {
    switch self {
    case .locate: "recovery-locate"
    case .grantAccess: "recovery-grant-access"
    case .tryAgain: "recovery-try-again"
    case .removeRecent: "recovery-remove-recent"
    }
  }
}

/// What failed in a window, and what it offers.
struct Recovery {
  enum Subject {
    case document(DocumentCapability)
    case folder(FolderCapability)
  }

  let subject: Subject
  let actions: [RecoveryAction]
}

/// A recent item a window is showing or recovering.
struct RecentReference: Equatable {
  let id: UUID
  let kind: RecentItem.Kind
}

extension DocumentError {
  var recoveryActions: [RecoveryAction] {
    switch self {
    case .notFound: [.locate]
    case .permissionDenied, .accessRevoked: [.grantAccess]
    case .volumeUnavailable: [.tryAgain, .locate]
    default: []
    }
  }

  init?(_ status: RecentStatus) {
    switch status {
    case .available, .moved: return nil
    case .missing: self = .notFound
    case .volumeUnavailable: self = .volumeUnavailable
    case .permissionDenied: self = .permissionDenied
    }
  }
}

extension FolderError {
  var recoveryActions: [RecoveryAction] {
    switch self {
    case .notFound: [.locate]
    case .permissionDenied, .accessRevoked: [.grantAccess]
    case .volumeUnavailable: [.tryAgain, .locate]
    case .notDirectory, .outsideFolder, .unreadable: []
    }
  }

  init?(_ status: RecentStatus) {
    switch status {
    case .available, .moved: return nil
    case .missing: self = .notFound
    case .volumeUnavailable: self = .volumeUnavailable
    case .permissionDenied: self = .permissionDenied
    }
  }
}
