// SPDX-License-Identifier: Apache-2.0
//
// Cross-kit conformance bridges for the lift-plugin protocol (cpp peer).
//
// Phase 2 of the cross-kit bridge work. Phase 1 landed in PR #84:
// implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs
// authored 10 contracts encoding the rules of
// protocol/specs/2026-04-30-lift-plugin-protocol.md (the "lift-plugin-
// protocol spec", v1.2.0 normative). The rust .proof now ships those 10
// contracts as signed mementos with content-addressed CIDs.
//
// This header carries:
//
//   * the 10 lift_plugin_protocol contract names in stable declaration
//     order (insertion order from rust's `lift_plugin_protocol::invariants()`)
//   * the rust memento envelope CID for each (frozen pin, see comment
//     on `rust_contract_cid` for re-extraction)
//   * `build_lift_plugin_protocol_bridges()` returning the 10 BridgeDecls
//     used by the pinned-hash test
//   * `marshal_bridges()` JCS emitter that stitches `write_bridge_decl`
//     into a JSON array (matches go's `MarshalDeclarations` / ts's
//     `canonicalEncode([...bridges])` byte shape)
//
// The matching .cpp file defines the `cross_kit_bridges_invariants()`
// extern-C registrar that authors the 10 cpp counterpart contracts when
// the cpp self-contracts orchestrator runs. The registrar pushes
// counterparts into the kit collector (same path as every other
// .invariant.cpp slab); bridges are not yet wired into the cpp bundle
// because the orchestrator has no bridge-marshal pass — phase-3 work
// will fix that and re-mint the bridges with real target_contract_cid
// values.
//
// targetProofCid is `deferred:phase-3-proof-bundle` because computing
// the cpp lift plugin's binary CID is phase-3 work (binary attestation
// per protocol/specs/2026-05-02-binary-attestation-protocol.md).

#pragma once

#include "provekit/ir.hpp"

#include <sstream>
#include <string>
#include <vector>

namespace provekit::cross_kit_bridges {

// --- Constants shared between the slab and the test ------------------------

inline constexpr const char* kRustKitLayer = "rust-kit";
inline constexpr const char* kCppKitLayer = "cpp-kit";
inline constexpr const char* kDeferredCppLiftBinaryCid =
    "deferred:phase-3-proof-bundle";
inline constexpr const char* kPhase2BridgeNotes =
    "lift-plugin-protocol conformance bridge; phase 2";

// 10 lift_plugin_protocol contract names in the order they're authored
// in the rust slab. Hard-coded so adding/removing a rust contract
// without updating the cpp slab fails loud (the pinned-hash test will
// diverge).
inline const std::vector<std::string>& lift_plugin_protocol_contract_names() {
    static const std::vector<std::string> names = {
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
    };
    return names;
}

// Rust memento envelope CIDs for each lift-plugin-protocol contract.
// Extracted from `cargo run --release -p provekit-self-contracts --bin
// print-lift-plugin-protocol-cids` on the post-PR-#84 quiescent tree.
//
// These are NOT JCS-of-the-formula hashes; they are the CIDs of the
// signed CBOR claim envelopes produced by rust's `mint_contract` under
// the foundation key (signer_seed = [0x42; 32]) with rust's pinned
// produced_at. Peer kits cannot recompute them locally without
// replicating the full sign pipeline, so we pin the values verbatim.
inline std::string rust_contract_cid(const std::string& rust_name) {
    if (rust_name == "lift_plugin_initialize_protocol_version_match") {
        return "blake3-512:95163d00976803c3ef381494a8a940bd862529f7bdfb72aa523bd58359b86d6fce017991658932e3e3dee8b4c60b26066bfa270474b2896c19dd2ec85d4aa47a";
    }
    if (rust_name == "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty") {
        return "blake3-512:1898e2518e96628bbe46704f6f6a90cc57572f3b15bb3f4f6a7d8fef28a8c92e31b33b14f21d4011ed7ad11d4ea09c67c1549cbe1c2bf38e53b7e8cfdb656099";
    }
    if (rust_name == "lift_plugin_initialize_capabilities_ir_version_starts_with_v") {
        return "blake3-512:08d09e6f677e77f5b501a07a5271cebdadb19c48c52375ae9e6edcb699b6515eacdea2d7966497c3b3aca4054340e7222fe97bbbb8f60e2ee62baaec6ef719f0";
    }
    if (rust_name == "lift_plugin_lift_request_surface_is_string") {
        return "blake3-512:bf6ac4f7e481ba1fea26716f9d2e7756c86b1940610e2d9e35a5d6e11faa8993a92cd291f491c4d520e5daf1a54c32aeb492adac5aa8d61d224ca1104adaaf8a";
    }
    if (rust_name == "lift_plugin_lift_request_source_paths_nonempty") {
        return "blake3-512:3f2915b063357c28cd2bd8132279e819424999b21a776824d3db9231ca4acb8fdc02ea6e5a8945e55a1d439fda94d07b365d0d160e4ece94b1012fe064ca7c22";
    }
    if (rust_name == "lift_plugin_lift_request_source_paths_each_nonempty") {
        return "blake3-512:f57621c2ba995cbd13d9d06c4209ad9ecdb6369d1e90d902b90996275dd40a38804c986b77b9f28bdf7eefc2b0f242284d1612a4149f5abb0451097a72f95822";
    }
    if (rust_name == "lift_plugin_lift_request_surface_in_capabilities") {
        return "blake3-512:61c67906e3b2ff0d0a61419436670140009556402b643516c4afb14212c057a080bf6f29a0c4c374fe2eb45f8016ddfc82ed12fae2735c7384a8b56a7597db51";
    }
    if (rust_name == "lift_plugin_lift_response_kind_in_set") {
        return "blake3-512:7642bd5eb5262354921513ee6e01bf70dad917f3467464ad904750685e84d0241ef9b0f40b6e0d66dd73e0d5cc1908e4a0a45d45530dda511e1919786034e2a0";
    }
    if (rust_name == "lift_plugin_lift_response_ir_document_array") {
        return "blake3-512:692df8b67bc3ad69943f5909779f489bdc8173bbb08fd61585bb1b8bc0a2c20c6891ba7b9a2a4e4e3a6e5a4441b1191f4618924783446cb07277879c885cbc20";
    }
    if (rust_name == "lift_plugin_diagnostic_field_is_array") {
        return "blake3-512:ea5dd139fddc9e5ab6cfcb9854de1ce6bbedcccbe7b070c1aef9fbbef3b8579ebf33ff14cdc97013e1f3e1c391964f275a0275b615b8259037b0cb92d0e0dd35";
    }
    return "";  // unknown name -> empty (caller checks)
}

// Counterpart contract name for a given rust contract name.
inline std::string counterpart_contract_name(const std::string& rust_name) {
    return "cpp_" + rust_name;
}

// Bridge declaration name for a given rust contract name.
inline std::string bridge_name(const std::string& rust_name) {
    return "bridge_to_" + rust_name;
}

// Pending placeholder for `target_contract_cid`. The cpp mint
// orchestrator does not yet have a bridge-resolution pass; until phase-3
// work wires bridges into the bundle, the placeholder shape is what the
// bridge bytes hash over. Mirrors go's `pending-go-counterpart:<name>`.
inline std::string pending_target_contract_cid(const std::string& cp_name) {
    return "pending-cpp-counterpart:" + cp_name;
}

// Build the 10 BridgeDecls in stable declaration order. Used by the
// pinned-bytes test in cross_kit_bridges_test.cpp.
inline std::vector<provekit::ir::BridgeDecl>
build_lift_plugin_protocol_bridges() {
    std::vector<provekit::ir::BridgeDecl> out;
    out.reserve(10);
    for (const auto& rust_name : lift_plugin_protocol_contract_names()) {
        const auto cp_name = counterpart_contract_name(rust_name);
        provekit::ir::BridgeDecl b{};
        b.name = bridge_name(rust_name);
        b.source_symbol = rust_name;
        b.source_layer = kRustKitLayer;
        b.source_contract_cid = rust_contract_cid(rust_name);
        b.target_contract_cid = pending_target_contract_cid(cp_name);
        b.target_proof_cid = kDeferredCppLiftBinaryCid;
        b.target_layer = kCppKitLayer;
        b.notes = kPhase2BridgeNotes;
        out.push_back(std::move(b));
    }
    return out;
}

// Marshal a BridgeDecl array as a JSON array using ir::write_bridge_decl
// for each element. JCS-canonical bytes used for the pinned hash test.
inline std::string marshal_bridges(
    const std::vector<provekit::ir::BridgeDecl>& bridges) {
    std::ostringstream out;
    out << "[";
    for (size_t i = 0; i < bridges.size(); i++) {
        if (i > 0) out << ",";
        provekit::ir::write_bridge_decl(out, bridges[i]);
    }
    out << "]";
    return out.str();
}

}  // namespace provekit::cross_kit_bridges
