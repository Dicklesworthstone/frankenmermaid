import Foundation

enum MermaidExportKind: String, CaseIterable, Identifiable {
    case source
    case svg
    case png
    case pdf
    case animatedHTML

    var id: Self { self }

    var title: String {
        switch self {
        case .source: "Mermaid Source"
        case .svg: "Vector SVG"
        case .png: "Raster PNG (2x)"
        case .pdf: "PDF Document"
        case .animatedHTML: "Animated Web Page"
        }
    }

    var symbol: String {
        switch self {
        case .source: "chevron.left.forwardslash.chevron.right"
        case .svg: "scribble.variable"
        case .png: "photo"
        case .pdf: "doc.richtext"
        case .animatedHTML: "sparkles.rectangle.stack"
        }
    }

    var fileExtension: String {
        switch self {
        case .source: "mmd"
        case .svg: "svg"
        case .png: "png"
        case .pdf: "pdf"
        case .animatedHTML: "html"
        }
    }
}

enum MermaidExportCodec {
    static let maximumBytes = 32 * 1_024 * 1_024
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
        guard let data = Data(base64Encoded: String(payload)),
              data.starts(with: [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) else {
            throw MermaidExportError.invalidPNG
        }
        try validateSize(data)
        return data
    }

    static func validateSize(_ data: Data) throws {
        guard !data.isEmpty else { throw MermaidExportError.missingArtifact }
        guard data.count <= maximumBytes else { throw MermaidExportError.tooLarge }
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
