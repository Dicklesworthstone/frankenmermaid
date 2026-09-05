import Foundation
import ImageIO
import UniformTypeIdentifiers

enum MermaidExportKind: String, CaseIterable, Identifiable {
    case source
    case svg
    case png
    case pdf
    case animatedHTML
    case deckHTML

    var id: Self { self }

    var title: String {
        switch self {
        case .source: "Mermaid Source"
        case .svg: "Vector SVG"
        case .png: "Raster PNG (2x)"
        case .pdf: "PDF Document"
        case .animatedHTML: "Animated Web Page"
        case .deckHTML: "Graph Deck Presentation"
        }
    }

    var symbol: String {
        switch self {
        case .source: "chevron.left.forwardslash.chevron.right"
        case .svg: "scribble.variable"
        case .png: "photo"
        case .pdf: "doc.richtext"
        case .animatedHTML: "sparkles.rectangle.stack"
        case .deckHTML: "rectangle.on.rectangle.angled"
        }
    }

    var fileExtension: String {
        switch self {
        case .source: "mmd"
        case .svg: "svg"
        case .png: "png"
        case .pdf: "pdf"
        case .animatedHTML, .deckHTML: "html"
        }
    }
}

enum MermaidExportCodec {
    static let maximumBytes = 32 * 1_024 * 1_024
    private static let maximumPNGDimension = 4_096
    private static let maximumPNGPixelCount = 16_000_000
    private static let pngPrefix = "data:image/png;base64,"

    static func decodePNGDataURL(_ value: String) throws -> Data {
        guard value.hasPrefix(pngPrefix) else { throw MermaidExportError.invalidPNG }
        let payload = value.dropFirst(pngPrefix.count)
        // A base64 string cannot legitimately need more than 4/3 of the byte
        // budget plus padding. Reject it before asking Foundation to allocate.
        let maximumCharacters = ((maximumBytes + 2) / 3) * 4
        guard !payload.isEmpty, payload.utf8.count <= maximumCharacters else {
            throw MermaidExportError.tooLarge
        }
        guard let data = Data(base64Encoded: String(payload)) else {
            throw MermaidExportError.invalidPNG
        }
        try validateSize(data)
        try validatePNG(data)
        return data
    }

    static func validateSize(_ data: Data) throws {
        guard !data.isEmpty else { throw MermaidExportError.missingArtifact }
        guard data.count <= maximumBytes else { throw MermaidExportError.tooLarge }
    }

    private static func validatePNG(_ data: Data) throws {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(data as CFData, options),
              CGImageSourceGetCount(source) == 1,
              let type = CGImageSourceGetType(source),
              type as String == UTType.png.identifier,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, options) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0,
              width <= maximumPNGDimension, height <= maximumPNGDimension,
              width * height <= maximumPNGPixelCount,
              CGImageSourceCreateImageAtIndex(source, 0, options) != nil else {
            throw MermaidExportError.invalidPNG
        }
    }
}

enum MermaidExportError: LocalizedError {
    case invalidPNG
    case missingArtifact
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .invalidPNG:
            "The renderer returned an invalid PNG image."
        case .missingArtifact:
            "The rendered diagram was not available to export."
        case .tooLarge:
            "That export exceeds the 32 MB sharing limit."
        }
    }
}
