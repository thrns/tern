import AppKit

enum ResultRow: Equatable {
    case calc(display: String, approximate: Bool)
}

@MainActor
final class ResultsListView: NSScrollView, NSTableViewDataSource, NSTableViewDelegate {
    static let rowHeight: CGFloat = 44
    static let maximumHeight: CGFloat = 464

    private static let columnIdentifier = NSUserInterfaceItemIdentifier("result")
    private static let calcCellIdentifier = NSUserInterfaceItemIdentifier("calcCell")

    private let table = NSTableView()
    private var rows: [ResultRow] = []

    var contentHeight: CGFloat {
        min(CGFloat(rows.count) * Self.rowHeight, Self.maximumHeight)
    }

    var firstCalcDisplay: String? {
        guard case let .calc(display, _)? = rows.first else { return nil }
        return display
    }

    init() {
        super.init(frame: .zero)
        let column = NSTableColumn(identifier: Self.columnIdentifier)
        column.width = LauncherPanel.width
        table.addTableColumn(column)
        table.headerView = nil
        table.rowHeight = Self.rowHeight
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.selectionHighlightStyle = .none
        table.backgroundColor = .clear
        table.focusRingType = .none
        table.dataSource = self
        table.delegate = self
        documentView = table
        drawsBackground = false
        contentView.drawsBackground = false
        hasVerticalScroller = false
        autohidesScrollers = true
        borderType = .noBorder
    }

    required init?(coder: NSCoder) {
        nil
    }

    func update(_ newRows: [ResultRow]) {
        rows = newRows
        table.reloadData()
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        rows.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard rows.indices.contains(row) else { return nil }
        let cell = tableView.makeView(withIdentifier: Self.calcCellIdentifier, owner: self) as? CalcCellView
            ?? CalcCellView(identifier: Self.calcCellIdentifier)
        switch rows[row] {
        case let .calc(display, approximate):
            cell.configure(display: display, approximate: approximate)
        }
        return cell
    }
}

@MainActor
private final class CalcCellView: NSTableCellView {
    private let valueLabel = NSTextField(labelWithString: "")
    private let hintLabel = NSTextField(labelWithString: "Enter to copy")

    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier
        valueLabel.font = NSFont.monospacedSystemFont(ofSize: 18, weight: .medium)
        valueLabel.textColor = .labelColor
        valueLabel.lineBreakMode = .byTruncatingTail
        hintLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        hintLabel.textColor = .secondaryLabelColor
        hintLabel.alignment = .right
        for label in [valueLabel, hintLabel] {
            label.translatesAutoresizingMaskIntoConstraints = false
            addSubview(label)
        }
        NSLayoutConstraint.activate([
            valueLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            valueLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            valueLabel.trailingAnchor.constraint(lessThanOrEqualTo: hintLabel.leadingAnchor, constant: -12),
            hintLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -18),
            hintLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    func configure(display: String, approximate: Bool) {
        valueLabel.stringValue = (approximate ? "≈ " : "= ") + display
    }
}
