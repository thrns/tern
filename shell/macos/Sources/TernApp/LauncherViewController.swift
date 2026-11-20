import AppKit

@MainActor
final class LauncherViewController: NSViewController, NSTextFieldDelegate {
    var onDismiss: (@MainActor () -> Void)?

    private let inputField = NSTextField()

    override func loadView() {
        let background = NSVisualEffectView(
            frame: NSRect(x: 0, y: 0, width: LauncherPanel.width, height: LauncherPanel.minimumHeight)
        )
        background.material = .hudWindow
        background.blendingMode = .behindWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = LauncherPanel.cornerRadius
        background.layer?.masksToBounds = true

        inputField.isBezeled = false
        inputField.isBordered = false
        inputField.drawsBackground = false
        inputField.focusRingType = .none
        inputField.font = NSFont.systemFont(ofSize: 22, weight: .regular)
        inputField.placeholderString = "Search or ask"
        inputField.lineBreakMode = .byClipping
        inputField.usesSingleLineMode = true
        inputField.cell?.isScrollable = true
        inputField.delegate = self
        inputField.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(inputField)

        NSLayoutConstraint.activate([
            inputField.leadingAnchor.constraint(equalTo: background.leadingAnchor, constant: 18),
            inputField.trailingAnchor.constraint(equalTo: background.trailingAnchor, constant: -18),
            inputField.centerYAnchor.constraint(equalTo: background.topAnchor, constant: LauncherPanel.minimumHeight / 2),
        ])
        view = background
    }

    func focusInput() {
        view.window?.makeFirstResponder(inputField)
    }

    var isInputFocused: Bool {
        guard let window = view.window, let editor = window.firstResponder as? NSTextView else { return false }
        return editor.delegate === inputField && window.isKeyWindow
    }

    func control(
        _ control: NSControl,
        textView: NSTextView,
        doCommandBy commandSelector: Selector
    ) -> Bool {
        guard commandSelector == #selector(NSResponder.cancelOperation(_:)) else { return false }
        onDismiss?()
        return true
    }
}
