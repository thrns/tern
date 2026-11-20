import AppKit
import OSLog
import TernKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private static let log = Logger(subsystem: "dev.tern.Tern", category: "panel")

    private static let prewarmDelay: Duration = .seconds(3)

    private var bridge: CoreBridge?
    private var statusItemController: StatusItemController?
    private var hotKeyCenter: HotKeyCenter?
    private var launcherPanel: LauncherPanel?

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            bridge = try CoreBridge.start()
        } catch {
            presentStartupFailure(error)
            NSApp.terminate(nil)
            return
        }
        let statusItemController = StatusItemController()
        statusItemController.onOpen = { [weak self] in self?.togglePanel() }
        self.statusItemController = statusItemController
        registerHotKeys()
        schedulePrewarm()
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        Self.log.debug("reopen handler called, visible windows: \(flag, privacy: .public)")
        if let launcherPanel, launcherPanel.isVisible {
            launcherPanel.present()
            return false
        }
        NSApp.activate()
        Task { @MainActor [weak self] in
            self?.showPanel()
        }
        return false
    }

    func applicationWillTerminate(_ notification: Notification) {
        hotKeyCenter?.unregisterAll()
        bridge?.core.shutdown()
    }

    private func registerHotKeys() {
        guard let bridge else { return }
        let configured = bridge.core.settings().hotkeyPanel
        let center = HotKeyCenter()
        center.handler = { [weak self] action in
            switch action {
            case .togglePanel:
                self?.togglePanel()
            case .captureAndAsk:
                break
            }
        }
        hotKeyCenter = center
        guard let spec = HotKeySpec.parse(configured), center.register(spec, action: .togglePanel) else {
            presentHotKeyFailure(configured)
            return
        }
    }

    private func schedulePrewarm() {
        guard ProcessInfo.processInfo.environment["TERN_BENCH"] != "1" else { return }
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: Self.prewarmDelay)
            self?.prewarmPanel()
        }
    }

    private func prewarmPanel() {
        guard launcherPanel == nil else { return }
        let panel = LauncherPanel()
        panel.prewarm()
        launcherPanel = panel
        Self.log.debug("panel prewarmed")
    }

    private func showPanel() {
        let started = ContinuousClock.now
        let wasPrewarmed = launcherPanel != nil
        let panel = launcherPanel ?? LauncherPanel()
        launcherPanel = panel
        guard !panel.isVisible else { return }
        panel.present()
        reportShown(panel, since: started, prewarmed: wasPrewarmed)
    }

    private func togglePanel() {
        let started = ContinuousClock.now
        let wasPrewarmed = launcherPanel != nil
        let panel = launcherPanel ?? LauncherPanel()
        launcherPanel = panel
        panel.toggle()
        guard panel.isVisible else { return }
        reportShown(panel, since: started, prewarmed: wasPrewarmed)
    }

    private func reportShown(
        _ panel: LauncherPanel,
        since started: ContinuousClock.Instant,
        prewarmed: Bool
    ) {
        let elapsed = started.duration(to: .now)
        let milliseconds = Double(elapsed.components.attoseconds) / 1e15
            + Double(elapsed.components.seconds) * 1000
        Self.log.info(
            "panel shown in \(milliseconds, privacy: .public) ms, prewarmed: \(prewarmed, privacy: .public), input focused: \(panel.isInputFocused, privacy: .public)"
        )
    }

    private func presentHotKeyFailure(_ configured: String) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "Tern could not register its hotkey"
        alert.informativeText =
            "The hotkey \"\(configured)\" is invalid or already used by another app. "
            + "Set a different value for hotkey_panel in config.toml and restart Tern. "
            + "You can still open the panel from the menu bar."
        alert.addButton(withTitle: "OK")
        NSApp.activate()
        alert.runModal()
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
