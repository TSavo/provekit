// SPDX-License-Identifier: Apache-2.0
//
// Mint signed ClaimEnvelopes (property + bridge variants) — pure C++.
//
// Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
//       protocol/specs/2026-04-30-memento-envelope-grammar.md
//
// Output: signed canonical bytes (JCS) + envelope CID.
// CID rule: cid = sha256(JCS(envelope minus cid minus producerSignature))[:32 hex chars]
// Signature: ed25519 over the same canonical bytes the CID hashes.

#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#include "../canonicalizer/value.hpp"
#include "../proof-envelope/sign_ed25519.hpp"

namespace provekit::claim_envelope {

struct MintedEnvelope {
    std::vector<uint8_t> canonical_bytes;  // JCS bytes of signed envelope
    std::string cid;                        // 32 lowercase hex chars
};

struct MintPropertyArgs {
    std::string binding_hash;        // 16 lowercase hex chars
    std::string property_hash;       // 16 lowercase hex chars
    std::string produced_by;
    std::string produced_at;          // ISO 8601
    std::vector<std::string> input_cids;
    ::provekit::canonicalizer::ValuePtr ir_formula;  // Value tree
    ::provekit::canonicalizer::ValuePtr scope;        // Value tree (e.g. {kind:"function", name:"..."})
    std::string ir_kit_version;       // e.g. "cpp-kit@1.0"
    ::provekit::proof_envelope::Ed25519Seed signer_seed;
};

MintedEnvelope mint_property(const MintPropertyArgs& args);

struct MintBridgeArgs {
    std::string binding_hash;
    std::string property_hash;
    std::string produced_by;
    std::string produced_at;
    std::string source_symbol;
    std::string source_layer;
    std::string target_contract_cid;  // CID of the property memento this bridges to
    std::string target_layer;
    std::vector<std::string> ir_arg_sorts;  // each a SortRef as a string
    std::string ir_return_sort;
    std::string notes;                       // optional, "" → omitted
    ::provekit::proof_envelope::Ed25519Seed signer_seed;
};

MintedEnvelope mint_bridge(const MintBridgeArgs& args);

}  // namespace provekit::claim_envelope
