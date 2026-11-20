import AppKit

@MainActor
final class LauncherPanel: NSPanel {
    static let width: CGFloat = 680
    static let minimumHeight: CGFloat = 56
    static let cornerRadius: CGFloat = 14
    private static let topInsetFraction: CGFloat = 0.22

    private let launcherController = LauncherViewController()

    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: Self.width, height: Self.minimumHeight),
            styleMask: [.nonactivatingPanel, .fullSizeContentView],
            backing: .buffered,
            defer: true
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

    func present() {
        position()
        makeKeyAndOrderFront(nil)
        launcherController.focusInput()
    }

    func hide() {
        guard isVisible else { return }
        orderOut(nil)
    }

    override func cancelOperation(_ sender: Any?) {
        hide()
    }

    @objc private func panelDidResignKey(_ notification: Notification) {
        hide()
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
