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
    // emit a proof-envelope. The contractSetCid is the canonical
    // BLAKE3-512(JCS(sorted(contractCids))) per
    // protocol/specs/2026-05-03-contract-set-extension.md §1, byte-identical
    // to what the rust/go/cpp/ts kits produce for the same contracts.
    //
    // The .proof bundle format proper (CBOR + signed catalog memento) is
    // Phase 3 deferred for the swift kit; this RPC emits a minimal
    // JCS-JSON catalog whose filename_cid is the BLAKE3-512 of those
    // bytes. This satisfies the rust dispatcher's `proof-envelope`
    // contract (kind, filename_cid non-empty, bytes_base64 decodable) and
    // produces a content-meaningful CID that changes when the slab changes,
    // satisfying acceptance gate #2 of issue #211.

    let contracts = swiftSelfContracts()
    let cids = contracts.map { contractContentCid($0) }
    let setCid = computeContractSetCid(cids)

    // Build the (Phase-3-deferred) catalog body. JCS-encoded; the file's
    // filename CID is BLAKE3-512 of these bytes.
    let catalogJcs = encodeSwiftCatalog(name: "@provekit/swift-self-contracts",
                                        version: "1.0.0",
                                        contracts: contracts,
                                        contractCids: cids)
    let catalogBytes = Data(catalogJcs.utf8)
    let filenameCid = Blake3.hex(catalogBytes)

    let b64 = catalogBytes.base64EncodedString()

    let result: [String: Any] = [
        "kind": "proof-envelope",
        "filename_cid": filenameCid,
        "contract_set_cid": setCid,
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
        return Blake3.hex(Data(Jcs.encode(Jcs.declToValue(d)).utf8))
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
    return Blake3.hex(Data(jcs.utf8))
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
    return Blake3.hex(Data(jcs.utf8))
}

/// Build a minimal JCS-canonical catalog body. The filename_cid of the
/// emitted .proof bundle is BLAKE3-512 of these bytes. NOT byte-equivalent
/// to the rust/go/cpp/ts CBOR-signed catalog format; that's Phase 3 work
/// (per the existing comment in MintSwiftSelfContracts/main.swift line 208).
///
/// Body shape:
///   {
///     contracts: <jcs declarations array>,
///     contractCids: [<sorted contract content CIDs>],
///     declaredAt: <ISO-8601>,
///     kind: "swift-self-contracts-catalog-phase3-pending",
///     name: <catalog name>,
///     version: <catalog version>
///   }
///
/// The presence of `contractCids` and `contracts` makes the bytes change
/// when ANY contract field changes, satisfying issue #211 acceptance gate #2
/// ("Bundle CID is content-meaningful").
func encodeSwiftCatalog(name: String,
                        version: String,
                        contracts: [Declaration],
                        contractCids: [String]) -> String {
    let declsValue: JcsValue = .array(contracts.map(Jcs.declToValue))
    let cidsValue: JcsValue = .array(contractCids.sorted().map { .string($0) })
    let declaredAt = "2026-05-03T18:00:00Z"
    let body: JcsValue = .object([
        ("contractCids", cidsValue),
        ("contracts", declsValue),
        ("declaredAt", .string(declaredAt)),
        ("kind", .string("swift-self-contracts-catalog-phase3-pending")),
        ("name", .string(name)),
        ("version", .string(version)),
    ])
    return Jcs.encode(body)
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
