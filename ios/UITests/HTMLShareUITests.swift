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
