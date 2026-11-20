import AppKit
import TernFFI
import TernKit

@MainActor
final class LauncherViewController: NSViewController, NSTextFieldDelegate {
    fileprivate static let debounceDelay: TimeInterval = 0.06
    fileprivate static let coreQueue = DispatchQueue(label: "dev.tern.Tern.core", qos: .userInitiated)

    var onDismiss: (@MainActor () -> Void)?
    var onContentHeightChange: (@MainActor (CGFloat) -> Void)?

    private let core: TernCore
    private let inputField = NSTextField()
    private let resultsView = ResultsListView()
    private let calcDebouncer = Debouncer(delay: LauncherViewController.debounceDelay, queue: LauncherViewController.coreQueue)
    private var generation: UInt = 0
    private var resultsHeightConstraint: NSLayoutConstraint?

    private static let inputTopInset: CGFloat = 15

    init(core: TernCore) {
        self.core = core
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        nil
    }

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

        resultsView.translatesAutoresizingMaskIntoConstraints = false
        resultsView.isHidden = true
        background.addSubview(resultsView)

        let resultsHeight = resultsView.heightAnchor.constraint(equalToConstant: 0)
        resultsHeightConstraint = resultsHeight
        NSLayoutConstraint.activate([
            inputField.leadingAnchor.constraint(equalTo: background.leadingAnchor, constant: 18),
            inputField.trailingAnchor.constraint(equalTo: background.trailingAnchor, constant: -18),
            inputField.topAnchor.constraint(equalTo: background.topAnchor, constant: Self.inputTopInset),
            resultsView.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            resultsView.trailingAnchor.constraint(equalTo: background.trailingAnchor),
            resultsView.bottomAnchor.constraint(equalTo: background.bottomAnchor),
            resultsHeight,
        ])
        view = background
    }

    func prepareInput(in window: NSWindow) {
        _ = window.fieldEditor(true, for: inputField)
    }

    func focusInput() {
        view.window?.makeFirstResponder(inputField)
        scheduleCalculation()
    }

    var isInputFocused: Bool {
        guard let window = view.window, let editor = window.firstResponder as? NSTextView else { return false }
        return editor.delegate === inputField && window.isKeyWindow
    }

    func controlTextDidChange(_ notification: Notification) {
        scheduleCalculation()
    }

    func control(
        _ control: NSControl,
        textView: NSTextView,
        doCommandBy commandSelector: Selector
    ) -> Bool {
        switch commandSelector {
        case #selector(NSResponder.cancelOperation(_:)):
            onDismiss?()
            return true
        case #selector(NSResponder.insertNewline(_:)):
            guard let display = resultsView.firstCalcDisplay else { return true }
            copyToPasteboard(display)
            onDismiss?()
            return true
        default:
            return false
        }
    }

    private func scheduleCalculation() {
        generation &+= 1
        let token = generation
        let text = inputField.stringValue
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty else {
            calcDebouncer.cancel()
            renderCalc(nil)
            return
        }
        let core = core
        calcDebouncer.schedule {
            let output = core.calc(input: text)
            Task { @MainActor [weak self] in
                self?.finishCalculation(output, token: token)
            }
        }
    }

    private func finishCalculation(_ output: CalcOutput?, token: UInt) {
        guard token == generation, view.window?.isVisible == true else { return }
        renderCalc(output)
    }

    private func renderCalc(_ output: CalcOutput?) {
        let rows: [ResultRow] = output.map { [.calc(display: $0.display, approximate: $0.isApproximate)] } ?? []
        resultsView.update(rows)
        resultsView.isHidden = rows.isEmpty
        resultsHeightConstraint?.constant = resultsView.contentHeight
        onContentHeightChange?(LauncherPanel.minimumHeight + resultsView.contentHeight)
    }

    private func copyToPasteboard(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}
