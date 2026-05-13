// SPDX-License-Identifier: Apache-2.0
//
// ProofEnvelope: assemble a complete .proof file.
//
// Mirrors the go reference at
// implementations/go/provekit-ir-symbolic/proof_envelope/builder.go and
// the rust crate provekit-proof-envelope's `proof.rs`.
//
// Per protocol/specs/2026-04-30-proof-file-format.md (v1.1.0):
//   1. Build the unsigned body as a CBOR map with deterministic-CBOR-
//      sorted keys (RFC 8949 §4.2.1).
//   2. ed25519-sign the unsigned-body bytes.
//   3. Re-emit with the signature added (keys re-sorted bytewise on
//      their CBOR-encoded form). The `signature` field is RAW 64-byte
//      CBOR bstr (mirrors go and C++; the self-identifying "ed25519:"
//      prefix is for member-envelope `producerSignature` strings, NOT
//      the catalog signature).
//   4. filenameCID := "blake3-512:" + 128 hex (full digest, no truncation).

import Foundation

public struct ProofEnvelopeInput: Sendable {
    public var name: String
    public var version: String
    public var binaryCid: String
    /// `members` is a list of (cid, body-bytes) pairs. The encoder
    /// sorts by CBOR-encoded key so insertion order is irrelevant; we
    /// store as an array of pairs (not a Dictionary) because Swift's
    /// `[String: Data]` would reorder unpredictably and the kit needs
    /// determinism observable from the call site.
    public var members: [(String, Data)]
    public var signerCid: String
    public var signerSeed: [UInt8]
    /// RFC 3339 with millisecond precision and trailing 'Z',
    /// e.g. "2026-04-30T18:00:00.000Z".
    public var declaredAt: String

    public init(
        name: String,
        version: String,
        binaryCid: String = "",
        members: [(String, Data)],
        signerCid: String,
        signerSeed: [UInt8],
        declaredAt: String
    ) {
        precondition(signerSeed.count == 32, "Ed25519 seed must be 32 bytes")
        self.name = name
        self.version = version
        self.binaryCid = binaryCid
        self.members = members
        self.signerCid = signerCid
        self.signerSeed = signerSeed
        self.declaredAt = declaredAt
    }
}

public struct ProofEnvelopeOutput: Sendable {
    public let bytes: Data
    public let filenameCid: String
}

public enum ProofEnvelopeBuilder {

    /// Build the full deterministic-CBOR .proof bytes + filename CID.
    public static func build(_ input: ProofEnvelopeInput) -> ProofEnvelopeOutput {
        let membersCbor = CborHelpers.encodeMembersMap(input.members)

        // 1. Encode the unsigned body.
        let unsignedPairs = bodyPairsUnsigned(input, membersCbor: membersCbor)
        var unsignedEnc = CborEncoder()
        CborHelpers.emitSortedMap(into: &unsignedEnc, pairs: unsignedPairs)
        let unsignedBytes = unsignedEnc.data

        // 2. ed25519 sign over the unsigned bytes (RAW 64-byte sig).
        let sig = Ed25519.sign(message: unsignedBytes, seed: input.signerSeed)

        // 3. Re-emit with the signature added. Re-sort by CBOR-key bytes.
        var signedPairs = unsignedPairs
        signedPairs.append(CborKVPair(
            key: CborHelpers.encodeKey("signature"),
            value: CborHelpers.encodeBStrValue(Data(sig))
        ))
        var finalEnc = CborEncoder()
        CborHelpers.emitSortedMap(into: &finalEnc, pairs: signedPairs)
        let finalBytes = finalEnc.data

        // 4. filenameCID = "blake3-512:" + 128 hex (no truncation).
        let cid = Blake3.hex(finalBytes)
        return ProofEnvelopeOutput(bytes: finalBytes, filenameCid: cid)
    }

    /// `bodyPairsUnsigned`: emit every catalog field except `signature`.
    /// `binaryCid` is appended only when set (mirrors rust's
    /// `Option<String>` skip-when-None); the outer emitSortedMap
    /// re-sorts by CBOR-encoded-key form, so insertion order does not
    /// affect the final bytes.
    private static func bodyPairsUnsigned(
        _ input: ProofEnvelopeInput,
        membersCbor: Data
    ) -> [CborKVPair] {
        var pairs: [CborKVPair] = [
            CborKVPair(
                key: CborHelpers.encodeKey("kind"),
                value: CborHelpers.encodeStringValue("catalog")),
            CborKVPair(
                key: CborHelpers.encodeKey("name"),
                value: CborHelpers.encodeStringValue(input.name)),
            CborKVPair(
                key: CborHelpers.encodeKey("version"),
                value: CborHelpers.encodeStringValue(input.version)),
            CborKVPair(
                key: CborHelpers.encodeKey("members"),
                value: membersCbor),
            CborKVPair(
                key: CborHelpers.encodeKey("signer"),
                value: CborHelpers.encodeStringValue(input.signerCid)),
            CborKVPair(
                key: CborHelpers.encodeKey("declaredAt"),
                value: CborHelpers.encodeStringValue(input.declaredAt)),
        ]
        if !input.binaryCid.isEmpty {
            pairs.append(CborKVPair(
                key: CborHelpers.encodeKey("binaryCid"),
                value: CborHelpers.encodeStringValue(input.binaryCid)))
        }
        return pairs
    }
}
