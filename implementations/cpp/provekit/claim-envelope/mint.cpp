// SPDX-License-Identifier: Apache-2.0

#include "mint.hpp"

#include "../canonicalizer/hash.hpp"
#include "../canonicalizer/jcs.hpp"

#include <algorithm>
#include <stdexcept>
#include <string>
#include <utility>

namespace provekit::claim_envelope {

using ::provekit::canonicalizer::Value;
using ::provekit::canonicalizer::ValuePtr;

namespace {

// Schema CIDs are placeholders for the v1.1.0 demo; they are valid
// self-identifying hashes ("blake3-512:" + 128 hex chars) so they pass
// the memento envelope grammar's regex even though the bytes do not
// resolve to a real schema memento yet. A future cut substitutes the
// real schema-memento CIDs.
constexpr const char* SCHEMA_CID_CONTRACT =
    "blake3-512:00000000000000000000000000000000000000000000000000000000000000d0"
                "00000000000000000000000000000000000000000000000000000000000000d0";
constexpr const char* SCHEMA_CID_BRIDGE =
    "blake3-512:00000000000000000000000000000000000000000000000000000000000000c0"
                "00000000000000000000000000000000000000000000000000000000000000c0";
constexpr const char* SCHEMA_CID_IMPLICATION =
    "blake3-512:00000000000000000000000000000000000000000000000000000000000000e0"
                "00000000000000000000000000000000000000000000000000000000000000e0";

std::string base64_encode(const uint8_t* data, size_t len) {
    static constexpr char A[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    out.reserve(((len + 2) / 3) * 4);
    size_t i = 0;
    while (i + 2 < len) {
        uint32_t n = (uint32_t(data[i]) << 16) | (uint32_t(data[i + 1]) << 8) | uint32_t(data[i + 2]);
        out.push_back(A[(n >> 18) & 0x3F]);
        out.push_back(A[(n >> 12) & 0x3F]);
        out.push_back(A[(n >> 6) & 0x3F]);
        out.push_back(A[n & 0x3F]);
        i += 3;
    }
    if (i < len) {
        uint32_t n = uint32_t(data[i]) << 16;
        if (i + 1 < len) n |= uint32_t(data[i + 1]) << 8;
        out.push_back(A[(n >> 18) & 0x3F]);
        out.push_back(A[(n >> 12) & 0x3F]);
        if (i + 1 < len) {
            out.push_back(A[(n >> 6) & 0x3F]);
            out.push_back('=');
        } else {
            out.append("==");
        }
    }
    return out;
}

// hash_value computes the self-identifying CID over the JCS bytes of a
// canonical AST. Per protocol v1.1.0: full BLAKE3-512, no truncation,
// "blake3-512:" prefix.
std::string hash_value(const ValuePtr& v) {
    const std::string canonical = ::provekit::canonicalizer::encode_jcs(*v);
    return ::provekit::canonicalizer::compute_cid(canonical);
}

std::string hash_string(const std::string& s) {
    return ::provekit::canonicalizer::compute_cid(s);
}

ValuePtr build_envelope_for_hashing(
    const std::string& binding_hash,
    const std::string& property_hash,
    const std::string& verdict,
    const std::string& produced_by,
    const std::string& produced_at,
    const std::vector<std::string>& input_cids,
    ValuePtr evidence) {
    std::vector<ValuePtr> input_cids_arr;
    std::vector<std::string> sorted = input_cids;
    std::sort(sorted.begin(), sorted.end());
    for (const auto& c : sorted) input_cids_arr.push_back(Value::string(c));

    return Value::object({
        {"schemaVersion", Value::string("1")},
        {"bindingHash", Value::string(binding_hash)},
        {"propertyHash", Value::string(property_hash)},
        {"verdict", Value::string(verdict)},
        {"producedBy", Value::string(produced_by)},
        {"producedAt", Value::string(produced_at)},
        {"inputCids", Value::array(input_cids_arr)},
        {"evidence", evidence},
    });
}

MintedEnvelope mint_internal(
    const std::string& binding_hash,
    const std::string& property_hash,
    const std::string& verdict,
    const std::string& produced_by,
    const std::string& produced_at,
    const std::vector<std::string>& input_cids,
    ValuePtr evidence,
    const ::provekit::proof_envelope::Ed25519Seed& signer_seed) {
    ValuePtr unsigned_v = build_envelope_for_hashing(
        binding_hash, property_hash, verdict, produced_by, produced_at,
        input_cids, evidence);
    const std::string canonical = ::provekit::canonicalizer::encode_jcs(*unsigned_v);
    // CID = "blake3-512:" + full 128 hex chars (no truncation).
    const std::string cid = ::provekit::canonicalizer::compute_cid(canonical);
    auto sig = ::provekit::proof_envelope::ed25519_sign_with_seed(
        signer_seed,
        reinterpret_cast<const uint8_t*>(canonical.data()),
        canonical.size());
    // Self-identifying signature: "ed25519:" + base64(sig bytes).
    const std::string sig_str =
        std::string("ed25519:") + base64_encode(sig.data(), sig.size());
    std::vector<std::pair<std::string, ValuePtr>> kvs = unsigned_v->as_object();
    kvs.emplace_back("producerSignature", Value::string(sig_str));
    kvs.emplace_back("cid", Value::string(cid));
    ValuePtr signed_v = Value::object(kvs);
    const std::string final_bytes = ::provekit::canonicalizer::encode_jcs(*signed_v);
    std::vector<uint8_t> bytes_vec(final_bytes.begin(), final_bytes.end());
    return {std::move(bytes_vec), cid};
}

ValuePtr build_authoring(const MintContractArgs& args) {
    switch (args.authoring_kind) {
        case AuthoringKind::KitAuthor: {
            std::vector<std::pair<std::string, ValuePtr>> kvs = {
                {"producerKind", Value::string("kit-author")},
                {"author", Value::string(args.authoring_kit_author.author)},
            };
            if (!args.authoring_kit_author.note.empty()) {
                kvs.emplace_back("note", Value::string(args.authoring_kit_author.note));
            }
            return Value::object(kvs);
        }
        case AuthoringKind::Lift: {
            std::vector<std::pair<std::string, ValuePtr>> kvs = {
                {"producerKind", Value::string("lift")},
                {"lifter", Value::string(args.authoring_lift.lifter)},
                {"evidence", Value::string(args.authoring_lift.evidence)},
            };
            if (!args.authoring_lift.source_cid.empty()) {
                kvs.emplace_back("sourceCid", Value::string(args.authoring_lift.source_cid));
            }
            return Value::object(kvs);
        }
        case AuthoringKind::Llm: {
            std::vector<std::pair<std::string, ValuePtr>> kvs = {
                {"producerKind", Value::string("llm")},
                {"llm", Value::string(args.authoring_llm.llm)},
                {"llmVersion", Value::string(args.authoring_llm.llm_version)},
                {"promptCid", Value::string(args.authoring_llm.prompt_cid)},
                // Confidence is a real; encode losslessly as integer percent for v0
                // (the LLM authoring path lifts in a follow-up commit).
                {"confidence", Value::integer(static_cast<int64_t>(args.authoring_llm.confidence * 1000))},
            };
            if (!args.authoring_llm.rationale.empty()) {
                kvs.emplace_back("rationale", Value::string(args.authoring_llm.rationale));
            }
            return Value::object(kvs);
        }
    }
    throw std::runtime_error("mint_contract: unknown authoring kind");
}

}  // namespace

MintedEnvelope mint_contract(const MintContractArgs& args) {
    if (!args.pre && !args.post && !args.inv) {
        throw std::runtime_error(
            "mint_contract: at least one of pre/post/inv must be present");
    }
    if (args.out_binding.empty()) {
        throw std::runtime_error("mint_contract: outBinding is required");
    }

    // Build the contract body. Per spec:
    //   contractName (string)
    //   pre? / post? / inv? (each optional; omit when absent)
    //   outBinding (string)
    //   preHash? / postHash? / invHash? (DERIVED from formulas; omit when absent)
    //   authoring (typed union)
    std::vector<std::pair<std::string, ValuePtr>> body_kvs = {
        {"contractName", Value::string(args.contract_name)},
        {"outBinding", Value::string(args.out_binding)},
    };
    if (args.pre) {
        body_kvs.emplace_back("pre", args.pre);
        body_kvs.emplace_back("preHash", Value::string(hash_value(args.pre)));
    }
    if (args.post) {
        body_kvs.emplace_back("post", args.post);
        body_kvs.emplace_back("postHash", Value::string(hash_value(args.post)));
    }
    if (args.inv) {
        body_kvs.emplace_back("inv", args.inv);
        body_kvs.emplace_back("invHash", Value::string(hash_value(args.inv)));
    }
    body_kvs.emplace_back("authoring", build_authoring(args));
    ValuePtr body = Value::object(body_kvs);

    ValuePtr evidence = Value::object({
        {"kind", Value::string("contract")},
        {"schema", Value::string(SCHEMA_CID_CONTRACT)},
        {"body", body},
    });

    // DERIVED:
    //   propertyHash = hash_value(canonical({pre?, post?, inv?, outBinding}))
    //   bindingHash  = hash_value(canonical({producerId, contractName, propertyHash}))
    std::vector<std::pair<std::string, ValuePtr>> ph_kvs;
    if (args.pre) ph_kvs.emplace_back("pre", args.pre);
    if (args.post) ph_kvs.emplace_back("post", args.post);
    if (args.inv) ph_kvs.emplace_back("inv", args.inv);
    ph_kvs.emplace_back("outBinding", Value::string(args.out_binding));
    const std::string property_hash = hash_value(Value::object(ph_kvs));

    ValuePtr bh_obj = Value::object({
        {"producerId", Value::string(args.produced_by)},
        {"contractName", Value::string(args.contract_name)},
        {"propertyHash", Value::string(property_hash)},
    });
    const std::string binding_hash = hash_value(bh_obj);

    return mint_internal(
        binding_hash, property_hash, "holds",
        args.produced_by, args.produced_at, args.input_cids,
        evidence, args.signer_seed);
}

MintedEnvelope mint_bridge(const MintBridgeArgs& args) {
    std::vector<ValuePtr> arg_sorts;
    arg_sorts.reserve(args.ir_arg_sorts.size());
    for (const auto& s : args.ir_arg_sorts) arg_sorts.push_back(Value::string(s));

    std::vector<std::pair<std::string, ValuePtr>> body_kvs = {
        {"sourceSymbol", Value::string(args.source_symbol)},
        {"sourceLayer", Value::string(args.source_layer)},
        {"targetContractCid", Value::string(args.target_contract_cid)},
        {"targetLayer", Value::string(args.target_layer)},
        {"irArgSorts", Value::array(arg_sorts)},
        {"irReturnSort", Value::string(args.ir_return_sort)},
    };
    if (!args.notes.empty()) {
        body_kvs.emplace_back("notes", Value::string(args.notes));
    }
    ValuePtr body = Value::object(body_kvs);

    ValuePtr evidence = Value::object({
        {"kind", Value::string("bridge")},
        {"schema", Value::string(SCHEMA_CID_BRIDGE)},
        {"body", body},
    });

    // DERIVED per spec:
    //   bindingHash  = hash_value(canonical({sourceLayer, sourceSymbol}))
    //   propertyHash = hash_value(canonical("bridge:" || sourceSymbol))
    ValuePtr bh_obj = Value::object({
        {"sourceLayer", Value::string(args.source_layer)},
        {"sourceSymbol", Value::string(args.source_symbol)},
    });
    const std::string binding_hash = hash_value(bh_obj);
    const std::string property_hash = hash_string("bridge:" + args.source_symbol);

    return mint_internal(
        binding_hash, property_hash, "holds",
        args.produced_by, args.produced_at,
        {args.target_contract_cid},
        evidence, args.signer_seed);
}

MintedEnvelope mint_implication(const MintImplicationArgs& args) {
    std::vector<std::pair<std::string, ValuePtr>> body_kvs = {
        {"antecedentHash", Value::string(args.antecedent_hash)},
        {"consequentHash", Value::string(args.consequent_hash)},
        {"antecedentCid", Value::string(args.antecedent_cid)},
        {"consequentCid", Value::string(args.consequent_cid)},
        {"antecedentSlot", Value::string(args.antecedent_slot)},
        {"consequentSlot", Value::string(args.consequent_slot)},
        {"prover", Value::string(args.prover)},
        {"proverRunMs", Value::integer(static_cast<int64_t>(args.prover_run_ms))},
    };
    if (!args.smt_lib_input.empty()) {
        body_kvs.emplace_back("smtLibInput", Value::string(args.smt_lib_input));
    }
    if (!args.proof_witness.empty()) {
        body_kvs.emplace_back("proofWitness", Value::string(args.proof_witness));
    }
    ValuePtr body = Value::object(body_kvs);

    ValuePtr evidence = Value::object({
        {"kind", Value::string("implication")},
        {"schema", Value::string(SCHEMA_CID_IMPLICATION)},
        {"body", body},
    });

    // DERIVED per spec:
    //   bindingHash  = hash_value(canonical({antecedentHash, consequentHash}))
    //   propertyHash = hash_value(canonical("implication:" || antecedentHash || ":" || consequentHash))
    ValuePtr bh_obj = Value::object({
        {"antecedentHash", Value::string(args.antecedent_hash)},
        {"consequentHash", Value::string(args.consequent_hash)},
    });
    const std::string binding_hash = hash_value(bh_obj);
    const std::string property_hash = hash_string(
        "implication:" + args.antecedent_hash + ":" + args.consequent_hash);

    // inputCids = [antecedentCid, consequentCid] (lex-sorted by mint_internal).
    std::vector<std::string> input_cids = {args.antecedent_cid, args.consequent_cid};

    return mint_internal(
        binding_hash, property_hash, "holds",
        args.produced_by, args.produced_at, input_cids,
        evidence, args.signer_seed);
}

// ── v1.4 BridgeDeclaration (layered envelope/header/body, tagged-union target) ──

static ValuePtr build_target_value(const ::provekit::ir::BridgeTarget& target) {
    const char* kind = ::provekit::ir::target_kind_of(target);
    std::string cid = ::provekit::ir::target_cid_of(target);
    return Value::object({
        {"kind", Value::string(kind)},
        {"cid", Value::string(cid)},
    });
}

MintedEnvelope mint_bridge_v14(const MintBridgeV14Args& args) {
    // Build target
    auto target_v = build_target_value(args.target);

    // Build header (7 canonical fields per spec §1.R3)
    ValuePtr header = Value::object({
        {"schemaVersion", Value::string("1")},
        {"kind", Value::string("bridge")},
        {"name", Value::string(args.name)},
        {"sourceSymbol", Value::string(args.source_symbol)},
        {"sourceLayer", Value::string(args.source_layer)},
        {"sourceContractCid", Value::string(args.source_contract_cid)},
        {"target", target_v},
    });

    // Build metadata (only non-empty fields emitted per §1.R2)
    std::vector<std::pair<std::string, ValuePtr>> meta_kvs;
    auto emit_if_not_empty = [&](const char* key, const std::string& val) {
        if (!val.empty()) meta_kvs.emplace_back(key, Value::string(val));
    };
    emit_if_not_empty("targetWitnessCid", args.target_witness_cid);
    emit_if_not_empty("targetBinaryCid", args.target_binary_cid);
    emit_if_not_empty("targetLayer", args.target_layer);
    emit_if_not_empty("targetContractSetCid", args.target_contract_set_cid);
    emit_if_not_empty("producedBy", args.produced_by);
    emit_if_not_empty("producedAt", args.produced_at);
    ValuePtr metadata = Value::object(meta_kvs);

    // Build envelope
    auto pubkey = ::provekit::proof_envelope::ed25519_pubkey_string_from_seed(args.signer_seed);
    std::vector<std::pair<std::string, ValuePtr>> env_kvs = {
        {"signer", Value::string(pubkey)},
        {"declaredAt", Value::string(args.declared_at)},
    };
    ValuePtr envelope = Value::object(env_kvs);

    // Sign: JCS({header, metadata})
    ValuePtr sig_payload = Value::object({
        {"header", header},
        {"metadata", metadata},
    });
    const std::string sig_payload_str = ::provekit::canonicalizer::encode_jcs(*sig_payload);
    auto sig = ::provekit::proof_envelope::ed25519_sign_with_seed(
        args.signer_seed,
        reinterpret_cast<const uint8_t*>(sig_payload_str.data()),
        sig_payload_str.size());
    const std::string sig_str = std::string("ed25519:") +
        base64_encode(sig.data(), sig.size());
    env_kvs.emplace_back("signature", Value::string(sig_str));
    envelope = Value::object(env_kvs);

    // Full memento: {envelope, header, metadata}
    ValuePtr memento = Value::object({
        {"envelope", envelope},
        {"header", header},
        {"metadata", metadata},
    });
    const std::string canonical = ::provekit::canonicalizer::encode_jcs(*memento);
    std::vector<uint8_t> bytes_vec(canonical.begin(), canonical.end());
    const std::string cid = ::provekit::canonicalizer::compute_cid(canonical);

    return {std::move(bytes_vec), cid};
}

}  // namespace provekit::claim_envelope
