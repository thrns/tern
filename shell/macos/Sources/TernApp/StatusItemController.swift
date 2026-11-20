import AppKit

@MainActor
final class StatusItemController: NSObject {
    var onOpen: (@MainActor () -> Void)?
    var onSettings: (@MainActor () -> Void)?

    private let statusItem: NSStatusItem

    override init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        configureButton()
        statusItem.menu = makeMenu()
    }

    private func configureButton() {
        guard let button = statusItem.button else { return }
        let symbol = NSImage(
            systemSymbolName: "sparkle.magnifyingglass",
            accessibilityDescription: "Tern"
        )
        if let symbol {
            symbol.isTemplate = true
            button.image = symbol
        } else {
            button.title = "Tern"
        }
    }

    private func makeMenu() -> NSMenu {
        let menu = NSMenu()
        let openItem = NSMenuItem(title: "Open Tern", action: #selector(openSelected), keyEquivalent: "")
        openItem.target = self
        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(settingsSelected), keyEquivalent: "")
        settingsItem.target = self
        let quitItem = NSMenuItem(title: "Quit Tern", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        quitItem.keyEquivalentModifierMask = .command
        quitItem.target = NSApp
        menu.addItem(openItem)
        menu.addItem(settingsItem)
        menu.addItem(.separator())
        menu.addItem(quitItem)
        return menu
    }

    @objc private func openSelected() {
        onOpen?()
    }

    @objc private func settingsSelected() {
        onSettings?()
    }
}
