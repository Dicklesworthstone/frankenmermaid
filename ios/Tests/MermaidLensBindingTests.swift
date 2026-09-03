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
}
