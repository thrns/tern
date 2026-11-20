import AppKit
import Foundation
import TernFFI

@MainActor
enum BenchMode {
    private static let idleDelay: Duration = .seconds(10)
    private static let hiddenSettleDelay: Duration = .seconds(5)
    private static let fillText = "bench"

    static var isEnabled: Bool {
        ProcessInfo.processInfo.environment["TERN_BENCH"] == "1"
    }

    static func run(core: TernCore, panel: @MainActor @escaping () -> LauncherPanel?) async {
        try? await Task.sleep(for: idleDelay)
        emit("idle")
        guard await waitForContinue() else { return finish() }

        guard let launcher = panel() else { return finish() }
        launcher.present()
        fillInput(of: launcher)
        emit("panel_shown")
        guard await waitForContinue() else { return finish() }

        launcher.hide()
        core.trimMemory()
        try? await Task.sleep(for: hiddenSettleDelay)
        emit("panel_hidden")
        guard await waitForContinue() else { return finish() }

        emit("shutdown")
        finish()
    }

    private static func fillInput(of panel: LauncherPanel) {
        guard let editor = panel.firstResponder as? NSTextView else {
            writeError("bench: input field is not first responder")
            return
        }
        editor.insertText(fillText, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    private static func emit(_ phase: String) {
        let line = "{\"phase\":\"\(phase)\",\"ready\":true}\n"
        FileHandle.standardOutput.write(Data(line.utf8))
    }

    private static func writeError(_ message: String) {
        FileHandle.standardError.write(Data((message + "\n").utf8))
    }

    private static func waitForContinue() async -> Bool {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                continuation.resume(returning: readLine(strippingNewline: true) != nil)
            }
        }
    }

    private static func finish() {
        NSApp.terminate(nil)
    }
}
