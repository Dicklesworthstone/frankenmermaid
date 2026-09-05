import XCTest
@testable import FrankenMermaid

final class MermaidLensBindingTests: XCTestCase {
    func testBindingPreservesEngineIdentityAndSourceLocation() throws {
        let binding = try XCTUnwrap(MermaidLensBinding(payload: [
            "elementId": "fm-node-a-0",
            "kind": "Node",
            "sourceId": "A",
            "snippet": "A[Alpha]",
            "textRange": ["startByte": 13, "endByte": 21],
            "span": ["start": ["line": 2, "col": 1, "byte": 13]]
        ]))

        XCTAssertEqual(binding.id, "fm-node-a-0")
        XCTAssertEqual(binding.kind, "node")
        XCTAssertEqual(binding.sourceID, "A")
        XCTAssertEqual(binding.snippet, "A[Alpha]")
        XCTAssertEqual(binding.startByte, 13)
        XCTAssertEqual(binding.endByte, 21)
        XCTAssertEqual(binding.line, 2)
        XCTAssertEqual(binding.column, 1)
    }

    func testBindingRejectsMissingElementIdentity() {
        XCTAssertNil(MermaidLensBinding(payload: ["kind": "node"]))
        XCTAssertNil(MermaidLensBinding(payload: ["elementId": ""]))
    }

    func testExactSourceSnippetUsesUTF8ByteOffsets() throws {
        let source = "flowchart LR\nA[Crème] --> B[Tea]\n"
        let snippet = "A[Crème] --> B[Tea]"
        let start = try XCTUnwrap(source.utf8.firstRange(of: snippet.utf8)?.lowerBound)
        let startByte = source.utf8.distance(from: source.utf8.startIndex, to: start)
        let binding = try XCTUnwrap(MermaidLensBinding(payload: [
            "elementId": "fm-edge-0",
            "kind": "edge",
            "snippet": snippet,
            "textRange": ["startByte": startByte, "endByte": startByte + snippet.utf8.count]
        ]))

        XCTAssertEqual(binding.exactSourceSnippet(in: source), snippet)
    }

    func testExactSourceSnippetRejectsStaleOrNonUTF8Range() throws {
        let binding = try XCTUnwrap(MermaidLensBinding(payload: [
            "elementId": "fm-node-a-0",
            "kind": "node",
            "snippet": "Crème",
            "textRange": ["startByte": 1, "endByte": 6]
        ]))

        XCTAssertNil(binding.exactSourceSnippet(in: "XCrème"), "range ends inside a UTF-8 scalar")
        XCTAssertNil(binding.exactSourceSnippet(in: "XOther"), "stale source must not reuse a prior snippet")
    }
}

final class MermaidSourceHistoryTests: XCTestCase {
    func testUndoAndRedoRoundTripEverySourceMutationPath() throws {
        let history = MermaidSourceHistory()
        history.recordChange(from: "first", to: "second", continuous: false)

        XCTAssertTrue(history.canUndo)
        XCTAssertFalse(history.canRedo)
        let restored = try XCTUnwrap(history.undo(currentSource: "second"))
        XCTAssertEqual(restored, "first")
        XCTAssertFalse(history.canUndo)
        XCTAssertTrue(history.canRedo)

        history.recordChange(from: "second", to: restored, continuous: false)
        let replayed = try XCTUnwrap(history.redo(currentSource: restored))
        XCTAssertEqual(replayed, "second")
        history.recordChange(from: restored, to: replayed, continuous: false)
        XCTAssertTrue(history.canUndo)
        XCTAssertFalse(history.canRedo)
    }

    func testContinuousTypingCoalescesIntoOneUndoStep() throws {
        let history = MermaidSourceHistory()
        let start = Date(timeIntervalSince1970: 1_000)
        history.recordChange(from: "A", to: "AB", continuous: true, now: start)
        history.recordChange(
            from: "AB",
            to: "ABC",
            continuous: true,
            now: start.addingTimeInterval(0.4)
        )

        XCTAssertEqual(try XCTUnwrap(history.undo(currentSource: "ABC")), "A")
        XCTAssertFalse(history.canUndo)
    }

    func testFreshChangeAfterUndoClearsRedo() throws {
        let history = MermaidSourceHistory()
        history.recordChange(from: "A", to: "B", continuous: false)
        let restored = try XCTUnwrap(history.undo(currentSource: "B"))
        history.recordChange(from: "B", to: restored, continuous: false)
        XCTAssertTrue(history.canRedo)

        history.recordChange(from: restored, to: "C", continuous: false)
        XCTAssertFalse(history.canRedo)
        XCTAssertEqual(history.undo(currentSource: "C"), restored)
    }
}
