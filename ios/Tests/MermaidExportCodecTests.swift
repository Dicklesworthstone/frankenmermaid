import XCTest
@testable import FrankenMermaid

final class MermaidExportCodecTests: XCTestCase {
    func testExportKindsExposePromisedNativeFormats() {
        XCTAssertEqual(
            MermaidExportKind.allCases.map(\.fileExtension),
            ["mmd", "svg", "png", "pdf", "html"]
        )
    }

    func testPNGDataURLRequiresSignature() throws {
        let signature = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01])
        let encoded = "data:image/png;base64," + signature.base64EncodedString()
        XCTAssertEqual(try MermaidExportCodec.decodePNGDataURL(encoded), signature)
        XCTAssertThrowsError(try MermaidExportCodec.decodePNGDataURL("data:text/plain;base64,SGk="))
        XCTAssertThrowsError(try MermaidExportCodec.decodePNGDataURL("data:image/png;base64,SGk="))
    }

    func testExportBudgetRejectsEmptyAndOversizedArtifacts() {
        XCTAssertThrowsError(try MermaidExportCodec.validateSize(Data()))
        XCTAssertThrowsError(
            try MermaidExportCodec.validateSize(Data(count: MermaidExportCodec.maximumBytes + 1))
        )
    }
}
