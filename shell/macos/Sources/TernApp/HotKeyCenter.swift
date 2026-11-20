import Carbon.HIToolbox
import Foundation
import TernKit

@MainActor
final class HotKeyCenter {
    enum Action: UInt32 {
        case togglePanel = 1
        case captureAndAsk = 2
    }

    var handler: ((Action) -> Void)?

    private static let signature: OSType = 0x5445_524E

    private var registrations: [Action: EventHotKeyRef] = [:]
    private var eventHandler: EventHandlerRef?

    func register(_ spec: HotKeySpec, action: Action) -> Bool {
        guard installEventHandlerIfNeeded() else { return false }
        unregister(action)
        let identifier = EventHotKeyID(signature: Self.signature, id: action.rawValue)
        var reference: EventHotKeyRef?
        let status = RegisterEventHotKey(
            spec.keyCode,
            spec.carbonModifiers,
            identifier,
            GetApplicationEventTarget(),
            0,
            &reference
        )
        guard status == noErr, let reference else { return false }
        registrations[action] = reference
        return true
    }

    func unregisterAll() {
        for reference in registrations.values {
            UnregisterEventHotKey(reference)
        }
        registrations.removeAll()
        if let eventHandler {
            RemoveEventHandler(eventHandler)
            self.eventHandler = nil
        }
    }

    fileprivate func dispatch(identifier: UInt32) {
        guard let action = Action(rawValue: identifier) else { return }
        handler?(action)
    }

    private func unregister(_ action: Action) {
        guard let reference = registrations.removeValue(forKey: action) else { return }
        UnregisterEventHotKey(reference)
    }

    private func installEventHandlerIfNeeded() -> Bool {
        if eventHandler != nil { return true }
        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )
        var installed: EventHandlerRef?
        let status = InstallEventHandler(
            GetApplicationEventTarget(),
            hotKeyEventCallback,
            1,
            &eventType,
            Unmanaged.passUnretained(self).toOpaque(),
            &installed
        )
        guard status == noErr, let installed else { return false }
        eventHandler = installed
        return true
    }
}

private func hotKeyEventCallback(
    _ nextHandler: EventHandlerCallRef?,
    _ event: EventRef?,
    _ userData: UnsafeMutableRawPointer?
) -> OSStatus {
    guard let event, let userData else { return OSStatus(eventNotHandledErr) }
    var identifier = EventHotKeyID()
    let status = GetEventParameter(
        event,
        EventParamName(kEventParamDirectObject),
        EventParamType(typeEventHotKeyID),
        nil,
        MemoryLayout<EventHotKeyID>.size,
        nil,
        &identifier
    )
    guard status == noErr else { return status }
    let center = Unmanaged<HotKeyCenter>.fromOpaque(userData).takeUnretainedValue()
    let pressed = identifier.id
    if Thread.isMainThread {
        MainActor.assumeIsolated { center.dispatch(identifier: pressed) }
    } else {
        DispatchQueue.main.async { center.dispatch(identifier: pressed) }
    }
    return noErr
}
