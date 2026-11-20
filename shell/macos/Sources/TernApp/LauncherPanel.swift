import AppKit
import OSLog
import TernFFI

@MainActor
final class LauncherPanel: NSPanel {
    static let width: CGFloat = 680
    static let minimumHeight: CGFloat = 56
    static let cornerRadius: CGFloat = 14
    static let maximumHeight: CGFloat = 520
    private static let topInsetFraction: CGFloat = 0.22

    private static let log = Logger(subsystem: "dev.tern.Tern", category: "panel")

    private static let resignGraceInterval: Duration = .milliseconds(300)

    private var presentedAt: ContinuousClock.Instant?
    private let launcherController: LauncherViewController

    init(core: TernCore) {
        launcherController = LauncherViewController(core: core)
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: Self.width, height: Self.minimumHeight),
            styleMask: [.nonactivatingPanel, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        hidesOnDeactivate = false
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isReleasedWhenClosed = false
        animationBehavior = .none
        contentViewController = launcherController
        setContentSize(NSSize(width: Self.width, height: Self.minimumHeight))
        launcherController.onDismiss = { [weak self] in self?.hide() }
        launcherController.onContentHeightChange = { [weak self] height in self?.resize(toContentHeight: height) }
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(panelDidResignKey(_:)),
            name: NSWindow.didResignKeyNotification,
            object: self
        )
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    var isInputFocused: Bool { launcherController.isInputFocused }

    func toggle() {
        if isVisible {
            hide()
        } else {
            present()
        }
    }

    func prewarm() {
        contentView?.layoutSubtreeIfNeeded()
        launcherController.prepareInput(in: self)
        alphaValue = 0
        ignoresMouseEvents = true
        position()
        orderFrontRegardless()
        displayIfNeeded()
        orderOut(nil)
        ignoresMouseEvents = false
        alphaValue = 1
    }

    func present() {
        presentedAt = .now
        position()
        makeKeyAndOrderFront(nil)
        launcherController.focusInput()
        Self.log.debug(
            "panel presented, key: \(self.isKeyWindow, privacy: .public), app active: \(NSApp.isActive, privacy: .public)"
        )
    }

    func hide() {
        guard isVisible else { return }
        Self.log.debug("panel hidden")
        orderOut(nil)
    }

    override func cancelOperation(_ sender: Any?) {
        hide()
    }

    @objc private func panelDidResignKey(_ notification: Notification) {
        let withinGrace = presentedAt.map { $0.duration(to: .now) < Self.resignGraceInterval } ?? false
        Self.log.debug(
            "panel resigned key, visible: \(self.isVisible, privacy: .public), ignored: \(withinGrace, privacy: .public), app active: \(NSApp.isActive, privacy: .public)"
        )
        guard !withinGrace else { return }
        hide()
    }

    private func resize(toContentHeight height: CGFloat) {
        let clamped = min(max(height, Self.minimumHeight), Self.maximumHeight)
        let topLeft = NSPoint(x: frame.minX, y: frame.maxY)
        setContentSize(NSSize(width: Self.width, height: clamped))
        setFrameTopLeftPoint(topLeft)
    }

    private func position() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { $0.frame.contains(mouse) })
            ?? NSScreen.main
            ?? NSScreen.screens.first
        guard let screen else { return }
        let frame = screen.frame
        let topLeft = NSPoint(
            x: frame.midX - Self.width / 2,
            y: frame.maxY - frame.height * Self.topInsetFraction
        )
        setFrameTopLeftPoint(topLeft)
    }
}
