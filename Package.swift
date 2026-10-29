// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RamplePreview",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "RamplePreview", targets: ["RamplePreview"])],
    targets: [
        .executableTarget(name: "RamplePreview", path: "Sources"),
        .testTarget(name: "RamplePreviewTests", dependencies: ["RamplePreview"], path: "Tests")
    ]
)
