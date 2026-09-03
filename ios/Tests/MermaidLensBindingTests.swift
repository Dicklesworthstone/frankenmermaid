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
