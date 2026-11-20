import AppKit
import OSLog
import TernKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private static let log = Logger(subsystem: "dev.tern.Tern", category: "panel")

    private static let prewarmDelay: Duration = .seconds(3)
    private static let bundleIdentifier = "dev.tern.Tern"
    private static let showPanelNotification = Notification.Name("dev.tern.Tern.showPanel")
    private static let handoffFlushDelay: Duration = .milliseconds(150)

    private var bridge: CoreBridge?
    private var statusItemController: StatusItemController?
    private var hotKeyCenter: HotKeyCenter?
    private var launcherPanel: LauncherPanel?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if hasRunningPeer() {
            handOffToRunningInstance()
            return
        }
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
        observeShowPanelRequests()
        schedulePrewarm()
        startBenchIfRequested()
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        Self.log.debug("reopen handler called, visible windows: \(flag, privacy: .public)")
        revealPanel()
        return false
    }

    private func revealPanel() {
        if let launcherPanel, launcherPanel.isVisible {
            launcherPanel.present()
            return
        }
        NSApp.activate()
        Task { @MainActor [weak self] in
            self?.showPanel()
        }
    }

    private func hasRunningPeer() -> Bool {
        guard !BenchMode.isEnabled else { return false }
        let current = ProcessInfo.processInfo.processIdentifier
        return NSRunningApplication
            .runningApplications(withBundleIdentifier: Self.bundleIdentifier)
            .contains { $0.processIdentifier != current }
    }

    private func handOffToRunningInstance() {
        DistributedNotificationCenter.default().postNotificationName(
            Self.showPanelNotification,
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
        Task { @MainActor in
            try? await Task.sleep(for: Self.handoffFlushDelay)
            NSApp.terminate(nil)
        }
    }

    private func observeShowPanelRequests() {
        DistributedNotificationCenter.default().addObserver(
            forName: Self.showPanelNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.revealPanel()
            }
        }
    }

    private func startBenchIfRequested() {
        guard BenchMode.isEnabled, let core = bridge?.core else { return }
        Task { @MainActor [weak self] in
            await BenchMode.run(core: core) { [weak self] in
                self?.ensurePanel() ?? LauncherPanel()
            }
        }
    }

    private func ensurePanel() -> LauncherPanel {
        let panel = launcherPanel ?? LauncherPanel()
        launcherPanel = panel
        return panel
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
