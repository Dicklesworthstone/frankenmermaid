import Foundation
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let mermaidSource = UTType(
        exportedAs: "com.frankenmermaid.source",
        conformingTo: .plainText
    )
}

struct MermaidSourceFile: FileDocument {
    static var readableContentTypes: [UTType] { [.mermaidSource, .plainText] }

    let source: String

    init(source: String) {
        self.source = source
    }

    init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents else {
            throw SourceImportError.notAFile
        }
        source = try MermaidSourceLoader.decode(data)
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: try MermaidSourceLoader.encode(source))
    }
}

struct MermaidOpenedDocument: Equatable, Sendable {
    let url: URL
    let bookmarkData: Data
    let source: String
    let diskData: Data

    var displayName: String { url.lastPathComponent }
}

struct MermaidRecentDocument: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let bookmarkData: Data
    let displayName: String
    let pathHint: String
    let lastOpenedAt: Date
}

enum MermaidDocumentAttention: Equatable {
    case changedOnDisk
    case unavailable
}

@MainActor
final class MermaidDocumentSession: ObservableObject {
    static let recentsStorageKey = "frankenmermaid.recentSourceDocuments.v1"
    static let maximumRecentDocuments = 6

    @Published private(set) var currentDocument: MermaidOpenedDocument?
    @Published private(set) var recentDocuments: [MermaidRecentDocument]
    @Published private(set) var isSaving = false
    @Published private(set) var attention: MermaidDocumentAttention?

    private let untitledBaseline: String
    private let defaults: UserDefaults

    init(initialSource: String, defaults: UserDefaults = .standard) {
        untitledBaseline = initialSource
        self.defaults = defaults
        recentDocuments = Self.loadRecents(from: defaults)
    }

    var displayName: String { currentDocument?.displayName ?? "Untitled Diagram" }
    var hasCurrentDocument: Bool { currentDocument != nil }

    func isDirty(source: String) -> Bool {
        source != (currentDocument?.source ?? untitledBaseline)
    }

    func adopt(_ document: MermaidOpenedDocument) {
        currentDocument = document
        attention = nil
        recordRecent(document)
    }

    func openRecent(_ recent: MermaidRecentDocument) async throws -> MermaidOpenedDocument {
        let url = try MermaidSourceLoader.resolveBookmark(recent.bookmarkData)
        return try await MermaidSourceLoader.open(from: url)
    }

    func save(source: String) async throws {
        guard let currentDocument else { throw SourceDocumentError.noCurrentDocument }
        guard !isSaving else { return }
        isSaving = true
        defer { isSaving = false }
        do {
            let saved = try await MermaidSourceLoader.save(source, replacing: currentDocument)
            self.currentDocument = saved
            attention = nil
            recordRecent(saved)
        } catch {
            if error as? SourceDocumentError == .changedOnDisk {
                attention = .changedOnDisk
            } else if Self.isUnavailableFileError(error) {
                attention = .unavailable
            }
            throw error
        }
    }

    func suggestedFilename() -> String {
        currentDocument?.url.deletingPathExtension().lastPathComponent ?? "Untitled Diagram"
    }

    private func recordRecent(_ document: MermaidOpenedDocument) {
        let path = document.url.standardizedFileURL.path
        let recent = MermaidRecentDocument(
            id: recentDocuments.first(where: { $0.pathHint == path })?.id ?? UUID(),
            bookmarkData: document.bookmarkData,
            displayName: document.displayName,
            pathHint: path,
            lastOpenedAt: Date()
        )
        recentDocuments.removeAll { $0.pathHint == path }
        recentDocuments.insert(recent, at: 0)
        if recentDocuments.count > Self.maximumRecentDocuments {
            recentDocuments = Array(recentDocuments.prefix(Self.maximumRecentDocuments))
        }
        if let encoded = try? JSONEncoder().encode(recentDocuments) {
            defaults.set(encoded, forKey: Self.recentsStorageKey)
        }
    }

    private static func loadRecents(from defaults: UserDefaults) -> [MermaidRecentDocument] {
        guard let data = defaults.data(forKey: recentsStorageKey),
              let decoded = try? JSONDecoder().decode([MermaidRecentDocument].self, from: data) else {
            return []
        }
        return Array(decoded.prefix(maximumRecentDocuments))
    }

    private static func isUnavailableFileError(_ error: Error) -> Bool {
        let cocoaError = error as NSError
        guard cocoaError.domain == NSCocoaErrorDomain else { return false }
        return [
            CocoaError.Code.fileNoSuchFile.rawValue,
            CocoaError.Code.fileReadNoSuchFile.rawValue,
            CocoaError.Code.fileReadNoPermission.rawValue,
            CocoaError.Code.fileWriteNoPermission.rawValue
        ].contains(cocoaError.code)
    }
}

enum MermaidSourceLoader {
    static let maximumBytes = 2 * 1_024 * 1_024
    private static let utf8ByteOrderMark = Data([0xEF, 0xBB, 0xBF])

    static func load(from url: URL) async throws -> String {
        try await open(from: url).source
    }

    static func open(from url: URL) async throws -> MermaidOpenedDocument {
        return try await Task.detached(priority: .userInitiated) {
            try withSecurityScopedAccess(to: url) {
                let data = try coordinatedRead(from: url)
                return MermaidOpenedDocument(
                    url: url,
                    bookmarkData: try bookmark(for: url),
                    source: try decode(data),
                    diskData: data
                )
            }
        }.value
    }

    static func save(
        _ source: String,
        replacing document: MermaidOpenedDocument
    ) async throws -> MermaidOpenedDocument {
        try await Task.detached(priority: .userInitiated) {
            try withSecurityScopedAccess(to: document.url) {
                let bytes = try encode(
                    source,
                    includingByteOrderMark: document.diskData.starts(with: utf8ByteOrderMark)
                )
                try coordinatedReplace(
                    at: document.url,
                    expectedData: document.diskData,
                    replacementData: bytes
                )
                return MermaidOpenedDocument(
                    url: document.url,
                    bookmarkData: try bookmark(for: document.url),
                    source: source,
                    diskData: bytes
                )
            }
        }.value
    }

    static func resolveBookmark(_ data: Data) throws -> URL {
        var stale = false
        var options: URL.BookmarkResolutionOptions = [
            .withoutUI,
            .withoutImplicitStartAccessing
        ]
#if targetEnvironment(macCatalyst)
        options.insert(.withSecurityScope)
#endif
        return try URL(
            resolvingBookmarkData: data,
            options: options,
            relativeTo: nil,
            bookmarkDataIsStale: &stale
        )
    }

    static func decode(_ data: Data) throws -> String {
        guard data.count <= maximumBytes else { throw SourceImportError.tooLarge }
        var bytes = data
        if bytes.starts(with: utf8ByteOrderMark) { bytes.removeFirst(3) }
        guard let source = String(data: bytes, encoding: .utf8) else {
            throw SourceImportError.notUTF8
        }
        guard !source.unicodeScalars.contains(where: { $0.value == 0 }) else {
            throw SourceImportError.containsNull
        }
        return source
    }

    static func encode(_ source: String, includingByteOrderMark: Bool = false) throws -> Data {
        var data = Data(source.utf8)
        if includingByteOrderMark { data.insert(contentsOf: utf8ByteOrderMark, at: 0) }
        guard data.count <= maximumBytes else { throw SourceImportError.tooLarge }
        return data
    }

    private static func bookmark(for url: URL) throws -> Data {
#if targetEnvironment(macCatalyst)
        let options: URL.BookmarkCreationOptions = [.withSecurityScope]
#else
        let options: URL.BookmarkCreationOptions = []
#endif
        return try url.bookmarkData(
            options: options,
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        )
    }

    private static func withSecurityScopedAccess<T>(
        to url: URL,
        operation: () throws -> T
    ) throws -> T {
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        return try operation()
    }

    private static func coordinatedRead(from url: URL) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile != false else { throw SourceImportError.notAFile }
        if let size = values.fileSize, size > maximumBytes { throw SourceImportError.tooLarge }

        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<Data, Error>?
        coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError) { coordinatedURL in
            result = Result { try Data(contentsOf: coordinatedURL, options: .mappedIfSafe) }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw SourceDocumentError.coordinationFailed }
        return try result.get()
    }

    private static func coordinatedReplace(
        at url: URL,
        expectedData: Data,
        replacementData: Data
    ) throws {
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<Void, Error>?
        coordinator.coordinate(
            writingItemAt: url,
            options: .forReplacing,
            error: &coordinationError
        ) { coordinatedURL in
            result = Result {
                let currentData = try Data(contentsOf: coordinatedURL, options: .mappedIfSafe)
                guard currentData == expectedData else { throw SourceDocumentError.changedOnDisk }
                try replacementData.write(to: coordinatedURL, options: .atomic)
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw SourceDocumentError.coordinationFailed }
        try result.get()
    }
}

enum SourceDocumentError: LocalizedError, Equatable {
    case noCurrentDocument
    case changedOnDisk
    case coordinationFailed
    case savedCopyMismatch

    var errorDescription: String? {
        switch self {
        case .noCurrentDocument:
            "Choose Save to create a Mermaid file first."
        case .changedOnDisk:
            "That file changed outside FrankenMermaid. Your edits were not overwritten. " +
                "Save a copy, or reopen the file to use its newer contents."
        case .coordinationFailed:
            "The document provider did not complete the coordinated file operation. " +
                "Your source was not changed."
        case .savedCopyMismatch:
            "The saved file did not contain the source currently in the editor, so FrankenMermaid " +
                "left the document association unchanged."
        }
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
