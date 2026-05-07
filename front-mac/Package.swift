// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "CPUMonitorTray",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "CPUMonitorTray",
            resources: [.process("Resources")]
        )
    ]
)
