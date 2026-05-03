// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Provekit",
    products: [
        .library(name: "Provekit", targets: ["Provekit"]),
        .library(name: "SwiftLifter", targets: ["SwiftLifter"]),
        .executable(name: "conformance", targets: ["ConformanceRunner"]),
        .executable(name: "provekit-lsp-swift", targets: ["ProveKitLSPSwift"]),
        .executable(name: "mint-swift-self-contracts", targets: ["MintSwiftSelfContracts"]),
        .executable(name: "test-swift-lsp", targets: ["LSPTests"]),
    ],
    targets: [
        .target(name: "Provekit"),
        .target(
            name: "SwiftLifter",
            dependencies: []
        ),
        .executableTarget(
            name: "ConformanceRunner",
            dependencies: ["Provekit"]
        ),
        .executableTarget(
            name: "ProveKitLSPSwift",
            dependencies: ["SwiftLifter"]
        ),
        .executableTarget(
            name: "MintSwiftSelfContracts",
            dependencies: ["Provekit"]
        ),
        // LSPTests: standalone integration test runner (no XCTest/Testing dep needed).
        // `swift run test-swift-lsp` returns exit 0 on pass, non-zero on fail.
        .executableTarget(
            name: "LSPTests",
            dependencies: ["SwiftLifter"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
