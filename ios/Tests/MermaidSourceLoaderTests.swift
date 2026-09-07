import Foundation
import XCTest
@testable import FrankenMermaid

final class MermaidSourceLoaderTests: XCTestCase {
    func testDecodesExactUTF8Source() throws {
        let source = "flowchart LR\n  A --> B\n"
        XCTAssertEqual(try MermaidSourceLoader.decode(Data(source.utf8)), source)
    }

    func testStripsUTF8ByteOrderMark() throws {
        let source = "sequenceDiagram\n  A->>B: Hello\n"
        let data = Data([0xEF, 0xBB, 0xBF]) + Data(source.utf8)
        XCTAssertEqual(try MermaidSourceLoader.decode(data), source)
    }

    func testRejectsOversizeBinaryAndMalformedUTF8() {
        XCTAssertThrowsError(
            try MermaidSourceLoader.decode(
                Data(repeating: 0x61, count: MermaidSourceLoader.maximumBytes + 1)
            )
        )
        XCTAssertThrowsError(try MermaidSourceLoader.decode(Data([0xFF, 0xFE])))
        XCTAssertThrowsError(try MermaidSourceLoader.decode(Data([0x41, 0x00, 0x42])))
    }

    func testEncodingPreservesAnExplicitByteOrderMark() throws {
        let source = "flowchart LR\n  A --> B\n"
        let encoded = try MermaidSourceLoader.encode(source, includingByteOrderMark: true)

        XCTAssertTrue(encoded.starts(with: Data([0xEF, 0xBB, 0xBF])))
        XCTAssertEqual(try MermaidSourceLoader.decode(encoded), source)
    }

    func testOpenAndSaveRoundTripPreservesDocumentIdentityAndBOM() async throws {
        let original = "flowchart LR\n  A --> B\n"
        let updated = "flowchart LR\n  A --> B --> C\n"
        let url = try temporarySourceURL(
            contents: Data([0xEF, 0xBB, 0xBF]) + Data(original.utf8)
        )

        let opened = try await MermaidSourceLoader.open(from: url)
        let saved = try await MermaidSourceLoader.save(updated, replacing: opened)

        XCTAssertEqual(opened.url, url)
        XCTAssertEqual(opened.source, original)
        XCTAssertEqual(saved.url, url)
        XCTAssertEqual(saved.source, updated)
        XCTAssertTrue(saved.diskData.starts(with: Data([0xEF, 0xBB, 0xBF])))
        XCTAssertEqual(try MermaidSourceLoader.decode(Data(contentsOf: url)), updated)
    }

    func testSaveRefusesToOverwriteAnExternalChange() async throws {
        let original = "flowchart LR\n  A --> B\n"
        let external = "flowchart LR\n  External --> Edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let opened = try await MermaidSourceLoader.open(from: url)
        try Data(external.utf8).write(to: url, options: .atomic)

        do {
            _ = try await MermaidSourceLoader.save("flowchart LR\n  Local --> Edit\n", replacing: opened)
            XCTFail("Save must not overwrite a file whose bytes changed after it was opened")
        } catch {
            XCTAssertEqual(error as? SourceDocumentError, .changedOnDisk)
        }
        XCTAssertEqual(try MermaidSourceLoader.decode(Data(contentsOf: url)), external)
    }

    @MainActor
    func testDocumentSessionKeepsExternalConflictVisibleUntilReopen() async throws {
        let defaults = try XCTUnwrap(
            UserDefaults(suiteName: "MermaidSourceLoaderConflictTests.\(UUID().uuidString)")
        )
        let source = "flowchart LR\n  A --> B\n"
        let url = try temporarySourceURL(contents: Data(source.utf8))
        let session = MermaidDocumentSession(initialSource: source, defaults: defaults)
        session.adopt(try await MermaidSourceLoader.open(from: url))
        try Data("flowchart LR\n  External --> Edit\n".utf8).write(to: url, options: .atomic)

        do {
            try await session.save(source: "flowchart LR\n  Local --> Edit\n")
            XCTFail("Session save must surface the external-file conflict")
        } catch {
            XCTAssertEqual(error as? SourceDocumentError, .changedOnDisk)
        }
        XCTAssertEqual(session.attention, .changedOnDisk)

        session.adopt(try await MermaidSourceLoader.open(from: url))
        XCTAssertNil(session.attention)
    }

    @MainActor
    func testDocumentSessionTracksDirtyStateAndPersistsBoundedRecents() async throws {
        let suiteName = "MermaidSourceLoaderTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        let initial = "flowchart LR\n  Ready --> Sample\n"
        let session = MermaidDocumentSession(initialSource: initial, defaults: defaults)

        XCTAssertFalse(session.isDirty(source: initial))
        XCTAssertTrue(session.isDirty(source: initial + "  More\n"))

        var newestName = ""
        for index in 0..<(MermaidDocumentSession.maximumRecentDocuments + 2) {
            let name = "recent-\(index).mmd"
            newestName = name
            let url = try temporarySourceURL(contents: Data("flowchart LR\nA-->B\n".utf8), name: name)
            session.adopt(try await MermaidSourceLoader.open(from: url))
        }

        XCTAssertEqual(session.recentDocuments.count, MermaidDocumentSession.maximumRecentDocuments)
        XCTAssertTrue(session.recentDocuments.first?.displayName.hasSuffix(newestName) == true)
        let restored = MermaidDocumentSession(initialSource: initial, defaults: defaults)
        XCTAssertEqual(restored.recentDocuments, session.recentDocuments)
        let reopened = try await restored.openRecent(try XCTUnwrap(restored.recentDocuments.first))
        XCTAssertEqual(reopened.source, "flowchart LR\nA-->B\n")
    }

    func testProtectedDraftStoreRoundTripsDocumentIdentityAndSource() throws {
        let store = MermaidDraftStore(fileURL: temporaryDraftURL())
        let identity = UUID()
        let draft = makeDraft(
            source: "flowchart LR\n  Recovered --> Safely\n",
            documentIdentity: identity
        )

        try store.save(draft)

        XCTAssertEqual(store.load(), draft)
        XCTAssertEqual(store.load()?.documentIdentity, identity)
    }

    func testDraftStoreRejectsInvalidAndOversizedRecords() throws {
        let invalidStore = MermaidDraftStore(fileURL: temporaryDraftURL())
        let invalid = MermaidActiveDraft(
            schema: MermaidActiveDraft.currentSchema + 1,
            savedAtMilliseconds: 1,
            documentIdentity: nil,
            source: "flowchart LR\nA-->B\n"
        )
        XCTAssertThrowsError(try invalidStore.save(invalid)) { error in
            XCTAssertEqual(error as? MermaidDraftStore.StoreError, .invalidDraft)
        }

        let oversizedStore = MermaidDraftStore(fileURL: temporaryDraftURL())
        let oversized = makeDraft(
            source: String(repeating: "\"", count: MermaidSourceLoader.maximumBytes)
        )
        XCTAssertThrowsError(try oversizedStore.save(oversized)) { error in
            XCTAssertEqual(error as? MermaidDraftStore.StoreError, .oversizedDraft)
        }
    }

    func testDraftStoreIgnoresCorruptAndOversizedFiles() throws {
        let corruptURL = temporaryDraftURL()
        try Data("not json".utf8).write(to: corruptURL, options: .atomic)
        XCTAssertNil(MermaidDraftStore(fileURL: corruptURL).load())

        let oversizedURL = temporaryDraftURL()
        try Data(
            repeating: 0x61,
            count: MermaidDraftStore.maximumEncodedBytes + 1
        ).write(to: oversizedURL, options: .atomic)
        XCTAssertNil(MermaidDraftStore(fileURL: oversizedURL).load())
    }

    @MainActor
    func testSessionRestoresCurrentFileVersionWithoutRecoveredEdits() async throws {
        let defaults = try makeDefaults()
        let original = "flowchart LR\n  Original --> File\n"
        let external = "flowchart LR\n  Current --> File\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let firstSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        firstSession.adopt(try await MermaidSourceLoader.open(from: url))
        try Data(external.utf8).write(to: url, options: .atomic)

        let restoredSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: nil,
            recoveredDocumentIdentity: nil
        )

        guard case .fileVersion(let restored) = restoration else {
            return XCTFail("A launch without recovered edits must use the current file version")
        }
        XCTAssertEqual(restored.document.source, external)
    }

    @MainActor
    func testSessionReconnectsRecoveredEditsWhenFileIsUnchanged() async throws {
        let defaults = try makeDefaults()
        let original = "flowchart LR\n  Original --> File\n"
        let recovered = "flowchart LR\n  Recovered --> Edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let firstSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        firstSession.adopt(try await MermaidSourceLoader.open(from: url))
        let identity = try XCTUnwrap(firstSession.currentDocumentIdentity)

        let restoredSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: recovered,
            recoveredDocumentIdentity: identity
        )
        guard case .recoveredEdits(let restored) = restoration else {
            return XCTFail("Recovered edits should reconnect to an unchanged file")
        }
        restoredSession.adoptRecoveredEdits(
            from: restored.document,
            documentIdentity: restored.documentIdentity,
            changedOnDisk: false
        )

        XCTAssertTrue(restoredSession.hasCurrentDocument)
        XCTAssertTrue(restoredSession.isDirty(source: recovered))
        XCTAssertNil(restoredSession.attention)
    }

    @MainActor
    func testSessionRefusesInPlaceSaveAfterTwoSidedRestorationConflict() async throws {
        let defaults = try makeDefaults()
        let original = "flowchart LR\n  Original --> File\n"
        let external = "flowchart LR\n  External --> Edit\n"
        let recovered = "flowchart LR\n  Recovered --> Edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let firstSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        firstSession.adopt(try await MermaidSourceLoader.open(from: url))
        let identity = try XCTUnwrap(firstSession.currentDocumentIdentity)
        try Data(external.utf8).write(to: url, options: .atomic)

        let restoredSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: recovered,
            recoveredDocumentIdentity: identity
        )
        guard case .conflict(let restored) = restoration else {
            return XCTFail("Two changed versions must produce an explicit restoration conflict")
        }
        restoredSession.adoptRecoveredEdits(
            from: restored.document,
            documentIdentity: restored.documentIdentity,
            changedOnDisk: true
        )

        do {
            try await restoredSession.save(source: recovered)
            XCTFail("In-place Save must stay blocked while the restoration conflict is unresolved")
        } catch {
            XCTAssertEqual(error as? SourceDocumentError, .changedOnDisk)
        }
        XCTAssertEqual(try MermaidSourceLoader.decode(Data(contentsOf: url)), external)
    }

    @MainActor
    func testSessionDoesNotAttachDraftFromAnotherDocument() async throws {
        let defaults = try makeDefaults()
        let url = try temporarySourceURL(contents: Data("flowchart LR\n  Current --> File\n".utf8))
        let firstSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        firstSession.adopt(try await MermaidSourceLoader.open(from: url))

        let restoredSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: "flowchart LR\n  Other --> Draft\n",
            recoveredDocumentIdentity: UUID()
        )

        guard case .unassociatedDraft(let restored) = restoration else {
            return XCTFail("A draft with another identity must never attach to the active file")
        }
        restoredSession.adoptUnassociatedDraft(
            while: restored.document,
            documentIdentity: restored.documentIdentity
        )
        XCTAssertEqual(restoredSession.attention, .recoveryConflict)
        do {
            try await restoredSession.save(source: "flowchart LR\n  Other --> Draft\n")
            XCTFail("In-place Save must remain blocked for a mismatched draft")
        } catch {
            XCTAssertEqual(error as? SourceDocumentError, .recoveryConflict)
        }
    }

    @MainActor
    func testBeginningUntitledClearsAutomaticDocumentRestoration() async throws {
        let defaults = try makeDefaults()
        let url = try temporarySourceURL(contents: Data("flowchart LR\n  Stored --> File\n".utf8))
        let firstSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        firstSession.adopt(try await MermaidSourceLoader.open(from: url))
        firstSession.beginUntitled(source: MermaidRendererModel.sample)

        let restoredSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: MermaidRendererModel.sample,
            recoveredDocumentIdentity: nil
        )
        XCTAssertEqual(restoration, .none)
    }

    @MainActor
    func testUnavailableActiveFileKeepsItsNameAndAttentionState() async throws {
        let defaults = try makeDefaults()
        let url = try temporarySourceURL(
            contents: Data("flowchart LR\n  Stored --> File\n".utf8),
            name: "remember-me.mmd"
        )
        let firstSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        firstSession.adopt(try await MermaidSourceLoader.open(from: url))
        let stored = try XCTUnwrap(defaults.data(forKey: MermaidDocumentSession.activeDocumentStorageKey))
        var reference = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: stored) as? [String: Any]
        )
        reference["bookmarkData"] = Data([0x00, 0x01, 0x02]).base64EncodedString()
        defaults.set(try JSONSerialization.data(withJSONObject: reference),
                     forKey: MermaidDocumentSession.activeDocumentStorageKey)

        let restoredSession = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        do {
            _ = try await restoredSession.restoreActiveDocument(
                recoveredSource: "flowchart LR\n  Recovered --> Draft\n",
                recoveredDocumentIdentity: firstSession.currentDocumentIdentity
            )
            XCTFail("An unresolvable active bookmark should not be reported as restored")
        } catch {
            XCTAssertEqual(restoredSession.attention, .unavailable)
            XCTAssertTrue(restoredSession.displayName.hasSuffix("remember-me.mmd"))
        }
    }

    @MainActor
    func testActiveReferenceDoesNotStoreSourceOrClearPath() async throws {
        let defaults = try makeDefaults()
        let source = "flowchart LR\n  PrivateSourceMarker --> File\n"
        let url = try temporarySourceURL(contents: Data(source.utf8), name: "private-path-marker.mmd")
        let session = MermaidDocumentSession(initialSource: MermaidRendererModel.sample, defaults: defaults)
        session.adopt(try await MermaidSourceLoader.open(from: url))

        let stored = try XCTUnwrap(defaults.data(forKey: MermaidDocumentSession.activeDocumentStorageKey))
        let json = try XCTUnwrap(String(data: stored, encoding: .utf8))
        XCTAssertFalse(json.contains("PrivateSourceMarker"))
        XCTAssertFalse(json.contains(url.path))
    }

    private func makeDefaults() throws -> UserDefaults {
        try XCTUnwrap(UserDefaults(suiteName: "MermaidRestorationTests.\(UUID().uuidString)"))
    }

    private func makeDraft(
        source: String,
        documentIdentity: UUID? = nil
    ) -> MermaidActiveDraft {
        MermaidActiveDraft(
            schema: MermaidActiveDraft.currentSchema,
            savedAtMilliseconds: 1_725_350_400_000,
            documentIdentity: documentIdentity,
            source: source
        )
    }

    private func temporaryDraftURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("frankenmermaid-draft-tests-\(UUID().uuidString).json")
    }

    private func temporarySourceURL(contents: Data, name: String = "diagram.mmd") throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("frankenmermaid-tests-\(UUID().uuidString)-\(name)")
        try contents.write(to: url, options: .atomic)
        return url
    }
}
