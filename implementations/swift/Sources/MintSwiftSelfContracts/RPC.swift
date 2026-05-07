// SPDX-License-Identifier: Apache-2.0
//
// RPC.swift — `--rpc` mode for mint-swift-self-contracts.
//
// Speaks the lift-plugin protocol (provekit-lift/1) over NDJSON on stdio.
// Mirrors implementations/typescript/src/bin/mint-ts-self-contracts-rpc.cjs
// (PR #220 daemon-lifecycle pattern: persistent stdin loop, explicit
// `shutdown` response, EOF-on-stdin = graceful exit).
//
// Wire flow:
//   -> initialize
//   <- {name, version, protocol_version, capabilities}
//   -> lift
//   <- {kind:"proof-envelope", filename_cid, contract_set_cid, bytes_base64,
//       diagnostics:[]}
//   -> shutdown
//   <- null  (then process exits)
//
// Why a single file alongside main.swift: SPM compiles every .swift file
// in the target into the same module. The `runRPC()` entry point is
// invoked from main.swift when `--rpc` is on argv.
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md (v1.2.0 normative)

import Foundation
import Provekit
import ProvekitCrypto

// MARK: - RPC entry point

/// Persistent NDJSON stdio loop. Returns when the client sends `shutdown`
/// or stdin closes (EOF). Each line is one JSON-RPC request; each response
/// is one line on stdout.
///
/// Architect rule #3 (issue #176): stdin EOF = graceful shutdown. We exit
/// cleanly without expecting a `shutdown` request.
func runRPC() {
    while let line = readLine(strippingNewline: true) {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { continue }

        guard let data = trimmed.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            // Malformed JSON: ignore per NDJSON convention.
            continue
        }

        let id = parsed["id"]
        let method = parsed["method"] as? String ?? ""

        switch method {
        case "initialize":
            handleInitialize(id: id)
        case "lift":
            handleLift(id: id)
        case "shutdown":
            handleShutdown(id: id)
            return
        case "exit":
            return
        default:
            writeError(id: id, code: -32601, message: "METHOD_NOT_FOUND: \(method)")
        }
    }
    // EOF: graceful exit per architect rule #3.
}

// MARK: - Method handlers

private func handleInitialize(id: Any?) {
    // Per spec lines 64-86 + manifest mirror lines 44-48:
    //   protocol_version: "provekit-lift/1"
    //   capabilities.authoring_surfaces: non-empty array
    //   capabilities.ir_version: starts with "v"
    let result: [String: Any] = [
        "name": "swift-self-contracts",
        "version": "1.0.0",
        "protocol_version": "provekit-lift/1",
        "capabilities": [
            "authoring_surfaces": ["swift-self-contracts"],
            "ir_version": "v1.1.0",
            "emits_signed_mementos": true,
        ],
    ]
    writeResponse(id: id, result: result)
}

private func handleLift(id: Any?) {
    // Walk the slab (the `allContracts` array authored in main.swift) and
    // emit a real signed-CBOR .proof bundle. The contractSetCid is the
    // canonical BLAKE3-512(JCS(sorted(contractCids))) per
    // protocol/specs/2026-05-03-contract-set-extension.md §1, byte-identical
    // to what the Rust verifier re-derives from the loaded member mementos.
    let minted: SwiftSelfContractProof
    do {
        minted = try mintSwiftSelfContractProof(contracts: swiftSelfContracts())
    } catch {
        writeError(id: id, code: -32000, message: "LIFT_FAILED: \(error)")
        return
    }

    let b64 = minted.bytes.base64EncodedString()

    let result: [String: Any] = [
        "kind": "proof-envelope",
        "filename_cid": minted.filenameCid,
        "contract_set_cid": minted.contractSetCid,
        "bytes_base64": b64,
        "diagnostics": [],
    ]
    writeResponse(id: id, result: result)
}

private func handleShutdown(id: Any?) {
    writeResponse(id: id, result: NSNull())
}

// MARK: - Slab + CID computation

/// Compute the signer-independent content CID of one contract per spec
/// `2026-05-03-contract-cid-vs-attestation-cid.md` §1:
///   contractCid = "blake3-512:" + hex(BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?})))
///
/// This is byte-identical to the rust `contract_cid()` in
/// `implementations/rust/provekit-claim-envelope/src/lib.rs:232`.
func contractContentCid(_ d: Declaration) -> String {
    guard case let .contract(name, outBinding, pre, post, inv) = d else {
        // Bridges aren't part of the swift self-contracts slab today;
        // if one shows up we'd fall back to encoding the full declaration.
        return ProvekitCrypto.Blake3.hex(Data(Jcs.encode(Jcs.declToValue(d)).utf8))
    }
    var pairs: [(String, JcsValue)] = [
        ("name", .string(name)),
        ("outBinding", .string(outBinding)),
    ]
    if let pre = pre {
        pairs.append(("pre", Jcs.formulaToValue(pre)))
    }
    if let post = post {
        pairs.append(("post", Jcs.formulaToValue(post)))
    }
    if let inv = inv {
        pairs.append(("inv", Jcs.formulaToValue(inv)))
    }
    let jcs = Jcs.encode(.object(pairs))
    return ProvekitCrypto.Blake3.hex(Data(jcs.utf8))
}

/// Compute the contract set CID per spec `2026-05-03-contract-set-extension.md` §1:
///   contractSetCid = "blake3-512:" + hex(BLAKE3-512(JCS(<sorted contractCids>)))
///
/// Byte-identical to the rust `compute_contract_set_cid()` in
/// `implementations/rust/provekit-claim-envelope/src/lib.rs:265` for the same set.
func computeContractSetCid(_ cids: [String]) -> String {
    let sorted = cids.sorted()
    let arr: [JcsValue] = sorted.map { .string($0) }
    let jcs = Jcs.encode(.array(arr))
    return ProvekitCrypto.Blake3.hex(Data(jcs.utf8))
}

private struct SwiftSelfContractProof {
    let bytes: Data
    let filenameCid: String
    let contractSetCid: String
}

private enum SwiftSelfContractMintError: Error, CustomStringConvertible {
    case nonIntegerNumber(String)
    case noContractMembers

    var description: String {
        switch self {
        case .nonIntegerNumber(let n):
            return "self-contract formula contains non-integer JCS number: \(n)"
        case .noContractMembers:
            return "swift self-contract slab produced no contract members"
        }
    }
}

private let swiftSelfContractCatalogName = "@provekit/swift-self-contracts"
private let swiftSelfContractCatalogVersion = "1.0.0"
private let swiftSelfContractProducedBy = "swift-self-contracts@1.0.0"
private let swiftSelfContractDeclaredAt = "2026-05-03T18:00:00Z"

private func mintSwiftSelfContractProof(contracts: [Declaration]) throws -> SwiftSelfContractProof {
    let minter = ClaimMinter(signerSeed: Ed25519.foundationV0Seed)
    var members: [(String, Data)] = []
    var contractCids: [String] = []

    for declaration in contracts {
        guard case let .contract(name, outBinding, pre, post, inv) = declaration else {
            continue
        }

        let args = ContractMintArgs(
            contractName: name,
            pre: try pre.map { try formulaCanonical($0) },
            post: try post.map { try formulaCanonical($0) },
            inv: try inv.map { try formulaCanonical($0) },
            outBinding: outBinding,
            producedBy: swiftSelfContractProducedBy,
            producedAt: swiftSelfContractDeclaredAt,
            inputCids: [],
            authoring: .kitAuthor(author: swiftSelfContractProducedBy, note: nil)
        )

        contractCids.append(contractCid(fromArgs: args))
        let memento = try minter.mintContract(args)
        members.append((memento.cid, memento.canonicalBytes))
    }

    if members.isEmpty {
        throw SwiftSelfContractMintError.noContractMembers
    }

    let signerPubkey = Ed25519.publicKeyString(fromSeed: Ed25519.foundationV0Seed)
    let signerCid = ProvekitCrypto.Blake3.hex(Data(signerPubkey.utf8))
    let envelope = ProofEnvelopeBuilder.build(ProofEnvelopeInput(
        name: swiftSelfContractCatalogName,
        version: swiftSelfContractCatalogVersion,
        members: members,
        signerCid: signerCid,
        signerSeed: Ed25519.foundationV0Seed,
        declaredAt: swiftSelfContractDeclaredAt
    ))

    return SwiftSelfContractProof(
        bytes: envelope.bytes,
        filenameCid: envelope.filenameCid,
        contractSetCid: ProvekitCrypto.computeContractSetCid(contractCids)
    )
}

private func formulaCanonical(_ formula: Formula) throws -> JcsCanonical {
    return try jcsCanonical(from: Jcs.formulaToValue(formula))
}

private func jcsCanonical(from value: JcsValue) throws -> JcsCanonical {
    switch value {
    case .string(let s):
        return .string(s)
    case .number(let n):
        guard let i = Int64(n) else {
            throw SwiftSelfContractMintError.nonIntegerNumber(n)
        }
        return .int(i)
    case .bool(let b):
        return .bool(b)
    case .null:
        return .null
    case .array(let values):
        return .array(try values.map { try jcsCanonical(from: $0) })
    case .object(let pairs):
        return .object(try pairs.map { (key, nested) in
            (key, try jcsCanonical(from: nested))
        })
    }
}

// MARK: - JSON-RPC writers

private func writeResponse(id: Any?, result: Any) {
    let response: [String: Any] = [
        "jsonrpc": "2.0",
        "id": id ?? NSNull(),
        "result": result,
    ]
    writeJsonLine(response)
}

private func writeError(id: Any?, code: Int, message: String) {
    let response: [String: Any] = [
        "jsonrpc": "2.0",
        "id": id ?? NSNull(),
        "error": [
            "code": code,
            "message": message,
        ],
    ]
    writeJsonLine(response)
}

private func writeJsonLine(_ obj: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: obj, options: []),
          let s = String(data: data, encoding: .utf8) else {
        return
    }
    print(s)
    fflush(stdout)
}
