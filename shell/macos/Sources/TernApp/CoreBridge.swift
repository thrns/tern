import Foundation
import TernFFI

private final class ListenerRelay: CoreListener, @unchecked Sendable {
    private let lock = NSLock()
    private weak var target: CoreBridge?

    func attach(_ bridge: CoreBridge) {
        lock.lock()
        target = bridge
        lock.unlock()
    }

    private func current() -> CoreBridge? {
        lock.lock()
        defer { lock.unlock() }
        return target
    }

    func onAskEvent(askId: String, event: AskEvent) {
        current()?.onAskEvent(askId: askId, event: event)
    }

    func onIndexStatus(status: IndexStatus) {
        current()?.onIndexStatus(status: status)
    }

    func onAiCapture(path: String) {
        current()?.onAiCapture(path: path)
    }
}

final class CoreBridge: CoreListener, @unchecked Sendable {
    let core: TernCore

    private init(core: TernCore) {
        self.core = core
    }

    static func start() throws -> CoreBridge {
        let relay = ListenerRelay()
        let core = try TernCore(homeDir: nil, listener: relay)
        let bridge = CoreBridge(core: core)
        relay.attach(bridge)
        return bridge
    }

    static func describe(_ error: Error) -> String {
        if let failure = error as? FfiError, case let .Failed(kind, message) = failure {
            return "\(kind): \(message)"
        }
        return error.localizedDescription
    }

    func onAskEvent(askId: String, event: AskEvent) {
        DispatchQueue.main.async {}
    }

    func onIndexStatus(status: IndexStatus) {
        DispatchQueue.main.async {}
    }

    func onAiCapture(path: String) {
        DispatchQueue.main.async {}
    }
}
