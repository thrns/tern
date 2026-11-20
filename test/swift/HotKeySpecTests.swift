import Carbon.HIToolbox
import TernKit
import XCTest

final class HotKeySpecTests: XCTestCase {
    func testParsesDefaultPanelHotkey() {
        let spec = HotKeySpec.parse("alt+space")
        XCTAssertEqual(spec?.keyCode, UInt32(kVK_Space))
        XCTAssertEqual(spec?.carbonModifiers, UInt32(optionKey))
    }

    func testParsesDefaultCaptureHotkey() {
        let spec = HotKeySpec.parse("alt+shift+space")
        XCTAssertEqual(spec?.keyCode, UInt32(kVK_Space))
        XCTAssertEqual(spec?.carbonModifiers, UInt32(optionKey | shiftKey))
    }

    func testOptionIsAliasForAlt() {
        XCTAssertNotNil(HotKeySpec.parse("option+space"))
        XCTAssertEqual(HotKeySpec.parse("option+space"), HotKeySpec.parse("alt+space"))
    }

    func testAllModifiersCombine() {
        let spec = HotKeySpec.parse("cmd+alt+ctrl+shift+a")
        XCTAssertEqual(spec?.keyCode, UInt32(kVK_ANSI_A))
        XCTAssertEqual(spec?.carbonModifiers, UInt32(cmdKey | optionKey | controlKey | shiftKey))
    }

    func testModifierOrderDoesNotChangeResult() {
        XCTAssertEqual(HotKeySpec.parse("shift+alt+space"), HotKeySpec.parse("alt+shift+space"))
    }

    func testParsesNamedKeys() {
        XCTAssertEqual(HotKeySpec.parse("cmd+return")?.keyCode, UInt32(kVK_Return))
        XCTAssertEqual(HotKeySpec.parse("ctrl+tab")?.keyCode, UInt32(kVK_Tab))
    }

    func testParsesDigits() {
        XCTAssertEqual(HotKeySpec.parse("cmd+0")?.keyCode, UInt32(kVK_ANSI_0))
        XCTAssertEqual(HotKeySpec.parse("cmd+9")?.keyCode, UInt32(kVK_ANSI_9))
    }

    func testParsesFunctionKeysOneThroughTwelve() {
        XCTAssertEqual(HotKeySpec.parse("cmd+f1")?.keyCode, UInt32(kVK_F1))
        XCTAssertEqual(HotKeySpec.parse("cmd+f12")?.keyCode, UInt32(kVK_F12))
    }

    func testRejectsFunctionKeysOutOfRange() {
        XCTAssertNil(HotKeySpec.parse("cmd+f0"))
        XCTAssertNil(HotKeySpec.parse("cmd+f13"))
        XCTAssertNil(HotKeySpec.parse("cmd+f01"))
    }

    func testAllLettersMapToDistinctKeyCodes() {
        let letters = "abcdefghijklmnopqrstuvwxyz".map(String.init)
        let codes = letters.compactMap { HotKeySpec.parse("cmd+\($0)")?.keyCode }
        XCTAssertEqual(codes.count, 26)
        XCTAssertEqual(Set(codes).count, 26)
    }

    func testRejectsMissingModifier() {
        XCTAssertNil(HotKeySpec.parse("space"))
        XCTAssertNil(HotKeySpec.parse("a"))
        XCTAssertNil(HotKeySpec.parse("f5"))
    }

    func testRejectsUnknownKey() {
        XCTAssertNil(HotKeySpec.parse("alt+escape"))
        XCTAssertNil(HotKeySpec.parse("alt+!"))
        XCTAssertNil(HotKeySpec.parse("alt+ab"))
    }

    func testRejectsUppercase() {
        XCTAssertNil(HotKeySpec.parse("Alt+space"))
        XCTAssertNil(HotKeySpec.parse("alt+Space"))
        XCTAssertNil(HotKeySpec.parse("alt+A"))
        XCTAssertNil(HotKeySpec.parse("ALT+SPACE"))
    }

    func testRejectsDuplicateModifiers() {
        XCTAssertNil(HotKeySpec.parse("alt+alt+space"))
        XCTAssertNil(HotKeySpec.parse("alt+option+space"))
    }

    func testRejectsUnknownModifier() {
        XCTAssertNil(HotKeySpec.parse("super+space"))
        XCTAssertNil(HotKeySpec.parse("alt+space+a"))
    }

    func testRejectsMalformedInput() {
        XCTAssertNil(HotKeySpec.parse(""))
        XCTAssertNil(HotKeySpec.parse("+"))
        XCTAssertNil(HotKeySpec.parse("alt+"))
        XCTAssertNil(HotKeySpec.parse("+space"))
        XCTAssertNil(HotKeySpec.parse("alt++space"))
        XCTAssertNil(HotKeySpec.parse(" alt+space"))
        XCTAssertNil(HotKeySpec.parse("alt + space"))
    }
}
