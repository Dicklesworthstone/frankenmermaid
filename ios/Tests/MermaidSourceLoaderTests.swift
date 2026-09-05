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

    private func temporarySourceURL(contents: Data, name: String = "diagram.mmd") throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("frankenmermaid-tests-\(UUID().uuidString)-\(name)")
        try contents.write(to: url, options: .atomic)
        return url
    }
}
