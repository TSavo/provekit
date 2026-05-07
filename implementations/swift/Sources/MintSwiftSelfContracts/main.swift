// SPDX-License-Identifier: Apache-2.0
//
// MintSwiftSelfContracts — Swift kit self-contracts attestation minter.
//
// Two entry points:
//
//   1. Default (no `--rpc` arg): print the canonical contractSetCid for the
//      11-contract slab to stdout. Used by humans and the make targets.
//
//   2. `--rpc`: speak the lift-plugin protocol (provekit-lift/1) over NDJSON
//      on stdio. The Rust CLI dispatcher (provekit mint --kit=swift) spawns
//      this binary with `--rpc` and exchanges initialize/lift/shutdown.
//      Wire shape mirrors PR #220's typescript-self-contracts RPC server
//      (the recently-merged Side A pattern):
//        -> initialize
//        <- {name, version, protocol_version, capabilities}
//        -> lift
//        <- {kind:"proof-envelope", filename_cid, contract_set_cid, bytes_base64}
//        -> shutdown
//        <- null
//
// The slab itself (the 11 contract declarations) lives in Slab.swift; the
// RPC handlers live in RPC.swift. SPM compiles all .swift files in this
// target into one module so they share the same internal symbols.
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md (v1.2.0 normative)
// Issue: #211 (Side A: real swift slab walker, replacing regex-v0 lifter)
// Reference: PR #220 (ts daemon-lifecycle pattern), PR #217 (cpp Side A wiring).

import Foundation
import Provekit

// --rpc takes over stdin/stdout for the lift-plugin protocol. Detect it on
// argv before doing any non-RPC printing. Mirrors the go and cpp peers'
// `for _, a := range argv { if a == "--rpc" { runRPCMode(); return } }`
// pattern at the top of main().
let argv = CommandLine.arguments
if argv.contains("--rpc") {
    runRPC()
    exit(0)
}

// Default mode: print the canonical contractSetCid for human inspection.
let allContracts = swiftSelfContracts()

// contractSetCid: BLAKE3-512 of the JCS-canonical bytes of the sorted
// per-contract content CIDs. Cross-kit byte-identical to rust/go/cpp/ts
// for the same set of contracts (same JCS encoder, same BLAKE3-512 hash).
let cids = allContracts.map { contractContentCid($0) }
let contractSetCid = computeContractSetCid(cids)

// Human mode is a lightweight inspection path. The `--rpc` mode mints the
// real signed-CBOR proof artifact used by `provekit mint --kit=swift`.
print("catalog CID: swift-kit-bundle:rpc-mode")
print("contractSetCid: \(contractSetCid)")
