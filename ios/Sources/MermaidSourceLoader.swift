import CryptoKit
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

struct MermaidActiveDraft: Codable, Equatable, Sendable {
    static let currentSchema = 1

    let schema: Int
    let savedAtMilliseconds: Int64
    let documentIdentity: UUID?
    let source: String
}

struct MermaidDraftStore: Sendable {
    enum StoreError: Error, Equatable {
        case invalidDraft
        case oversizedDraft
    }

    static let maximumEncodedBytes = MermaidSourceLoader.maximumBytes + 32 * 1_024

    let fileURL: URL

    init(fileURL: URL? = nil) {
        if let fileURL {
            self.fileURL = fileURL
            return
        }
        let applicationSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.temporaryDirectory
        self.fileURL = applicationSupport
            .appendingPathComponent("FrankenMermaid", isDirectory: true)
            .appendingPathComponent("active-draft.json", isDirectory: false)
    }

    func load() -> MermaidActiveDraft? {
        guard let values = try? fileURL.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]),
              values.isRegularFile == true,
              let fileSize = values.fileSize,
              fileSize > 0,
              fileSize <= Self.maximumEncodedBytes,
              let data = try? Data(contentsOf: fileURL, options: .mappedIfSafe),
              let draft = try? JSONDecoder().decode(MermaidActiveDraft.self, from: data),
              isValid(draft) else {
            return nil
        }
        return draft
    }

    func save(_ draft: MermaidActiveDraft) throws {
        guard isValid(draft) else { throw StoreError.invalidDraft }
        let data = try JSONEncoder().encode(draft)
        guard data.count <= Self.maximumEncodedBytes else { throw StoreError.oversizedDraft }

        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication]
        )
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var mutableDirectory = directory
        try? mutableDirectory.setResourceValues(values)
        try data.write(to: fileURL, options: [.atomic, .completeFileProtection])
    }

    private func isValid(_ draft: MermaidActiveDraft) -> Bool {
        draft.schema == MermaidActiveDraft.currentSchema &&
            draft.savedAtMilliseconds > 0 &&
            draft.source.utf8.count <= MermaidSourceLoader.maximumBytes &&
            !draft.source.unicodeScalars.contains(where: { $0.value == 0 })
    }
}

private struct MermaidActiveDocumentReference: Codable, Equatable, Sendable {
    let documentIdentity: UUID
    let bookmarkData: Data
    let displayName: String
    let baselineSourceDigest: Data
    let baselineDiskDigest: Data
}

struct MermaidRestoredDocument: Equatable, Sendable {
    let document: MermaidOpenedDocument
    let documentIdentity: UUID
}

enum MermaidDocumentRestoration: Equatable, Sendable {
    case none
    case fileVersion(MermaidRestoredDocument)
    case recoveredEdits(MermaidRestoredDocument)
    case conflict(MermaidRestoredDocument)
    case unassociatedDraft(MermaidRestoredDocument)
}

enum MermaidDocumentAttention: Equatable {
    case changedOnDisk
    case recoveryConflict
    case unavailable
}

@MainActor
final class MermaidDocumentSession: ObservableObject {
    static let recentsStorageKey = "frankenmermaid.recentSourceDocuments.v1"
    static let activeDocumentStorageKey = "frankenmermaid.activeSourceDocument.v1"
    static let maximumRecentDocuments = 6

    @Published private(set) var currentDocument: MermaidOpenedDocument?
    @Published private(set) var recentDocuments: [MermaidRecentDocument]
    @Published private(set) var isSaving = false
    @Published private(set) var attention: MermaidDocumentAttention?
    @Published private(set) var currentDocumentIdentity: UUID?

    private var untitledBaseline: String
    private var activeDocumentReference: MermaidActiveDocumentReference?
    private var restorationDisplayName: String?
    private let defaults: UserDefaults

    init(initialSource: String, defaults: UserDefaults = .standard) {
        untitledBaseline = initialSource
        self.defaults = defaults
        recentDocuments = Self.loadRecents(from: defaults)
        activeDocumentReference = Self.loadActiveDocument(from: defaults)
        restorationDisplayName = activeDocumentReference?.displayName
    }

    var displayName: String {
        currentDocument?.displayName ?? restorationDisplayName ?? "Untitled Diagram"
    }
    var hasCurrentDocument: Bool { currentDocument != nil }

    func isDirty(source: String) -> Bool {
        source != (currentDocument?.source ?? untitledBaseline)
    }

    func beginUntitled(source: String) {
        currentDocument = nil
        currentDocumentIdentity = nil
        untitledBaseline = source
        attention = nil
        restorationDisplayName = nil
        activeDocumentReference = nil
        defaults.removeObject(forKey: Self.activeDocumentStorageKey)
    }

    func adopt(
        _ document: MermaidOpenedDocument,
        documentIdentity: UUID = UUID()
    ) {
        currentDocument = document
        currentDocumentIdentity = documentIdentity
        attention = nil
        restorationDisplayName = nil
        recordActive(document, documentIdentity: documentIdentity)
        recordRecent(document)
    }

    func adoptRecoveredEdits(
        from document: MermaidOpenedDocument,
        documentIdentity: UUID,
        changedOnDisk: Bool
    ) {
        currentDocument = document
        currentDocumentIdentity = documentIdentity
        attention = changedOnDisk ? .changedOnDisk : nil
        restorationDisplayName = nil
        if !changedOnDisk {
            recordActive(document, documentIdentity: documentIdentity)
        }
    }

    func adoptUnassociatedDraft(
        while retaining: MermaidOpenedDocument,
        documentIdentity: UUID
    ) {
        currentDocument = retaining
        currentDocumentIdentity = documentIdentity
        attention = .recoveryConflict
        restorationDisplayName = nil
    }

    func restoreActiveDocument(
        recoveredSource: String?,
        recoveredDocumentIdentity: UUID?
    ) async throws -> MermaidDocumentRestoration {
        guard let reference = activeDocumentReference else { return .none }
        do {
            let url = try MermaidSourceLoader.resolveBookmark(reference.bookmarkData)
            let document = try await MermaidSourceLoader.open(from: url)
            let restored = MermaidRestoredDocument(
                document: document,
                documentIdentity: reference.documentIdentity
            )
            guard let recoveredSource else { return .fileVersion(restored) }
            guard recoveredDocumentIdentity == reference.documentIdentity else {
                return .unassociatedDraft(restored)
            }

            let recoveredSourceDigest = Self.digest(Data(recoveredSource.utf8))
            if recoveredSourceDigest == reference.baselineSourceDigest {
                return .fileVersion(restored)
            }
            if Self.digest(document.diskData) == reference.baselineDiskDigest {
                return .recoveredEdits(restored)
            }
            return .conflict(restored)
        } catch {
            attention = .unavailable
            restorationDisplayName = reference.displayName
            throw error
        }
    }

    func openRecent(_ recent: MermaidRecentDocument) async throws -> MermaidOpenedDocument {
        let url = try MermaidSourceLoader.resolveBookmark(recent.bookmarkData)
        return try await MermaidSourceLoader.open(from: url)
    }

    func save(source: String) async throws {
        guard let currentDocument else { throw SourceDocumentError.noCurrentDocument }
        if attention == .changedOnDisk { throw SourceDocumentError.changedOnDisk }
        if attention == .recoveryConflict { throw SourceDocumentError.recoveryConflict }
        guard !isSaving else { return }
        isSaving = true
        defer { isSaving = false }
        do {
            let saved = try await MermaidSourceLoader.save(source, replacing: currentDocument)
            self.currentDocument = saved
            attention = nil
            recordActive(saved, documentIdentity: currentDocumentIdentity ?? UUID())
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

    private func recordActive(
        _ document: MermaidOpenedDocument,
        documentIdentity: UUID
    ) {
        let reference = MermaidActiveDocumentReference(
            documentIdentity: documentIdentity,
            bookmarkData: document.bookmarkData,
            displayName: document.displayName,
            baselineSourceDigest: Self.digest(Data(document.source.utf8)),
            baselineDiskDigest: Self.digest(document.diskData)
        )
        activeDocumentReference = reference
        if let encoded = try? JSONEncoder().encode(reference) {
            defaults.set(encoded, forKey: Self.activeDocumentStorageKey)
        }
    }

    private static func loadRecents(from defaults: UserDefaults) -> [MermaidRecentDocument] {
        guard let data = defaults.data(forKey: recentsStorageKey),
              let decoded = try? JSONDecoder().decode([MermaidRecentDocument].self, from: data) else {
            return []
        }
        return Array(decoded.prefix(maximumRecentDocuments))
    }

    private static func loadActiveDocument(
        from defaults: UserDefaults
    ) -> MermaidActiveDocumentReference? {
        guard let data = defaults.data(forKey: activeDocumentStorageKey) else { return nil }
        return try? JSONDecoder().decode(MermaidActiveDocumentReference.self, from: data)
    }

    private static func digest(_ data: Data) -> Data {
        Data(SHA256.hash(data: data))
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
    case recoveryConflict
    case coordinationFailed
    case savedCopyMismatch

    var errorDescription: String? {
        switch self {
        case .noCurrentDocument:
            "Choose Save to create a Mermaid file first."
        case .changedOnDisk:
            "That file changed outside FrankenMermaid. Your edits were not overwritten. " +
                "Save a copy, or reopen the file to use its newer contents."
        case .recoveryConflict:
            "This recovered draft could not be safely matched to the last file. " +
                "Use Save a Copy or reopen the file."
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
