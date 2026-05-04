// SPDX-License-Identifier: Apache-2.0
//
// v1.4 BridgeDeclaration IR (layered envelope/header/body, tagged-union target).
//
// Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6
// and 2026-05-03-substrate-layers-envelope-header-body.md §1.
//
// Canonical reference: implementations/rust/provekit-claim-envelope/src/lib.rs
//   fn mint_bridge_v14 (line 595).

#pragma once

#include <string>
#include <variant>

namespace provekit::ir {

// ── Tagged-union target per §1.R1 ─────────────────

struct BridgeTargetContract {
    std::string cid;
};

struct BridgeTargetContractSet {
    std::string cid;
};

using BridgeTarget = std::variant<BridgeTargetContract, BridgeTargetContractSet>;

constexpr const char* target_kind_of(const BridgeTarget& t) {
    return std::holds_alternative<BridgeTargetContract>(t) ? "contract" : "contractSet";
}
inline std::string target_cid_of(const BridgeTarget& t) {
    return std::visit([](auto& v) { return v.cid; }, t);
}

// ── Header (7 fields, substrate-verified per §1.R3) ─

struct BridgeHeaderV14 {
    std::string name;
    std::string source_symbol;
    std::string source_layer;
    std::string source_contract_cid;
    BridgeTarget target;

    static constexpr const char* schema_version = "1";
    static constexpr const char* kind = "bridge";
};

// ── Metadata (6 optional axes, empty string = omitted per §1.R2) ─

struct BridgeMetadataV14 {
    std::string target_witness_cid;       // empty = omit
    std::string target_binary_cid;        // empty = omit
    std::string target_layer;             // empty = omit
    std::string target_contract_set_cid;  // empty = omit
    std::string produced_by;              // empty = omit
    std::string produced_at;              // empty = omit
};

}  // namespace provekit::ir

namespace provekit::claim_envelope {

// ── Mint inputs ─────────────────────────────────

struct MintBridgeV14Args {
    // header fields
    std::string name;
    std::string source_symbol;
    std::string source_layer;
    std::string source_contract_cid;
    ::provekit::ir::BridgeTarget target;

    // metadata fields (empty string = omit)
    std::string target_witness_cid;
    std::string target_binary_cid;
    std::string target_layer;
    std::string target_contract_set_cid;
    std::string produced_by;
    std::string produced_at;

    // envelope fields
    std::string declared_at;
    ::provekit::proof_envelope::Ed25519Seed signer_seed;
};

}  // namespace provekit::claim_envelope
