// SPDX-License-Identifier: Apache-2.0
//
// Cbor: deterministic-CBOR encoder per RFC 8949 §4.2.1 (Core
// Deterministic Encoding).
//
// Mirrors the go reference at
// implementations/go/provekit-ir-symbolic/proof_envelope/cbor.go and the
// rust crate provekit-proof-envelope's `cbor.rs`. Used exclusively by
// the proof envelope catalog encoder. The kit does NOT need (and does
// NOT support) the full CBOR data model; only:
//
//   major 0  unsigned ints (definite, shortest-form)
//   major 2  byte strings  (raw bstr: used for ed25519 signatures + member envelope bodies)
//   major 3  text strings  (UTF-8: used for keys + most values)
//   major 4  arrays        (definite-length)
//   major 5  maps          (definite-length, keys sorted bytewise on CBOR-encoded form)
//
// The protocol's catalog body emits exactly these. No tags, no floats,
// no signed-int negation, no indefinite lengths, no streamed buffers.

import Foundation

public enum CborMajor: UInt8 {
    case uint  = 0
    case bstr  = 2
    case tstr  = 3
    case array = 4
    case map   = 5
}

public struct CborEncoder {
    public private(set) var bytes: Data

    public init() { self.bytes = Data() }

    public var data: Data { bytes }

    /// Reset the buffer so the encoder can be reused.
    public mutating func reset() { bytes.removeAll(keepingCapacity: true) }

    /// Append an "initial byte + argument" head (RFC 8949 §3) using the
    /// shortest of {0..23, uint8, uint16, uint32, uint64} per §4.2.1.
    public mutating func appendHead(_ major: CborMajor, _ arg: UInt64) {
        let mt = UInt8(major.rawValue) << 5
        switch arg {
        case 0..<24:
            bytes.append(mt | UInt8(arg))
        case 24...0xFF:
            bytes.append(mt | 24)
            bytes.append(UInt8(arg))
        case 0x100...0xFFFF:
            bytes.append(mt | 25)
            let v = UInt16(arg).bigEndian
            withUnsafeBytes(of: v) { bytes.append(contentsOf: $0) }
        case 0x10000...0xFFFF_FFFF:
            bytes.append(mt | 26)
            let v = UInt32(arg).bigEndian
            withUnsafeBytes(of: v) { bytes.append(contentsOf: $0) }
        default:
            bytes.append(mt | 27)
            let v = arg.bigEndian
            withUnsafeBytes(of: v) { bytes.append(contentsOf: $0) }
        }
    }

    public mutating func encodeUInt(_ value: UInt64) {
        appendHead(.uint, value)
    }

    /// CBOR byte string (major 2): head + raw bytes.
    public mutating func encodeBStr(_ data: Data) {
        appendHead(.bstr, UInt64(data.count))
        bytes.append(data)
    }

    public mutating func encodeBStr(_ data: [UInt8]) {
        appendHead(.bstr, UInt64(data.count))
        bytes.append(contentsOf: data)
    }

    /// CBOR text string (major 3, UTF-8). The caller is responsible for
    /// the input being valid UTF-8 (Swift `String` always is).
    public mutating func encodeTStr(_ s: String) {
        let utf8 = Array(s.utf8)
        appendHead(.tstr, UInt64(utf8.count))
        bytes.append(contentsOf: utf8)
    }

    /// Definite-length array head; caller appends `count` element bodies.
    public mutating func encodeArrayHead(_ count: UInt64) {
        appendHead(.array, count)
    }

    /// Definite-length map head; caller MUST emit keys in bytewise lex
    /// order of their CBOR-encoded form (RFC 8949 §4.2.1). The
    /// `emitSortedMap` helper handles sorting.
    public mutating func encodeMapHead(_ count: UInt64) {
        appendHead(.map, count)
    }

    /// Append a pre-encoded blob of CBOR bytes (e.g. a value-blob from a
    /// kvPair). Bypasses the encoder's structural state machine: the
    /// caller asserts the blob is well-formed CBOR.
    public mutating func appendRaw(_ raw: Data) {
        bytes.append(raw)
    }
}

/// One (key, value) pair where each component is already CBOR-encoded
/// to bytes. Used by `emitSortedMap` so the sort key is the bytewise
/// CBOR-encoded-key form (the protocol's deterministic order).
public struct CborKVPair: Equatable {
    public let keyCbor: Data
    public let valCbor: Data

    public init(key: Data, value: Data) {
        self.keyCbor = key
        self.valCbor = value
    }
}

public enum CborHelpers {

    /// CBOR-encode a text-string key into its own buffer.
    public static func encodeKey(_ k: String) -> Data {
        var enc = CborEncoder()
        enc.encodeTStr(k)
        return enc.data
    }

    /// CBOR-encode a text-string value (major 3).
    public static func encodeStringValue(_ s: String) -> Data {
        var enc = CborEncoder()
        enc.encodeTStr(s)
        return enc.data
    }

    /// CBOR-encode a byte-string value (major 2).
    public static func encodeBStrValue(_ data: Data) -> Data {
        var enc = CborEncoder()
        enc.encodeBStr(data)
        return enc.data
    }

    /// Emit a definite-length CBOR map with entries sorted by bytewise
    /// lex order of their CBOR-encoded keys. The output bytes match the
    /// go and rust kit envelopes byte-for-byte.
    public static func emitSortedMap(
        into encoder: inout CborEncoder,
        pairs: [CborKVPair]
    ) {
        let sorted = pairs.sorted { lhs, rhs in
            // Lexicographic byte comparison.
            let l = lhs.keyCbor
            let r = rhs.keyCbor
            let n = min(l.count, r.count)
            for i in 0..<n {
                let li = l[l.startIndex + i]
                let ri = r[r.startIndex + i]
                if li != ri { return li < ri }
            }
            return l.count < r.count
        }
        encoder.encodeMapHead(UInt64(sorted.count))
        for p in sorted {
            encoder.appendRaw(p.keyCbor)
            encoder.appendRaw(p.valCbor)
        }
    }

    /// Encode a `{cid: bstr(envelope)}` map for the catalog `members`
    /// field. Keys CBOR-sorted by §4.2.1.
    public static func encodeMembersMap(_ members: [(String, Data)]) -> Data {
        let pairs = members.map { (cid, body) -> CborKVPair in
            CborKVPair(key: encodeKey(cid), value: encodeBStrValue(body))
        }
        var enc = CborEncoder()
        emitSortedMap(into: &enc, pairs: pairs)
        return enc.data
    }
}
