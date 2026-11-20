import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var bridge: CoreBridge?
    private var statusItemController: StatusItemController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            bridge = try CoreBridge.start()
        } catch {
            presentStartupFailure(error)
            NSApp.terminate(nil)
            return
        }
        statusItemController = StatusItemController()
    }

    func applicationWillTerminate(_ notification: Notification) {
        bridge?.core.shutdown()
    }

    private func presentStartupFailure(_ error: Error) {
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = "Tern could not start"
        alert.informativeText = CoreBridge.describe(error)
        alert.addButton(withTitle: "Quit")
        NSApp.activate()
        alert.runModal()
    }
}
