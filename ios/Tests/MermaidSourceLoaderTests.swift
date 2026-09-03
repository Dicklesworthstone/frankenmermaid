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
}
