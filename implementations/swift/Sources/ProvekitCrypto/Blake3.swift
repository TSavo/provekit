// SPDX-License-Identifier: Apache-2.0
//
// Blake3 — native BLAKE3-512 via the vendored portable C reference impl.
//
// Replaces the python-shellout in `Sources/Provekit/IR.swift`'s legacy
// `Blake3.hex(_:)`. Same output shape ("blake3-512:" + 128 lowercase hex
// chars), now in-process, deterministic, and dependency-free at runtime.
//
// Spec: protocol/specs/2026-04-30-canonicalization-grammar.md §11
//       protocol/specs/2026-04-30-memento-envelope-grammar.md §"Self-identifying"
//
// Cross-kit conformance: digests MUST match the rust/go/cpp/python kits
// for identical input bytes. The vendored C impl is the same source the
// C++ peer orchestrator uses, so byte-equivalence is mechanical.
//
// Output length is 64 bytes (BLAKE3 default). The protocol calls this
// "BLAKE3-512" (alias used elsewhere in the kit-source for parity with
// the SHA-512 family). Despite the name, BLAKE3 always produces a single
// 64-byte digest; "512" reflects the canonical bit-length used as the
// protocol's content-address.

import Foundation
import CBlake3

public enum Blake3 {

    /// Self-identifying CID: "blake3-512:" + 128 lowercase hex chars of
    /// the BLAKE3-512 digest of `data`. This is the protocol-mandated
    /// content-address shape (cf. `canonicalizer/hasher.go::ComputeCID`).
    public static func hex(_ data: Data) -> String {
        return "blake3-512:" + hex64(data)
    }

    /// 128-character lowercase hex digest, NOT prefixed. Matches
    /// `Hasher.Blake3_512Hex` in the go canonicalizer.
    public static func hex64(_ data: Data) -> String {
        let raw = digest(data)
        return raw.map { String(format: "%02x", $0) }.joined()
    }

    /// Raw 64-byte BLAKE3 digest of `data`. Allocates a fresh hasher per
    /// call; the kit hashes only kilobytes of contract bytes per mint, so
    /// reuse is not worth the API surface.
    public static func digest(_ data: Data) -> [UInt8] {
        var hasher = blake3_hasher()
        blake3_hasher_init(&hasher)
        data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            // BLAKE3 accepts zero-length updates per the reference impl;
            // skip the call when raw.baseAddress is nil to avoid a Swift
            // strictness warning about passing nil to a non-nullable arg.
            if let base = raw.baseAddress, raw.count > 0 {
                blake3_hasher_update(&hasher, base, raw.count)
            }
        }
        var out = [UInt8](repeating: 0, count: 64)
        out.withUnsafeMutableBufferPointer { buf in
            blake3_hasher_finalize(&hasher, buf.baseAddress!, 64)
        }
        return out
    }
}
