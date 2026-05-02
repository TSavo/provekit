// swift-tools-version: 6.3
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Provekit",
    products: [
        .library(name: "Provekit", targets: ["Provekit"]),
        .executable(name: "conformance", targets: ["ConformanceRunner"]),
    ],
    targets: [
        .target(name: "Provekit"),
        .executableTarget(
            name: "ConformanceRunner",
            dependencies: ["Provekit"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
