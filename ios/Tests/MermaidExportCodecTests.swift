import XCTest
@testable import FrankenMermaid

final class MermaidExportCodecTests: XCTestCase {
    func testExportKindsExposePromisedNativeFormats() {
        XCTAssertEqual(
            MermaidExportKind.allCases.map(\.fileExtension),
            ["mmd", "svg", "png", "pdf", "html", "html"]
        )
        XCTAssertEqual(MermaidExportKind.deckHTML.title, "Graph Deck Presentation")
    }

    func testPNGDataURLRequiresDecodablePNG() throws {
        let png = try XCTUnwrap(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
        let encoded = "data:image/png;base64," + png.base64EncodedString()
        XCTAssertEqual(try MermaidExportCodec.decodePNGDataURL(encoded), png)
        XCTAssertThrowsError(try MermaidExportCodec.decodePNGDataURL("data:text/plain;base64,SGk="))
        XCTAssertThrowsError(try MermaidExportCodec.decodePNGDataURL("data:image/png;base64,SGk="))
        let truncated = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01])
        XCTAssertThrowsError(try MermaidExportCodec.decodePNGDataURL(
            "data:image/png;base64," + truncated.base64EncodedString()
        ))
    }

    func testExportBudgetRejectsEmptyAndOversizedArtifacts() {
        XCTAssertThrowsError(try MermaidExportCodec.validateSize(Data()))
        XCTAssertThrowsError(
            try MermaidExportCodec.validateSize(Data(count: MermaidExportCodec.maximumBytes + 1))
        )
    }
}
