// SPDX-License-Identifier: Apache-2.0
//
// JcsValue + JcsCanonical: RFC 8785 JSON Canonicalization Scheme.
//
// Mirrors the go reference at
// implementations/go/provekit-ir-symbolic/canonicalizer/encoder.go and the
// rust crate provekit-canonicalizer. UTF-8 output, no whitespace, object
// keys sorted by Unicode code-point order (ASCII keys reduce to byte
// order), minimal string escapes.
//
// The kit-IR-specific JCS encoder lives in `Sources/Provekit/IR.swift`
// (`Jcs.encode`). This module is the GENERAL-PURPOSE encoder used by
// the claim and proof envelope minters: keys/values are arbitrary
// canonicalizable values. The two share neither types nor code paths;
// the IR encoder only canonicalizes IR shapes, this one canonicalizes
// any JCS-shaped tree.

import Foundation

/// JCS-canonicalizable value tree. Mirrors `interface{}` in the go
/// encoder's accepted type set. Object keys are stored as inserted; the
/// emitter sorts at write time.
public indirect enum JcsCanonical: Sendable, Equatable {
    case null
    case bool(Bool)
    case int(Int64)
    case string(String)
    case array([JcsCanonical])
    /// Object pairs in insertion order; the encoder lex-sorts by key
    /// before emit. Allows callers to construct objects without paying
    /// for an immediate sort at every nesting level.
    case object([(String, JcsCanonical)])

    public static func == (lhs: JcsCanonical, rhs: JcsCanonical) -> Bool {
        switch (lhs, rhs) {
        case (.null, .null): return true
        case let (.bool(a), .bool(b)): return a == b
        case let (.int(a), .int(b)): return a == b
        case let (.string(a), .string(b)): return a == b
        case let (.array(a), .array(b)): return a == b
        case let (.object(a), .object(b)):
            return a.count == b.count
                && zip(a, b).allSatisfy { $0.0 == $1.0 && $0.1 == $1.1 }
        default: return false
        }
    }
}

public enum JcsCanonicalizer {

    /// Produce JCS-canonical UTF-8 bytes for `value`. Total order on
    /// keys via Unicode code-point comparison; for the ASCII-only keys
    /// the kit emits, that reduces to byte order.
    public static func encode(_ value: JcsCanonical) -> Data {
        var out = Data()
        write(value, into: &out)
        return out
    }

    /// Convenience: UTF-8 string view of `encode(value)`.
    public static func encodeString(_ value: JcsCanonical) -> String {
        return String(data: encode(value), encoding: .utf8) ?? ""
    }

    private static func write(_ value: JcsCanonical, into out: inout Data) {
        switch value {
        case .null:
            out.append(contentsOf: "null".utf8)
        case .bool(let b):
            out.append(contentsOf: (b ? "true" : "false").utf8)
        case .int(let n):
            out.append(contentsOf: String(n).utf8)
        case .string(let s):
            writeString(s, into: &out)
        case .array(let items):
            out.append(0x5B) // [
            for (i, item) in items.enumerated() {
                if i > 0 { out.append(0x2C) } // ,
                write(item, into: &out)
            }
            out.append(0x5D) // ]
        case .object(let pairs):
            // RFC 8785 §3.2.3: sort by Unicode code-point. For UTF-8
            // strings without surrogates, byte-order on the UTF-8
            // representation is equivalent. Swift's String comparison is
            // canonical-equivalence-aware; we drop to UTF-8 bytes to
            // match the protocol's byte-level expectation.
            let sorted = pairs.sorted { lhs, rhs in
                let l = Array(lhs.0.utf8)
                let r = Array(rhs.0.utf8)
                return l.lexicographicallyPrecedes(r)
            }
            out.append(0x7B) // {
            for (i, pair) in sorted.enumerated() {
                if i > 0 { out.append(0x2C) } // ,
                writeString(pair.0, into: &out)
                out.append(0x3A) // :
                write(pair.1, into: &out)
            }
            out.append(0x7D) // }
        }
    }

    /// RFC 8785 §3.2.2.5 minimal string escape: `"`, `\`, and the
    /// control chars U+0000..U+001F. Everything else passes through as
    /// its UTF-8 bytes verbatim.
    private static func writeString(_ s: String, into out: inout Data) {
        out.append(0x22) // "
        for byte in s.utf8 {
            switch byte {
            case 0x22: // "
                out.append(0x5C); out.append(0x22)
            case 0x5C: // \
                out.append(0x5C); out.append(0x5C)
            case 0x08: // \b
                out.append(0x5C); out.append(0x62)
            case 0x09: // \t
                out.append(0x5C); out.append(0x74)
            case 0x0A: // \n
                out.append(0x5C); out.append(0x6E)
            case 0x0C: // \f
                out.append(0x5C); out.append(0x66)
            case 0x0D: // \r
                out.append(0x5C); out.append(0x72)
            case 0x00...0x1F:
                // \u00XX, lowercase hex per RFC 8785 §3.2.2.5
                out.append(contentsOf: "\\u00".utf8)
                let hi = byte >> 4
                let lo = byte & 0x0F
                out.append(hexDigit(hi))
                out.append(hexDigit(lo))
            default:
                out.append(byte)
            }
        }
        out.append(0x22) // "
    }

    private static func hexDigit(_ n: UInt8) -> UInt8 {
        return n < 10 ? (0x30 + n) : (0x61 + (n - 10))
    }
}

/// Compute the protocol's content-address for `value`.
///
///     ComputeCID(JCS(value)) = "blake3-512:" + hex(BLAKE3_512(JCS(value)))
///
/// Equivalent to go's `canonicalizer.ComputeCID(canonicalizer.EncodeJCS(v))`.
public func computeJcsCid(_ value: JcsCanonical) -> String {
    let bytes = JcsCanonicalizer.encode(value)
    return Blake3.hex(bytes)
}
