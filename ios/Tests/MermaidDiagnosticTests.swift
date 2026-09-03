import XCTest
@testable import FrankenMermaid

final class MermaidDiagnosticTests: XCTestCase {
    func testBridgeDiagnosticPreservesEngineFinding() throws {
        let diagnostic = try XCTUnwrap(MermaidDiagnostic(payload: [
            "severity": "Warning",
            "category": "Recovery",
            "message": "Recovered an incomplete edge",
            "suggestion": "Add a target node",
            "line": 4,
            "column": 9
        ], index: 2))

        XCTAssertEqual(diagnostic.severity, "warning")
        XCTAssertEqual(diagnostic.category, "recovery")
        XCTAssertEqual(diagnostic.message, "Recovered an incomplete edge")
        XCTAssertEqual(diagnostic.suggestion, "Add a target node")
        XCTAssertEqual(diagnostic.line, 4)
        XCTAssertEqual(diagnostic.column, 9)
    }

    func testBridgeDiagnosticNormalizesUnknownSeverityAndAbsentLocation() throws {
        let diagnostic = try XCTUnwrap(MermaidDiagnostic(payload: [
            "severity": "future-level",
            "message": "A future engine finding",
            "line": 0,
            "column": 0
        ], index: 0))

        XCTAssertEqual(diagnostic.severity, "info")
        XCTAssertNil(diagnostic.line)
        XCTAssertNil(diagnostic.column)
        XCTAssertNil(diagnostic.suggestion)
    }

    func testBridgeDiagnosticRejectsMissingMessage() {
        XCTAssertNil(MermaidDiagnostic(payload: ["severity": "warning"], index: 0))
    }
}
