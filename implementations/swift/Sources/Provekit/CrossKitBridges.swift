// SPDX-License-Identifier: Apache-2.0
//
// Cross-kit conformance bridges (Phase 2 of the cross-kit bridge work).
//
// Phase 1 (PR #84) authored 10 contracts in
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs
// encoding the rules of protocol/specs/2026-04-30-lift-plugin-protocol.md
// (the "lift-plugin-protocol spec", v1.2.0 normative). PR #88 re-signed
// rust's bundle attestation; the rust .proof now ships those 10 contracts
// as signed mementos with content-addressed CIDs.
//
// This file does the SWIFT-side counterpart:
//
//   1. Mints 10 swift counterpart contracts named
//      `swift_<rust_contract_name>_counterpart`. Each counterpart asserts
//      "swift-kit's lift adapter satisfies rust's <contract name>" via an
//      `inv` formula of shape `satisfies("swift-lift-adapter", "<name>")`.
//      Each counterpart's CID is the BLAKE3-512 of the JCS bytes of the
//      single-element `[contract]` array, computed from the swift kit's
//      JCS emitter. (Swift kit does not replicate rust's signed-CBOR mint
//      pipeline, so swift counterpart CIDs and rust contract CIDs are NOT
//      in the same identity space; the bridge wires them by content.)
//
//   2. Mints 10 BridgeDeclaration records named
//      `bridge_to_<rust_contract_name>` linking each rust source contract
//      (by its memento envelope CID extracted from rust's .proof bundle
//      via `cargo run --release -p provekit-self-contracts --bin
//      print-lift-plugin-protocol-cids`) to its swift counterpart contract
//      CID. The bridge's `targetProofCid` is the deferred phase-3
//      placeholder `deferred:phase-3-proof-bundle`. Phase 3 will replace
//      it with the swift lift binary's signed proof CID per
//      protocol/specs/2026-05-02-binary-attestation-protocol.md.
//
// The mixed declarations array is ordered [c0, b0, c1, b1, ..., c9, b9]
// in slab declaration order from rust's lift_plugin_protocol.rs. This
// pairing is the byte-equality contract for cross-kit verification: any
// drift in a rust contract CID, a counterpart formula shape, a bridge
// field, the JCS emitter, or the declaration order surfaces as a hash
// mismatch in the pinned-hash test in ConformanceRunner/main.swift.
//
// Mirrors the python/go/typescript Phase-2 slabs (PRs #89, #92, #93).

import Foundation

public enum CrossKitBridges {

    /// Source layer for Phase-2 bridges: the upstream rust self-contracts kit.
    public static let rustKitLayer = "rust-kit"

    /// Target layer for Phase-2 bridges: this swift kit.
    public static let swiftKitLayer = "swift-kit"

    /// Adapter id used inside the counterpart's `satisfies` claim.
    public static let swiftAdapterId = "swift-lift-adapter"

    /// Phase-3 placeholder for the swift lift binary's .proof bundle CID.
    /// Phase 3 will replace this with a real CID per
    /// protocol/specs/2026-05-02-binary-attestation-protocol.md.
    public static let deferredProofCid = "deferred:phase-3-proof-bundle"

    /// Notes attached to every Phase-2 bridge declaration.
    public static let phase2BridgeNotes =
        "lift-plugin-protocol conformance bridge; phase 2"

    /// The 10 lift-plugin-protocol rust contract names in slab declaration
    /// order from rust's lift_plugin_protocol.rs.
    public static let liftPluginProtocolNames: [String] = [
        "lift_plugin_initialize_protocol_version_match",
        "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
        "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
        "lift_plugin_lift_request_surface_is_string",
        "lift_plugin_lift_request_source_paths_nonempty",
        "lift_plugin_lift_request_source_paths_each_nonempty",
        "lift_plugin_lift_request_surface_in_capabilities",
        "lift_plugin_lift_response_kind_in_set",
        "lift_plugin_lift_response_ir_document_array",
        "lift_plugin_diagnostic_field_is_array",
    ]

    /// Rust memento envelope CIDs for each lift-plugin-protocol contract.
    ///
    /// Source: `cargo run --release -p provekit-self-contracts --bin
    /// print-lift-plugin-protocol-cids`. Stable under the rust orchestrator's
    /// pinned producer ("provekit-self-contracts@1.0"), pinned timestamp
    /// ("2026-04-30T18:00:00.000Z"), and pinned signer seed ([0x42; 32]).
    /// Cross-kit-validated against the python/go/typescript pins.
    ///
    /// Drift in any of these invalidates the bridges array pinned hash in
    /// ConformanceRunner/main.swift.
    public static let rustContractCids: [String: String] = [
        "lift_plugin_initialize_protocol_version_match":
            "blake3-512:95163d00976803c3ef381494a8a940bd862529f7bdfb72aa523bd58359b86d6fce017991658932e3e3dee8b4c60b26066bfa270474b2896c19dd2ec85d4aa47a",
        "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty":
            "blake3-512:1898e2518e96628bbe46704f6f6a90cc57572f3b15bb3f4f6a7d8fef28a8c92e31b33b14f21d4011ed7ad11d4ea09c67c1549cbe1c2bf38e53b7e8cfdb656099",
        "lift_plugin_initialize_capabilities_ir_version_starts_with_v":
            "blake3-512:08d09e6f677e77f5b501a07a5271cebdadb19c48c52375ae9e6edcb699b6515eacdea2d7966497c3b3aca4054340e7222fe97bbbb8f60e2ee62baaec6ef719f0",
        "lift_plugin_lift_request_surface_is_string":
            "blake3-512:bf6ac4f7e481ba1fea26716f9d2e7756c86b1940610e2d9e35a5d6e11faa8993a92cd291f491c4d520e5daf1a54c32aeb492adac5aa8d61d224ca1104adaaf8a",
        "lift_plugin_lift_request_source_paths_nonempty":
            "blake3-512:3f2915b063357c28cd2bd8132279e819424999b21a776824d3db9231ca4acb8fdc02ea6e5a8945e55a1d439fda94d07b365d0d160e4ece94b1012fe064ca7c22",
        "lift_plugin_lift_request_source_paths_each_nonempty":
            "blake3-512:f57621c2ba995cbd13d9d06c4209ad9ecdb6369d1e90d902b90996275dd40a38804c986b77b9f28bdf7eefc2b0f242284d1612a4149f5abb0451097a72f95822",
        "lift_plugin_lift_request_surface_in_capabilities":
            "blake3-512:61c67906e3b2ff0d0a61419436670140009556402b643516c4afb14212c057a080bf6f29a0c4c374fe2eb45f8016ddfc82ed12fae2735c7384a8b56a7597db51",
        "lift_plugin_lift_response_kind_in_set":
            "blake3-512:7642bd5eb5262354921513ee6e01bf70dad917f3467464ad904750685e84d0241ef9b0f40b6e0d66dd73e0d5cc1908e4a0a45d45530dda511e1919786034e2a0",
        "lift_plugin_lift_response_ir_document_array":
            "blake3-512:692df8b67bc3ad69943f5909779f489bdc8173bbb08fd61585bb1b8bc0a2c20c6891ba7b9a2a4e4e3a6e5a4441b1191f4618924783446cb07277879c885cbc20",
        "lift_plugin_diagnostic_field_is_array":
            "blake3-512:ea5dd139fddc9e5ab6cfcb9854de1ce6bbedcccbe7b070c1aef9fbbef3b8579ebf33ff14cdc97013e1f3e1c391964f275a0275b615b8259037b0cb92d0e0dd35",
    ]

    /// Counterpart contract name for a given rust contract name.
    public static func counterpartName(_ rustContractName: String) -> String {
        return "swift_\(rustContractName)_counterpart"
    }

    /// Bridge declaration name for a given rust contract name.
    public static func bridgeName(_ rustContractName: String) -> String {
        return "bridge_to_\(rustContractName)"
    }

    /// Build the swift counterpart contract for a named rust contract.
    ///
    /// The IR claim is `inv = satisfies(adapter_id, rust_contract_name)`,
    /// an atomic predicate the verifier resolves through the bridge. Same
    /// shape for all 10 so the BridgeDeclaration carries the only per-
    /// contract variation.
    public static func counterpartContract(_ rustContractName: String) -> Declaration {
        let inv = Formula.atomic(
            name: "satisfies",
            args: [
                Term.str(swiftAdapterId),
                Term.str(rustContractName),
            ]
        )
        return .contract(
            name: counterpartName(rustContractName),
            outBinding: "out",
            pre: nil,
            post: nil,
            inv: inv
        )
    }

    /// JCS-encode a single Declaration and BLAKE3-512 hash it. Used to
    /// compute the counterpart contract's CID for use as a bridge's
    /// `targetContractCid`. The encoded form is the single-element
    /// `[contract]` array, mirroring how every other declaration goes
    /// through the kit's JCS emitter.
    public static func declarationCid(_ d: Declaration) -> String {
        let jcs = Jcs.encodeDeclarations([d])
        return Blake3.hex(Data(jcs.utf8))
    }

    /// Build the bridge declaration linking a rust source contract (by
    /// memento CID) to its swift counterpart contract (by JCS-of-decl CID).
    public static func bridge(
        rustContractName: String,
        targetContractCid: String
    ) -> Declaration {
        guard let sourceCid = rustContractCids[rustContractName] else {
            // Programmer error: caller passed an unknown name. Return a
            // bridge with an empty source CID so the pinned-hash test
            // catches it loudly.
            return .bridge(
                name: bridgeName(rustContractName),
                sourceSymbol: rustContractName,
                sourceLayer: rustKitLayer,
                sourceContractCid: "",
                targetContractCid: targetContractCid,
                targetProofCid: deferredProofCid,
                targetLayer: swiftKitLayer,
                notes: phase2BridgeNotes
            )
        }
        return .bridge(
            name: bridgeName(rustContractName),
            sourceSymbol: rustContractName,
            sourceLayer: rustKitLayer,
            sourceContractCid: sourceCid,
            targetContractCid: targetContractCid,
            targetProofCid: deferredProofCid,
            targetLayer: swiftKitLayer,
            notes: phase2BridgeNotes
        )
    }

    /// Build the full mixed `[counterpart, bridge, counterpart, bridge, ...]`
    /// declarations array. 10 counterparts + 10 bridges = 20 entries, in
    /// rust slab declaration order with each counterpart immediately
    /// followed by its bridge. This is the byte-equality contract for the
    /// pinned-hash test.
    public static func buildAllDeclarations() -> [Declaration] {
        var decls: [Declaration] = []
        decls.reserveCapacity(liftPluginProtocolNames.count * 2)
        for name in liftPluginProtocolNames {
            let cp = counterpartContract(name)
            let cpCid = declarationCid(cp)
            decls.append(cp)
            decls.append(bridge(rustContractName: name, targetContractCid: cpCid))
        }
        return decls
    }

    /// Build only the bridges (without their counterparts). Used by the
    /// pinned-hash test that pins the bridges-array hash specifically.
    public static func buildAllBridges() -> [Declaration] {
        var bridges: [Declaration] = []
        bridges.reserveCapacity(liftPluginProtocolNames.count)
        for name in liftPluginProtocolNames {
            let cp = counterpartContract(name)
            let cpCid = declarationCid(cp)
            bridges.append(bridge(rustContractName: name, targetContractCid: cpCid))
        }
        return bridges
    }
}
