// swift-tools-version: 6.0
import Foundation
import PackageDescription

let profile = ProcessInfo.processInfo.environment["TERN_PROFILE"] ?? "debug"
let libraryDirectory = "\(Context.packageDirectory)/target/aarch64-apple-darwin/\(profile)"

let package = Package(
    name: "Tern",
    platforms: [.macOS(.v14)],
    targets: [
        .target(
            name: "tern_ffiFFI",
            path: "shell/macos/Sources/TernFFIC",
            linkerSettings: [
                .unsafeFlags(["-Xlinker", "\(libraryDirectory)/libtern_ffi.a"])
            ]
        ),
        .target(
            name: "TernFFI",
            dependencies: ["tern_ffiFFI"],
            path: "shell/macos/Sources/TernFFI"
        ),
        .target(
            name: "TernKit",
            path: "shell/macos/Sources/TernKit"
        ),
        .executableTarget(
            name: "TernApp",
            dependencies: ["TernFFI", "TernKit"],
            path: "shell/macos/Sources/TernApp"
        ),
        .testTarget(
            name: "TernKitTests",
            dependencies: ["TernKit", "TernApp", "TernFFI"],
            path: "test/swift"
        ),
    ]
)
