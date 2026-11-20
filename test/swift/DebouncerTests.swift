import Foundation
import TernKit
import XCTest

private final class ValueBox: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: [Int] = []

    var values: [Int] {
        lock.lock()
        defer { lock.unlock() }
        return stored
    }

    func append(_ value: Int) {
        lock.lock()
        stored.append(value)
        lock.unlock()
    }
}

final class DebouncerTests: XCTestCase {
    func testOnlyLastScheduledWorkRuns() {
        let queue = DispatchQueue(label: "dev.tern.tests.debouncer.last")
        let debouncer = Debouncer(delay: 0.02, queue: queue)
        let box = ValueBox()
        let finished = DispatchSemaphore(value: 0)
        debouncer.schedule { box.append(1) }
        debouncer.schedule { box.append(2) }
        debouncer.schedule {
            box.append(3)
            finished.signal()
        }
        XCTAssertEqual(finished.wait(timeout: .now() + 2), .success)
        queue.sync {}
        XCTAssertEqual(box.values, [3])
    }

    func testCancelPreventsScheduledWork() {
        let queue = DispatchQueue(label: "dev.tern.tests.debouncer.cancel")
        let debouncer = Debouncer(delay: 0.02, queue: queue)
        let box = ValueBox()
        debouncer.schedule { box.append(1) }
        debouncer.cancel()
        Thread.sleep(forTimeInterval: 0.1)
        queue.sync {}
        XCTAssertEqual(box.values, [])
    }

    func testWorkRunsAfterCancelWhenScheduledAgain() {
        let queue = DispatchQueue(label: "dev.tern.tests.debouncer.reschedule")
        let debouncer = Debouncer(delay: 0.02, queue: queue)
        let box = ValueBox()
        let finished = DispatchSemaphore(value: 0)
        debouncer.schedule { box.append(1) }
        debouncer.cancel()
        debouncer.schedule {
            box.append(2)
            finished.signal()
        }
        XCTAssertEqual(finished.wait(timeout: .now() + 2), .success)
        queue.sync {}
        XCTAssertEqual(box.values, [2])
    }
}
