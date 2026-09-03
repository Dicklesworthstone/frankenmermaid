import Foundation

enum MermaidSourceLoader {
    static let maximumBytes = 2 * 1_024 * 1_024

    static func load(from url: URL) async throws -> String {
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        return try await Task.detached(priority: .userInitiated) {
            let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
            guard values.isRegularFile != false else { throw SourceImportError.notAFile }
            if let size = values.fileSize, size > maximumBytes {
                throw SourceImportError.tooLarge
            }
            let data = try Data(contentsOf: url, options: .mappedIfSafe)
            return try decode(data)
        }.value
    }

    static func decode(_ data: Data) throws -> String {
        guard data.count <= maximumBytes else { throw SourceImportError.tooLarge }
        var bytes = data
        if bytes.starts(with: [0xEF, 0xBB, 0xBF]) { bytes.removeFirst(3) }
        guard let source = String(data: bytes, encoding: .utf8) else {
            throw SourceImportError.notUTF8
        }
        guard !source.unicodeScalars.contains(where: { $0.value == 0 }) else {
            throw SourceImportError.containsNull
        }
        return source
    }
}

enum SourceImportError: LocalizedError {
    case notAFile
    case tooLarge
    case notUTF8
    case containsNull

    var errorDescription: String? {
        switch self {
        case .notAFile:
            "Choose a Mermaid text file, not a folder."
        case .tooLarge:
            "That source is larger than the 2 MB editor limit."
        case .notUTF8:
            "That file is not valid UTF-8 Mermaid source."
        case .containsNull:
            "That file contains binary null bytes and cannot be opened as Mermaid source."
        }
    }
}
