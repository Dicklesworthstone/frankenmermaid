import XCTest

final class HTMLShareUITests: XCTestCase {
    func testAnimatedWebPageIsSharedAsAFile() throws {
        let app = XCUIApplication()
        app.launch()

        let diagram = app.buttons.matching(
            NSPredicate(format: "label ==[c] %@", "diagram")
        ).firstMatch
        XCTAssertTrue(diagram.waitForExistence(timeout: 8))
        diagram.tap()

        let share = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "share")
        ).firstMatch
        XCTAssertTrue(share.waitForExistence(timeout: 12))
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
