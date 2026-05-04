// SPDX-License-Identifier: Apache-2.0
//
// CryptoTests — standalone integration test runner for ProvekitCrypto.
//
// Does NOT depend on XCTest or Swift Testing (neither is available on
// CI without full Xcode). Uses the kit-local PASS/FAIL idiom from
// LSPTests / ConformanceRunner. Exits 0 on all pass, 1 on any fail.
//
// Coverage:
//   * BLAKE3-512 reference vectors + cross-kit pinned hashes
//   * RFC 8785 JCS encoder shapes
//   * Deterministic CBOR (RFC 8949 §4.2.1)
//   * Ed25519 sign/verify + Foundation v0 public-key pin
//   * Claim envelope (mintContract / mintBridge) + contractCid +
//     contractSetCid
//   * Proof envelope determinism + members order-independence

import Foundation
import ProvekitCrypto

// MARK: - test harness

nonisolated(unsafe) var passed = 0
nonisolated(unsafe) var failed = 0

func test(_ name: String, block: () -> Bool) {
    if block() {
        print("PASS: \(name)")
        passed += 1
    } else {
        print("FAIL: \(name)")
        failed += 1
    }
}

func eq<T: Equatable>(_ a: T, _ b: T, _ label: String = "") -> Bool {
    if a != b {
        print("  expected: \(b)")
        print("  got:      \(a)")
        if !label.isEmpty { print("  context:  \(label)") }
        return false
    }
    return true
}

// MARK: - Blake3

test("blake3 empty input vector") {
    // BLAKE3 reference vector for input_len=0, first 64 bytes of output.
    let h = Blake3.hex(Data())
    return eq(
        h,
        "blake3-512:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a"
    )
}

test("blake3 conformance pinned eq_atomic JCS hash") {
    // Same hash pinned in ConformanceRunner/main.swift line 45,
    // previously produced via python-blake3 shellout.
    let jcs1 = #"{"args":[{"args":[{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"42"}],"kind":"ctor","name":"parse_int"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":42}],"kind":"atomic","name":"="}"#
    let h = Blake3.hex(Data(jcs1.utf8))
    return eq(
        h,
        "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa"
    )
}

test("blake3 digest length = 64 bytes / 128 hex") {
    return Blake3.digest(Data()).count == 64 && Blake3.hex64(Data()).count == 128
}

test("blake3 determinism") {
    let payload = Data("hello world".utf8)
    return Blake3.hex(payload) == Blake3.hex(payload)
}

// MARK: - JCS

test("jcs empty object") {
    return eq(JcsCanonicalizer.encodeString(.object([])), "{}")
}

test("jcs empty array") {
    return eq(JcsCanonicalizer.encodeString(.array([])), "[]")
}

test("jcs sorted keys") {
    let v: JcsCanonical = .object([
        ("c", .int(3)), ("a", .int(1)), ("b", .int(2)),
    ])
    return eq(JcsCanonicalizer.encodeString(v), #"{"a":1,"b":2,"c":3}"#)
}

test("jcs string escapes") {
    let v: JcsCanonical = .string("a\"b\\c\nd")
    return eq(JcsCanonicalizer.encodeString(v), "\"a\\\"b\\\\c\\nd\"")
}

test("jcs control char escape") {
    let v: JcsCanonical = .string("\u{01}")
    return eq(JcsCanonicalizer.encodeString(v), "\"\\u0001\"")
}

test("jcs nested object") {
    let v: JcsCanonical = .object([
        ("z", .object([("b", .int(2)), ("a", .int(1))])),
        ("a", .array([.int(1), .int(2), .int(3)])),
    ])
    return eq(
        JcsCanonicalizer.encodeString(v),
        #"{"a":[1,2,3],"z":{"a":1,"b":2}}"#
    )
}

test("jcs nulls and bools") {
    let v: JcsCanonical = .object([
        ("a", .null), ("b", .bool(true)), ("c", .bool(false)),
    ])
    return eq(
        JcsCanonicalizer.encodeString(v),
        #"{"a":null,"b":true,"c":false}"#
    )
}

test("jcs negative and zero ints") {
    let v: JcsCanonical = .array([.int(-5), .int(0), .int(42)])
    return eq(JcsCanonicalizer.encodeString(v), "[-5,0,42]")
}

test("jcs computeJcsCid shape") {
    let cid = computeJcsCid(.object([("a", .int(1))]))
    return cid.hasPrefix("blake3-512:")
        && cid.count == "blake3-512:".count + 128
}

// MARK: - CBOR

test("cbor empty map = 0xa0") {
    var enc = CborEncoder()
    enc.encodeMapHead(0)
    return enc.data == Data([0xa0])
}

test("cbor empty array = 0x80") {
    var enc = CborEncoder()
    enc.encodeArrayHead(0)
    return enc.data == Data([0x80])
}

test("cbor short uints (<24)") {
    var enc = CborEncoder()
    enc.encodeUInt(0)
    enc.encodeUInt(23)
    return enc.data == Data([0x00, 0x17])
}

test("cbor uint8 marker (24)") {
    var enc = CborEncoder()
    enc.encodeUInt(24)
    return enc.data == Data([0x18, 0x18])
}

test("cbor uint16 marker (256)") {
    var enc = CborEncoder()
    enc.encodeUInt(256)
    return enc.data == Data([0x19, 0x01, 0x00])
}

test("cbor uint32 marker (0x10000)") {
    var enc = CborEncoder()
    enc.encodeUInt(0x10000)
    return enc.data == Data([0x1a, 0x00, 0x01, 0x00, 0x00])
}

test("cbor text string \"a\"") {
    var enc = CborEncoder()
    enc.encodeTStr("a")
    return enc.data == Data([0x61, 0x61])
}

test("cbor empty byte string") {
    var enc = CborEncoder()
    enc.encodeBStr(Data())
    return enc.data == Data([0x40])
}

test("cbor sorted-map reorders inputs") {
    let pairs = [
        CborKVPair(
            key: CborHelpers.encodeKey("z"),
            value: CborHelpers.encodeStringValue("zv")),
        CborKVPair(
            key: CborHelpers.encodeKey("a"),
            value: CborHelpers.encodeStringValue("av")),
    ]
    var enc = CborEncoder()
    CborHelpers.emitSortedMap(into: &enc, pairs: pairs)
    return enc.data == Data([
        0xa2,
        0x61, 0x61, 0x62, 0x61, 0x76,
        0x61, 0x7a, 0x62, 0x7a, 0x76,
    ])
}

// MARK: - Ed25519

test("ed25519 foundation v0 public key") {
    let pk = Ed25519.publicKeyString(fromSeed: Ed25519.foundationV0Seed)
    return eq(pk, "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=")
}

test("ed25519 sign/verify round-trip") {
    let seed = Ed25519.foundationV0Seed
    let msg = Data("hello provekit".utf8)
    let sig = Ed25519.sign(message: msg, seed: seed)
    if sig.count != 64 { return false }
    let pk = Ed25519.publicKey(fromSeed: seed)
    return Ed25519.verify(message: msg, signature: sig, pubKey: pk)
}

test("ed25519 sign is deterministic") {
    let seed = Ed25519.foundationV0Seed
    let msg = Data("provekit-self-contracts@1.0".utf8)
    return Ed25519.sign(message: msg, seed: seed) ==
        Ed25519.sign(message: msg, seed: seed)
}

test("ed25519 verify rejects tampered signature") {
    let seed = Ed25519.foundationV0Seed
    let pk = Ed25519.publicKey(fromSeed: seed)
    let msg = Data("a".utf8)
    var sig = Ed25519.sign(message: msg, seed: seed)
    sig[0] ^= 0xFF
    return Ed25519.verify(message: msg, signature: sig, pubKey: pk) == false
}

// MARK: - Claim envelope

func sampleContractArgs() -> ContractMintArgs {
    return ContractMintArgs(
        contractName: "demo_contract",
        post: .object([
            ("kind", .string("atomic")),
            ("name", .string("=")),
            ("args", .array([
                .object([("kind", .string("var")), ("name", .string("x"))]),
                .object([
                    ("kind", .string("const")),
                    ("sort", .object([
                        ("kind", .string("primitive")),
                        ("name", .string("Int")),
                    ])),
                    ("value", .int(0)),
                ]),
            ])),
        ]),
        outBinding: "out",
        producedBy: "swift-self-contracts@1.0",
        producedAt: "2026-04-30T18:00:00.000Z",
        authoring: .kitAuthor(author: "swift-self-contracts@1.0", note: nil)
    )
}

test("claim envelope mintContract is byte-deterministic") {
    let minter = ClaimMinter(signerSeed: Ed25519.foundationV0Seed)
    do {
        let m1 = try minter.mintContract(sampleContractArgs())
        let m2 = try minter.mintContract(sampleContractArgs())
        return m1.cid == m2.cid && m1.canonicalBytes == m2.canonicalBytes
            && m1.cid.hasPrefix("blake3-512:")
            && m1.cid.count == "blake3-512:".count + 128
    } catch {
        print("  threw: \(error)")
        return false
    }
}

test("claim envelope mintContract rejects all-nil formulae") {
    let minter = ClaimMinter(signerSeed: Ed25519.foundationV0Seed)
    var args = sampleContractArgs()
    args.pre = nil; args.post = nil; args.inv = nil
    do {
        _ = try minter.mintContract(args)
        return false
    } catch {
        return true
    }
}

test("claim envelope contains cid + producerSignature fields") {
    let minter = ClaimMinter(signerSeed: Ed25519.foundationV0Seed)
    do {
        let m = try minter.mintContract(sampleContractArgs())
        guard let s = String(data: m.canonicalBytes, encoding: .utf8) else {
            return false
        }
        return s.contains(#""cid":"blake3-512:"#)
            && s.contains(#""producerSignature":"ed25519:"#)
    } catch {
        return false
    }
}

test("contractCid is signer-independent") {
    var a = sampleContractArgs()
    var b = sampleContractArgs()
    b.producedBy = "different-producer@9.9"
    b.producedAt = "2099-01-01T00:00:00.000Z"
    if contractCid(fromArgs: a) != contractCid(fromArgs: b) { return false }
    a.post = .string("different")
    return contractCid(fromArgs: a) != contractCid(fromArgs: b)
}

test("contractSetCid is order-independent") {
    let cids = [
        "blake3-512:a" + String(repeating: "0", count: 127),
        "blake3-512:b" + String(repeating: "0", count: 127),
        "blake3-512:c" + String(repeating: "0", count: 127),
    ]
    return computeContractSetCid(cids) == computeContractSetCid(cids.reversed())
}

test("claim envelope mintBridge produces well-formed CID") {
    let minter = ClaimMinter(signerSeed: Ed25519.foundationV0Seed)
    do {
        let m = try minter.mintBridge(BridgeMintArgs(
            producedBy: "swift-self-contracts@1.0",
            producedAt: "2026-04-30T18:00:00.000Z",
            sourceSymbol: "lift_plugin_initialize_protocol_version_match",
            sourceLayer: "rust-kit",
            targetContractCid: "blake3-512:" + String(repeating: "0", count: 128),
            targetLayer: "swift-kit",
            notes: "test bridge"
        ))
        return m.cid.hasPrefix("blake3-512:")
            && m.cid.count == "blake3-512:".count + 128
    } catch {
        return false
    }
}

test("claim envelope mintBridge requires target") {
    let minter = ClaimMinter(signerSeed: Ed25519.foundationV0Seed)
    do {
        _ = try minter.mintBridge(BridgeMintArgs(
            producedBy: "x", producedAt: "y",
            sourceSymbol: "s", sourceLayer: "L",
            targetContractCid: "",
            targetLayer: "T"
        ))
        return false
    } catch {
        return true
    }
}

// MARK: - Proof envelope

func sampleProofInput() -> ProofEnvelopeInput {
    return ProofEnvelopeInput(
        name: "@provekit/swift-test",
        version: "1.0.0",
        members: [
            ("blake3-512:" + String(repeating: "a", count: 128),
             Data("memberA".utf8)),
            ("blake3-512:" + String(repeating: "b", count: 128),
             Data("memberB".utf8)),
        ],
        signerCid: Blake3.hex(Data(
            Ed25519.publicKeyString(fromSeed: Ed25519.foundationV0Seed).utf8
        )),
        signerSeed: Ed25519.foundationV0Seed,
        declaredAt: "2026-04-30T18:00:00.000Z"
    )
}

test("proof envelope is byte-deterministic") {
    let a = ProofEnvelopeBuilder.build(sampleProofInput())
    let b = ProofEnvelopeBuilder.build(sampleProofInput())
    return a.bytes == b.bytes && a.filenameCid == b.filenameCid
        && a.filenameCid.hasPrefix("blake3-512:")
        && a.filenameCid.count == "blake3-512:".count + 128
}

test("proof envelope members order-independent") {
    let cidA = "blake3-512:" + String(repeating: "a", count: 128)
    let cidB = "blake3-512:" + String(repeating: "b", count: 128)
    let aBody = Data("aaa".utf8)
    let bBody = Data("bbb".utf8)
    let inForward = ProofEnvelopeInput(
        name: "@x/y", version: "1.0.0",
        members: [(cidA, aBody), (cidB, bBody)],
        signerCid: "blake3-512:" + String(repeating: "c", count: 128),
        signerSeed: Ed25519.foundationV0Seed,
        declaredAt: "2026-04-30T18:00:00.000Z"
    )
    let inReverse = ProofEnvelopeInput(
        name: "@x/y", version: "1.0.0",
        members: [(cidB, bBody), (cidA, aBody)],
        signerCid: "blake3-512:" + String(repeating: "c", count: 128),
        signerSeed: Ed25519.foundationV0Seed,
        declaredAt: "2026-04-30T18:00:00.000Z"
    )
    let f = ProofEnvelopeBuilder.build(inForward)
    let r = ProofEnvelopeBuilder.build(inReverse)
    return f.bytes == r.bytes && f.filenameCid == r.filenameCid
}

// MARK: - Cross-kit byte-equivalence
//
// Reproduce the canonical-bytes pipeline used by
// tools/foundation-keygen/src/lib.rs::build_self_contracts_message to
// sign the swift kit's self-contracts attestation. The rust signer
// produced the signature pinned in
// .provekit/self-contracts-attestations/swift.json. If the Swift
// implementation here is byte-equivalent (JCS + BLAKE3 + Ed25519 all
// matching), recomputing the signature for the same canonical bytes
// must produce the same 64-byte ed25519 result.

test("cross-kit signature equivalence: swift self-contracts attestation") {
    // Pinned values from .provekit/self-contracts-attestations/swift.json.
    let cid = "" // swift catalog CID is empty in the pinned attestation
    let contractSetCid = "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229"
    let declaredAt = "2026-05-03T18:00:00Z"
    let signerPubkey = Ed25519.publicKeyString(fromSeed: Ed25519.foundationV0Seed)

    // Build message — same field set as build_self_contracts_message
    // in tools/foundation-keygen/src/lib.rs.
    let message: JcsCanonical = .object([
        ("schemaVersion", .string("1")),
        ("kind", .string("self-contracts-attestation")),
        ("lang", .string("swift")),
        ("cid", .string(cid)),
        ("contractSetCid", .string(contractSetCid)),
        ("declaredAt", .string(declaredAt)),
        ("signer", .string(signerPubkey)),
    ])
    let canonicalBytes = JcsCanonicalizer.encode(message)
    let sigStr = Ed25519.signatureString(message: canonicalBytes, seed: Ed25519.foundationV0Seed)

    // Pinned: signature emitted by tools/foundation-keygen (Rust
    // ed25519-dalek backend). Byte-equivalence asserts:
    //   * JCS encoder produces identical bytes;
    //   * Ed25519 produces identical signatures (RFC 8032 deterministic).
    let pinned = "ed25519:rfH8zqKIRJn5wANeE2LUqwsOTrTuicdv4J80eFBbls+y43jXOWJ55wPFm5/ktigYsmnzI3kUe2DHwgmyDIB7AQ=="
    return eq(sigStr, pinned)
}

// MARK: - Summary

print("")
print("=== ProvekitCrypto test summary ===")
print("PASSED: \(passed)")
print("FAILED: \(failed)")
if failed > 0 {
    exit(1)
}
exit(0)
