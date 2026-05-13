// SPDX-License-Identifier: Apache-2.0
//
// Mint signed ClaimEnvelopes, pure C++. Roles: contract, bridge,
// implication. Per protocol/specs/2026-04-30-memento-envelope-grammar.md
// (catalog v1.1.0).
//
// CID rule: cid = "blake3-512:" + hex(BLAKE3_512(JCS(envelope minus cid
// minus producerSignature))). Full 64-byte digest, 128 hex chars, self-
// identifying tag.
//
// Signature: ed25519 over the same canonical bytes the CID hashes; the
// envelope's `producerSignature` field is "ed25519:" + base64(sig).

#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <variant>
#include <vector>

#include "../canonicalizer/value.hpp"
#include "../proof-envelope/sign_ed25519.hpp"
#include "../../provekit-ir-symbolic/include/provekit/bridge_v14.hpp"

namespace provekit::claim_envelope {

struct MintedEnvelope {
    std::vector<uint8_t> canonical_bytes;  // JCS bytes of the signed envelope
    std::string cid;                        // "blake3-512:" + 128 lowercase hex chars
};

// ---- Authoring metadata (matches the spec's authoring-block union) -------

enum class AuthoringKind { KitAuthor, Llm, Lift };

struct AuthoringKitAuthor {
    std::string author;       // producer-id, e.g. "cpp-kit@1.0"
    std::string note;          // optional, "" → omitted
};

struct AuthoringLift {
    std::string lifter;        // producer-id, e.g. "provekit-lift@1.0"
    std::string evidence;      // "tests" | "types" | "docs" | "symbolic-exec"
    std::string source_cid;    // optional, "" → omitted
};

struct AuthoringLlm {
    std::string llm;
    std::string llm_version;
    std::string prompt_cid;
    double confidence;
    std::string rationale;     // optional, "" → omitted
};

// ---- Contract role ---------------------------------------------------------

struct MintContractArgs {
    std::string contract_name;
    // Each formula is optional; null `ValuePtr` means absent. At least one
    // of pre/post/inv MUST be non-null (mint_contract enforces this).
    ::provekit::canonicalizer::ValuePtr pre;
    ::provekit::canonicalizer::ValuePtr post;
    ::provekit::canonicalizer::ValuePtr inv;
    std::string out_binding;        // e.g. "out"
    std::string produced_by;        // wrapper.producedBy
    std::string produced_at;         // ISO 8601
    std::vector<std::string> input_cids;
    AuthoringKind authoring_kind;
    AuthoringKitAuthor authoring_kit_author;
    AuthoringLift authoring_lift;
    AuthoringLlm authoring_llm;
    ::provekit::proof_envelope::Ed25519Seed signer_seed;
};

MintedEnvelope mint_contract(const MintContractArgs& args);

// ---- Bridge role -----------------------------------------------------------

struct MintBridgeArgs {
    std::string produced_by;
    std::string produced_at;
    std::string source_symbol;
    std::string source_layer;
    std::string target_contract_cid;  // CID of the contract memento this bridges to
    std::string target_layer;
    std::vector<std::string> ir_arg_sorts;
    std::string ir_return_sort;
    std::string notes;                 // optional, "" → omitted
    ::provekit::proof_envelope::Ed25519Seed signer_seed;
};

MintedEnvelope mint_bridge(const MintBridgeArgs& args);

// ── v1.4 BridgeDeclaration (layered envelope/header/body, tagged-union target) ──
MintedEnvelope mint_bridge_v14(const MintBridgeV14Args& args);

// ---- Implication role ------------------------------------------------------

struct MintImplicationArgs {
    std::string produced_by;
    std::string produced_at;
    std::string antecedent_hash;        // "blake3-512:" + 128 hex
    std::string consequent_hash;        // "blake3-512:" + 128 hex
    std::string antecedent_cid;         // contract memento CID
    std::string consequent_cid;         // contract memento CID
    std::string antecedent_slot;        // "pre" | "post" | "inv"
    std::string consequent_slot;        // "pre" | "post" | "inv"
    std::string prover;                  // e.g. "z3@4.13.4"
    uint64_t prover_run_ms;
    std::string smt_lib_input;          // optional, "" → omitted
    std::string proof_witness;          // optional, "" → omitted
    ::provekit::proof_envelope::Ed25519Seed signer_seed;
};

MintedEnvelope mint_implication(const MintImplicationArgs& args);

}  // namespace provekit::claim_envelope
