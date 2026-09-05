import Foundation
import XCTest
@testable import FrankenMermaid

final class MermaidDeckBridgeContractTests: XCTestCase {
    func testDeckSummaryAcceptsOnlyInternallyConsistentEngineMetadata() throws {
        let summary = try XCTUnwrap(MermaidDeckSummary(payload: [
            "title": "Pipeline Tour",
            "slideCount": 3,
            "sceneCount": 4,
            "overviewEnabled": true
        ]))
        XCTAssertEqual(summary.title, "Pipeline Tour")
        XCTAssertEqual(summary.slideCount, 3)
        XCTAssertEqual(summary.sceneCount, 4)
        XCTAssertTrue(summary.overviewEnabled)

        XCTAssertNil(MermaidDeckSummary(payload: [
            "title": "Contradictory",
            "slideCount": 3,
            "sceneCount": 3,
            "overviewEnabled": true
        ]))
        XCTAssertNil(MermaidDeckSummary(payload: [
            "title": "Empty",
            "slideCount": 0,
            "sceneCount": 0,
            "overviewEnabled": false
        ]))
    }

    func testDeckSceneRejectsNonPresentedOrIncompleteReceipts() throws {
        let scene = try XCTUnwrap(MermaidDeckSceneState(payload: [
            "presented": true,
            "title": "Start with source",
            "caption": "Private Mermaid text enters the parser.",
            "position": "01 / 04 · 0/1"
        ]))
        XCTAssertEqual(scene.title, "Start with source")
        XCTAssertEqual(scene.position, "01 / 04 · 0/1")
        XCTAssertNil(MermaidDeckSceneState(payload: [
            "presented": false,
            "title": "Start with source",
            "caption": "",
            "position": "01 / 04"
        ]))
        XCTAssertNil(MermaidDeckSceneState(payload: [
            "presented": true,
            "title": "Start with source",
            "caption": ""
        ]))
    }

    func testNativeBridgeUsesEngineDeckOutputAndCanonicalRuntime() throws {
        let bridge = try sourceFile("Renderer/bridge.html")
        XCTAssertTrue(bridge.contains("parse, renderDeck } from './frankenmermaid.js'"))
        XCTAssertTrue(bridge.contains("const deckOutput = renderDeck("))
        XCTAssertTrue(bridge.contains("window.FmDeckRuntime.mount({"))
        XCTAssertTrue(bridge.contains("window.frankenDeck = (command) =>"))
        XCTAssertTrue(bridge.contains("deckController.next()"))
        XCTAssertTrue(bridge.contains("deckController.prev()"))
        XCTAssertTrue(bridge.contains("deckController.overview()"))
        XCTAssertTrue(bridge.contains("if (command.kind === 'deckHTML')"))
    }

    func testCanonicalDeckTemplateIsHashGatedAndStaged() throws {
        let staging = try sourceFile("stage-renderer.sh")
        let manifest = try sourceFile("Renderer/RendererManifest.json")
        XCTAssertTrue(staging.contains("crates/fm-cli/src/deck_template.html"))
        XCTAssertTrue(staging.contains("expected_deck_template"))
        XCTAssertTrue(staging.contains("actual_deck_template"))
        XCTAssertTrue(manifest.contains("\"deckTemplateSha256\""))
    }

    func testGalleryIncludesAUsableGraphDeckDirective() throws {
        let sample = try XCTUnwrap(DiagramSample.all.first { $0.id == "graph-deck" })
        XCTAssertTrue(sample.source.contains("%%{deck:"))
        XCTAssertTrue(sample.source.contains("slides:"))
        XCTAssertTrue(sample.source.contains("reveal:"))
    }

    private func sourceFile(_ relativePath: String) throws -> String {
        let iosRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return try String(
            contentsOf: iosRoot.appendingPathComponent(relativePath),
            encoding: .utf8
        )
    }
}
