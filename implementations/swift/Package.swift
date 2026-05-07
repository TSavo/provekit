// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

// Optional override for non-Homebrew OpenSSL prefix. Mirrors the C++
// peer build script (tools/run-proof-envelope-conformance.sh):
//   OPENSSL_PREFIX=/usr/local/opt/openssl@3 swift build
import Foundation
let opensslPrefix: String = {
    if let env = ProcessInfo.processInfo.environment["OPENSSL_PREFIX"], !env.isEmpty {
        return env
    }
    // Default for Homebrew on x86_64 macOS. ARM Macs use /opt/homebrew.
    if FileManager.default.fileExists(atPath: "/opt/homebrew/opt/openssl@3/include/openssl/evp.h") {
        return "/opt/homebrew/opt/openssl@3"
    }
    return "/usr/local/opt/openssl@3"
}()

let package = Package(
    name: "Provekit",
    // Issue #155 / #212: swift kit is macOS-only. The Foundation v0
    // ed25519 path uses OpenSSL libcrypto (same backing as the C and
    // C++ peer kits) for RFC-8032 deterministic signatures; macOS 10.15
    // is the minimum for the rest of the kit's API surface.
    platforms: [
        .macOS(.v10_15),
    ],
    products: [
        .library(name: "Provekit", targets: ["Provekit"]),
        .library(name: "ProvekitCrypto", targets: ["ProvekitCrypto"]),
        .library(name: "SwiftLifter", targets: ["SwiftLifter"]),
        .executable(name: "conformance", targets: ["ConformanceRunner"]),
        .executable(name: "provekit-lsp-swift", targets: ["ProveKitLSPSwift"]),
        .executable(name: "mint-swift-self-contracts", targets: ["MintSwiftSelfContracts"]),
        .executable(name: "test-swift-lsp", targets: ["LSPTests"]),
        .executable(name: "test-swift-crypto", targets: ["CryptoTests"]),
    ],
    dependencies: [
        // SwiftSyntax: Apple's official swift-syntax for AST-based parsing.
        // Pinned to 600.0.x for compatibility with swift-tools-version 6.0.
        .package(
            url: "https://github.com/swiftlang/swift-syntax.git",
            from: "600.0.0"
        ),
    ],
    targets: [
        // CBlake3: vendored portable BLAKE3 reference C implementation.
        // Source: tools/blake3-vendored/, BLAKE3 1.8.5, Apache-2.0.
        // Public header: include/blake3.h. The portable path is selected
        // unconditionally via -DBLAKE3_NO_AVX2/SSE2/SSE41/AVX512 + USE_NEON=0,
        // matching the C++ peer self-contracts orchestrator's flags.
        // Performance is irrelevant for the tens of contracts the kit hashes.
        .target(
            name: "CBlake3",
            publicHeadersPath: "include",
            cSettings: [
                .define("BLAKE3_NO_AVX2"),
                .define("BLAKE3_NO_AVX512"),
                .define("BLAKE3_NO_SSE2"),
                .define("BLAKE3_NO_SSE41"),
                .define("BLAKE3_USE_NEON", to: "0"),
            ]
        ),
        // CEd25519: vendored thin wrapper over OpenSSL EVP_PKEY_ED25519.
        // Source: tools/ed25519-vendored/, Apache-2.0.
        //
        // Why OpenSSL: the cpp + c peer kits use the same backing
        // (`EVP_PKEY_ED25519`) for byte-identical signatures with rust's
        // ed25519-dalek and go's crypto/ed25519. Apple's CryptoKit on
        // macOS / iOS implements `Curve25519.Signing` with intentional
        // signature randomization (a fault-injection mitigation that
        // mixes a CSPRNG nonce into every signature). RFC 8032 mandates
        // deterministic signatures; CryptoKit deviates. swift-crypto's
        // BoringSSL backend is gated off on Apple platforms (it
        // re-exports CryptoKit), so neither library produces the
        // deterministic signature the cross-kit byte-equivalence AC
        // requires. Using the same OpenSSL primitive every other C-side
        // kit uses guarantees byte-level agreement with zero ambiguity.
        //
        // Build requirement: OpenSSL 1.1+ or 3.x at $OPENSSL_PREFIX
        // (default /usr/local/opt/openssl@3, falling back to
        // /opt/homebrew/opt/openssl@3 on ARM Macs).
        .target(
            name: "CEd25519",
            publicHeadersPath: "include",
            cSettings: [
                .unsafeFlags(["-I\(opensslPrefix)/include"]),
            ],
            linkerSettings: [
                .unsafeFlags([
                    "-L\(opensslPrefix)/lib",
                    "-lcrypto",
                ]),
            ]
        ),
        // ProvekitCrypto: native Swift substrate (Side B per issue #176/#212).
        // Provides BLAKE3-512, deterministic CBOR (RFC 8949 §4.2.1),
        // RFC 8785 JCS, Ed25519 (vendored OpenSSL wrapper), and
        // claim/proof envelope construction. No shell-out; all
        // primitives are in-process and byte-equivalent to the rust /
        // go / cpp / c peer kits.
        .target(
            name: "ProvekitCrypto",
            dependencies: ["CBlake3", "CEd25519"]
        ),
        .target(
            name: "Provekit",
            dependencies: ["ProvekitCrypto"]
        ),
        .target(
            name: "SwiftLifter",
            dependencies: [
                .product(name: "SwiftSyntax", package: "swift-syntax"),
                .product(name: "SwiftParser", package: "swift-syntax"),
            ]
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
            dependencies: ["Provekit", "ProvekitCrypto"]
        ),
        // LSPTests: standalone integration test runner (no XCTest/Testing dep needed).
        // `swift run test-swift-lsp` returns exit 0 on pass, non-zero on fail.
        .executableTarget(
            name: "LSPTests",
            dependencies: ["SwiftLifter"]
        ),
        // CryptoTests: substrate determinism + cross-kit byte-equivalence.
        // Standalone executable runner mirroring LSPTests' pattern (no
        // XCTest/Testing dep — neither is available on CI without full Xcode).
        // `swift run test-swift-crypto` exits 0 on pass, 1 on any fail.
        .executableTarget(
            name: "CryptoTests",
            dependencies: ["ProvekitCrypto"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
