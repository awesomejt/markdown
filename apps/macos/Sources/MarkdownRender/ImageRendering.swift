import AppKit
import ImageIO
import UniformTypeIdentifiers

/// Why an image is shown as its alt text instead of a picture.
public enum ImageFailure: Error, Sendable, Equatable {
  /// The core denied a remote image; nothing is fetched.
  case remote
  /// The core denied the target (unsafe, invalid or unsupported).
  case blocked
  /// No image loader was supplied, so local files are not read.
  case notLoaded
  case outsideFolder
  case notFound
  case permissionDenied
  case unsupportedType
  case tooLarge
  /// The document's image count or decoded-memory budget is used up.
  case limit
  case undecodable

  public var message: String {
    switch self {
    case .remote: "Remote images are blocked"
    case .blocked: "This image was blocked"
    case .notLoaded: "Image not shown"
    case .outsideFolder: "Images outside the document’s folder are not shown"
    case .notFound: "Image not found"
    case .permissionDenied: "You don’t have permission to read this image"
    case .unsupportedType: "This image format is not supported"
    case .tooLarge: "This image is too large to show"
    case .limit: "Too many images to show"
    case .undecodable: "This image couldn’t be read"
    }
  }
}

/// Reads the bytes of an image named by the core's decoded relative path, at
/// most `limit` bytes. Called off the main thread during rendering.
public typealias ImageLoader = @Sendable (_ path: String, _ limit: Int) throws(ImageFailure) -> Data

/// Per-document image bounds. Decoding is bounded before any pixels exist:
/// files are size-capped and the declared pixel count is checked from the
/// header. Formats ImageIO cannot subsample (PNG, most GIF/TIFF) are decoded in
/// full before downsampling, so `maxSourcePixels` keeps that transient RGBA
/// copy within 128 MiB; images decode one at a time. The retained copy is
/// downsampled to `maxPixelSide`, and `maxDecodedBytes` bounds retained pixels.
public struct ImageBudget: Sendable {
  public var maxImages = 128
  public var maxFileBytes = 16 * 1024 * 1024
  public var maxSourcePixels = 32 * 1024 * 1024
  public var maxPixelSide = 2048
  /// Retained decoded RGBA bytes for all of one document's images.
  public var maxDecodedBytes = 128 * 1024 * 1024

  public init() {}
}

/// One image occurrence in the rendered text, for checks and diagnostics.
public struct RenderedImage: Sendable, Equatable {
  /// The attachment character, or the alt-text placeholder.
  public let range: NSRange
  public let failure: ImageFailure?
}

/// An image that scales down to the line width and never up, so the reading
/// column decides its size whatever the window width.
final class ImageAttachment: NSTextAttachment {
  /// The image's size at one point per pixel.
  let naturalSize: CGSize

  /// Each occurrence gets its own `NSImage` over the shared decoded pixels,
  /// so its accessibility description is its own alt text.
  init(image: CGImage, description: String) {
    naturalSize = CGSize(width: image.width, height: image.height)
    super.init(data: nil, ofType: nil)
    let image = NSImage(cgImage: image, size: naturalSize)
    image.accessibilityDescription = description
    self.image = image
  }

  required init?(coder: NSCoder) { fatalError("Image attachments are constructed in code") }

  override func attachmentBounds(
    for attributes: [NSAttributedString.Key: Any], location: any NSTextLocation,
    textContainer: NSTextContainer?, proposedLineFragment: CGRect, position: CGPoint
  ) -> CGRect {
    Self.fitted(naturalSize, width: proposedLineFragment.width - position.x)
  }

  static func fitted(_ size: CGSize, width: CGFloat) -> CGRect {
    guard size.width > 0, size.height > 0 else { return .zero }
    let scale = min(1, max(width, 1) / size.width)
    return CGRect(x: 0, y: 0, width: size.width * scale, height: size.height * scale)
  }
}

/// Loads and decodes each distinct image once per render, within the budget.
final class ImageCache {
  private let loader: ImageLoader?
  private let budget: ImageBudget
  private var results: [String: Result<CGImage, ImageFailure>] = [:]
  private var decodedBytes = 0

  init(loader: ImageLoader?, budget: ImageBudget) {
    self.loader = loader
    self.budget = budget
  }

  func image(path: String) -> Result<CGImage, ImageFailure> {
    if let cached = results[path] { return cached }
    let result: Result<CGImage, ImageFailure>
    if results.count >= budget.maxImages {
      result = .failure(.limit)
    } else if let loader {
      result = Result { () throws(ImageFailure) in
        try decode(try loader(path, budget.maxFileBytes))
      }
    } else {
      result = .failure(.notLoaded)
    }
    results[path] = result
    return result
  }

  private func decode(_ data: Data) throws(ImageFailure) -> CGImage {
    let options = [kCGImageSourceShouldCache: false] as CFDictionary
    guard let source = CGImageSourceCreateWithData(data as CFData, options),
      CGImageSourceGetCount(source) > 0
    else { throw .undecodable }
    // Decide from the content, not the name: SVG and non-image data are refused.
    guard let typeName = CGImageSourceGetType(source) as String?,
      let type = UTType(typeName), type.conforms(to: .image), !type.conforms(to: .svg)
    else { throw .unsupportedType }
    guard
      let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, options)
        as? [CFString: Any],
      let width = properties[kCGImagePropertyPixelWidth] as? Int,
      let height = properties[kCGImagePropertyPixelHeight] as? Int, width > 0, height > 0
    else { throw .undecodable }
    let (pixels, overflow) = width.multipliedReportingOverflow(by: height)
    guard !overflow, pixels <= budget.maxSourcePixels else { throw .tooLarge }
    let thumbnail =
      [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
        kCGImageSourceThumbnailMaxPixelSize: min(max(width, height), budget.maxPixelSide),
      ] as CFDictionary
    guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbnail) else {
      throw .undecodable
    }
    let bytes = image.bytesPerRow * image.height
    guard decodedBytes + bytes <= budget.maxDecodedBytes else { throw .limit }
    decodedBytes += bytes
    return image
  }
}
