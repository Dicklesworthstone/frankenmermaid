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

        let saveToFiles = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "save to files")
        ).firstMatch
        XCTAssertTrue(
            saveToFiles.waitForExistence(timeout: 12),
            "Expected an activity sheet with a file destination.\n\(app.debugDescription)"
        )
        let htmlFilename = app.staticTexts.matching(
            NSPredicate(format: "label ENDSWITH[c] %@", ".html")
        ).firstMatch
        XCTAssertTrue(
            htmlFilename.waitForExistence(timeout: 3),
            "The activity sheet did not identify an HTML file.\n\(app.debugDescription)"
        )
    }
}
