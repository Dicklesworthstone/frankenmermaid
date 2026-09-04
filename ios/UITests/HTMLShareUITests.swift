import XCTest

final class HTMLShareUITests: XCTestCase {
    func testAnimatedWebPageIsSharedAsAFile() throws {
        let app = XCUIApplication()
        app.launch()

        // Follow the same explicit render path a compact-width user takes. Tapping the
        // segmented control alone can race SwiftUI's initial renderer publication and
        // leave the Code lane selected; View Diagram owns both operations atomically.
        let diagram = app.buttons["View Diagram"]
        XCTAssertTrue(diagram.waitForExistence(timeout: 8))
        XCTAssertTrue(diagram.isHittable)
        diagram.tap()

        let share = app.buttons["Share"]
        XCTAssertTrue(share.waitForExistence(timeout: 12))
        let shareIsReady = expectation(
            for: NSPredicate(format: "isEnabled == YES AND isHittable == YES"),
            evaluatedWith: share
        )
        wait(for: [shareIsReady], timeout: 12)
        share.tap()

        let animatedPage = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "animated web page")
        ).firstMatch
        XCTAssertTrue(animatedPage.waitForExistence(timeout: 3))
        animatedPage.tap()

        let activitySheet = app.otherElements["ActivityListView"]
        XCTAssertTrue(
            activitySheet.waitForExistence(timeout: 12),
            "Expected the system activity sheet.\n\(app.debugDescription)"
        )
        let saveToFiles = app.cells.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "save to files")
        ).firstMatch
        XCTAssertTrue(
            saveToFiles.waitForExistence(timeout: 12),
            "Expected an activity sheet with a file destination.\n\(app.debugDescription)"
        )
        // Save-to-Files is offered only because the share payload is a file
        // representation. Opening the system Files extension is unreliable in
        // headless Simulator runs, so close the proven activity sheet here.
        app.buttons["Close"].tap()
    }
}

final class FrankenMermaidStorefrontUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
        Thread.sleep(forTimeInterval: 0.25)
    }

    func testAppStoreSourceStudioIsAppBound() {
        let app = launch(lane: "Code")
        assertExists(
            app.descendants(matching: .any)["source-editor-panel"],
            in: app,
            message: "The native Mermaid source studio did not render",
            screenshotName: "App Store 1 - private Mermaid source studio"
        )
        XCTAssertTrue(
            app.buttons["View Diagram"].exists ||
                app.descendants(matching: .any)["live-diagram-stage"].exists,
            "The source studio did not expose a route or adjacent view for the rendered diagram"
        )
        assertNoForeignAppIdentity(in: app)
    }

    func testAppStoreLiveDiagramAndExportFormatsRender() {
        let app = launch(lane: "Diagram")
        XCTAssertTrue(
            app.descendants(matching: .any)["live-diagram-stage"].waitForExistence(timeout: 12),
            "The bundled Rust and WebAssembly diagram stage did not render"
        )

        let share = app.buttons["Share"]
        XCTAssertTrue(share.waitForExistence(timeout: 12))
        let ready = expectation(
            for: NSPredicate(format: "isEnabled == YES AND isHittable == YES"),
            evaluatedWith: share
        )
        wait(for: [ready], timeout: 12)
        Thread.sleep(forTimeInterval: 0.65)
        keepScreenshot(of: app, named: "App Store 2 - live private diagram preview")
        share.tap()

        XCTAssertTrue(app.buttons["Raster PNG (2x)"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["PDF Document"].exists)
        XCTAssertTrue(app.buttons["Vector SVG"].exists)
        keepScreenshot(of: app, named: "App Store 3 - PNG PDF SVG and HTML export")
        assertNoForeignAppIdentity(in: app)
    }

    func testAppStoreSourceLensSelectsAndAppliesAnExactEdit() {
        let app = launch(lane: "Diagram")
        XCTAssertTrue(
            app.descendants(matching: .any)["live-diagram-stage"].waitForExistence(timeout: 12),
            "The bundled diagram stage did not render"
        )

        let sourceNode = app.webViews.buttons.matching(
            NSPredicate(format: "label == %@", "Diagram node Source")
        ).firstMatch
        XCTAssertTrue(
            sourceNode.waitForExistence(timeout: 12),
            "The Rust source lens did not expose the rendered Source node as an accessible control.\n" +
                app.debugDescription
        )
        XCTAssertTrue(sourceNode.isHittable)
        sourceNode.tap()

        XCTAssertTrue(app.navigationBars["Source Lens"].waitForExistence(timeout: 5))
        let replacement = app.textViews.matching(
            NSPredicate(format: "label BEGINSWITH %@", "Source replacement for")
        ).firstMatch
        XCTAssertTrue(
            replacement.waitForExistence(timeout: 5),
            "Selecting a source-bound diagram node did not reveal its exact native editor"
        )
        replacement.tap()
        replacement.typeText(" ")

        let apply = app.buttons["Apply exact edit"]
        XCTAssertTrue(apply.waitForExistence(timeout: 3))
        XCTAssertTrue(apply.isEnabled)
        keepScreenshot(of: app, named: "App Store 4 - direct source lens edit")
        apply.tap()

        XCTAssertTrue(
            app.navigationBars["Source Lens"].waitForNonExistence(timeout: 8),
            "The native lens sheet remained after the Rust engine accepted the exact edit"
        )
        XCTAssertTrue(app.staticTexts["graph heart ready"].waitForExistence(timeout: 12))
        app.buttons["Code"].tap()
        let sourceEditor = app.textViews["Mermaid source editor"]
        XCTAssertTrue(sourceEditor.waitForExistence(timeout: 5))
        XCTAssertTrue(
            String(describing: sourceEditor.value).contains("Source[Mermaid source] "),
            "The engine-owned edit receipt did not update the native source editor"
        )
        keepScreenshot(of: app, named: "App Store 5 - source updated by Rust lens")
        assertNoForeignAppIdentity(in: app)
    }

    func testAppStoreTwentyFourFamilyGalleryRenders() {
        let app = XCUIApplication()
        app.launchEnvironment["FM_SHOW_SAMPLES"] = "1"
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 8))
        assertExists(
            app.descendants(matching: .any)["sample-gallery"],
            in: app,
            message: "The 24-family sample gallery did not render",
            screenshotName: "App Store 6 - 24 diagram family gallery"
        )
        XCTAssertTrue(app.navigationBars["Diagram Specimens"].exists)
        assertNoForeignAppIdentity(in: app)
    }

    @discardableResult
    private func launch(lane: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["FM_INITIAL_LANE"] = lane
        app.launch()
        XCTAssertTrue(
            app.wait(for: .runningForeground, timeout: 8),
            "FrankenMermaid did not remain in the foreground"
        )
        XCTAssertTrue(
            app.descendants(matching: .any)["frankenmermaid-brand"].waitForExistence(timeout: 12),
            "FrankenMermaid did not expose its app-bound identity"
        )
        return app
    }

    private func assertExists(
        _ element: XCUIElement,
        in app: XCUIApplication,
        message: String,
        screenshotName: String
    ) {
        let exists = element.waitForExistence(timeout: 12)
        if exists { Thread.sleep(forTimeInterval: 0.65) }
        keepScreenshot(of: app, named: screenshotName)
        XCTAssertTrue(exists, message)
    }

    private func keepScreenshot(of app: XCUIApplication, named name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func assertNoForeignAppIdentity(in app: XCUIApplication) {
        for foreignName in ["FrankenCA", "FrankenPatents", "FrankenRobots", "FrankenTTS"] {
            XCTAssertFalse(
                app.staticTexts[foreignName].exists,
                "App-bound FrankenMermaid evidence unexpectedly contained \(foreignName)"
            )
        }
    }
}
