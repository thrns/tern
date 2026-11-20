import AppKit
import TernFFI
import XCTest

@testable import TernApp

private final class StubCore: LauncherCore {
    func calc(input: String) -> CalcOutput? {
        nil
    }
}

@MainActor
final class LauncherLayoutTests: XCTestCase {
    private func loadedController() -> LauncherViewController {
        let controller = LauncherViewController(core: StubCore())
        controller.loadView()
        controller.view.layoutSubtreeIfNeeded()
        return controller
    }

    func testInputFieldLiesInsideContentView() {
        let controller = loadedController()
        let content = controller.view.bounds
        let input = controller.inputField.frame
        XCTAssertGreaterThan(input.width, 0)
        XCTAssertGreaterThan(input.height, 0)
        XCTAssertTrue(content.contains(input), "input \(input) is not inside content \(content)")
    }

    func testResultsViewIsEmptyAndCollapsedOnLoad() {
        let controller = loadedController()
        XCTAssertEqual(controller.resultsView.frame.height, 0)
        XCTAssertEqual(controller.resultsView.rowCount, 0)
        XCTAssertEqual(controller.resultsView.contentHeight, 0)
    }

    func testContentViewMatchesMinimumPanelSize() {
        let controller = loadedController()
        XCTAssertEqual(controller.view.bounds.width, LauncherPanel.width)
        XCTAssertEqual(controller.view.bounds.height, LauncherPanel.minimumHeight)
    }
}
