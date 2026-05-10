// SPDX-License-Identifier: Apache-2.0
//
// Ed25519: sign / verify wrapper over the vendored OpenSSL backend.
//
// Why NOT CryptoKit: Apple's CryptoKit on macOS / iOS implements
// `Curve25519.Signing` with intentional signature randomization (a
// fault-injection mitigation that mixes a CSPRNG nonce into every
// signature). RFC 8032 mandates deterministic signatures, and every
// other ProvekIt kit (rust ed25519-dalek, go crypto/ed25519, cpp
// OpenSSL EVP_PKEY_ED25519, c the same vendored wrapper) is RFC-8032
// deterministic. CryptoKit's randomization breaks the byte-equivalence
// contract Side B requires across all 11 kits.
//
// swift-crypto's `Crypto` module re-exports CryptoKit on Apple platforms
// (the BoringSSL backend is gated off via the `CRYPTO_IN_SWIFTPM`
// platform check; see swift-crypto's Package.swift), so it inherits the
// same randomization. We use OpenSSL libcrypto via the kit's existing
// `tools/ed25519-vendored/` thin wrapper: the same backing the C and
// C++ peer kits use: for byte-identical signatures with rust's
// ed25519-dalek and go's crypto/ed25519.
//
// Cross-kit conformance: the protocol pins the Foundation v0 ed25519
// seed `[0x42; 32]` (cf. tools/foundation-keygen/src/lib.rs). The same
// 32-byte seed produces the same 32-byte public key and the same
// 64-byte signature across all 11 kits because Ed25519 is deterministic.

import Foundation
import CEd25519

public enum Ed25519 {

    /// Foundation v0 seed: 32 bytes of 0x42. Pinned across every kit so
    /// the reproducible mints are byte-equivalent. Mirrors:
    ///   tools/foundation-keygen/src/lib.rs::FOUNDATION_V0_SEED
    public static let foundationV0Seed: [UInt8] = Array(repeating: 0x42, count: 32)

    /// Derive the 32-byte raw public key from a 32-byte seed.
    public static func publicKey(fromSeed seed: [UInt8]) -> [UInt8] {
        precondition(seed.count == 32, "Ed25519 seed must be 32 bytes")
        var pk = [UInt8](repeating: 0, count: 32)
        let rc = seed.withUnsafeBufferPointer { sptr -> Int32 in
            pk.withUnsafeMutableBufferPointer { pptr -> Int32 in
                pk_ed25519_pubkey_from_seed(sptr.baseAddress, pptr.baseAddress)
            }
        }
        precondition(rc == 0, "pk_ed25519_pubkey_from_seed failed")
        return pk
    }

    /// Self-identifying form: "ed25519:" + base64(publicKey). Used as
    /// the `signer` field in the self-contracts attestations.
    public static func publicKeyString(fromSeed seed: [UInt8]) -> String {
        let pk = publicKey(fromSeed: seed)
        return "ed25519:" + Data(pk).base64EncodedString()
    }

    /// Sign `message` with `seed`. Returns a 64-byte raw signature
    /// (RFC 8032 deterministic).
    public static func sign(message: Data, seed: [UInt8]) -> [UInt8] {
        precondition(seed.count == 32, "Ed25519 seed must be 32 bytes")
        var sig = [UInt8](repeating: 0, count: 64)
        let rc = message.withUnsafeBytes { mptr -> Int32 in
            seed.withUnsafeBufferPointer { sptr -> Int32 in
                sig.withUnsafeMutableBufferPointer { sigptr -> Int32 in
                    pk_ed25519_sign(
                        mptr.bindMemory(to: UInt8.self).baseAddress,
                        message.count,
                        sptr.baseAddress,
                        sigptr.baseAddress
                    )
                }
            }
        }
        precondition(rc == 0, "pk_ed25519_sign failed")
        return sig
    }

    /// Verify `signature` over `message` for the public key `pubKey`.
    public static func verify(message: Data, signature: [UInt8], pubKey: [UInt8]) -> Bool {
        guard signature.count == 64, pubKey.count == 32 else { return false }
        let rc = message.withUnsafeBytes { mptr -> Int32 in
            signature.withUnsafeBufferPointer { sigptr -> Int32 in
                pubKey.withUnsafeBufferPointer { pkptr -> Int32 in
                    pk_ed25519_verify(
                        mptr.bindMemory(to: UInt8.self).baseAddress,
                        message.count,
                        sigptr.baseAddress,
                        pkptr.baseAddress
                    )
                }
            }
        }
        return rc == 1
    }

    /// Self-identifying form for member-envelope signatures:
    /// "ed25519:" + base64(signature). Mirrors
    /// `Ed25519SigPrefix + base64.StdEncoding.EncodeToString(sig)` in
    /// the go claim_envelope finalize step.
    public static func signatureString(message: Data, seed: [UInt8]) -> String {
        let sig = sign(message: message, seed: seed)
        return "ed25519:" + Data(sig).base64EncodedString()
    }
}
