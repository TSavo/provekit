// SPDX-License-Identifier: Apache-2.0
//
// Mint signed ClaimEnvelopes — pure C++.

#include "mint.hpp"

#include "../canonicalizer/jcs.hpp"
#include "../canonicalizer/sha256.hpp"

#include <algorithm>
#include <stdexcept>
#include <string>
#include <utility>

namespace provekit::claim_envelope {

using ::provekit::canonicalizer::Value;
using ::provekit::canonicalizer::ValuePtr;

namespace {

// VARIANT_SCHEMA_CIDS from the TS impl. Keep in sync.
constexpr const char* SCHEMA_CID_PROPERTY = "0000000000000000d0000000000000d0";
constexpr const char* SCHEMA_CID_BRIDGE   = "0000000000000000c0000000000000c0";

// base64 (RFC 4648). Standard alphabet, padded.
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

// canonical-byte input for CID/signature: envelope MINUS cid + producerSignature.
// Per protocol/specs/2026-04-29-universal-claim-envelope.md §CID construction.
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
    // 1. Build canonical-input value (envelope minus cid + producerSignature).
    ValuePtr unsigned_v = build_envelope_for_hashing(
        binding_hash, property_hash, verdict, produced_by, produced_at,
        input_cids, evidence);

    // 2. JCS-encode → bytes.
    const std::string canonical = ::provekit::canonicalizer::encode_jcs(*unsigned_v);

    // 3. CID = sha256(bytes)[:32 hex].
    const std::string sha = ::provekit::canonicalizer::sha256_hex(canonical);
    const std::string cid = sha.substr(0, 32);

    // 4. Sign over the same bytes.
    auto sig = ::provekit::proof_envelope::ed25519_sign_with_seed(
        signer_seed,
        reinterpret_cast<const uint8_t*>(canonical.data()),
        canonical.size());
    const std::string sig_b64 = base64_encode(sig.data(), sig.size());

    // 5. Build the FULL signed envelope = unsigned_v + producerSignature + cid.
    std::vector<std::pair<std::string, ValuePtr>> kvs = unsigned_v->as_object();
    kvs.emplace_back("producerSignature", Value::string(sig_b64));
    kvs.emplace_back("cid", Value::string(cid));
    ValuePtr signed_v = Value::object(kvs);

    // 6. JCS-encode the full envelope as the canonical-bytes representation
    //    used in the .proof catalog members map.
    const std::string final_bytes = ::provekit::canonicalizer::encode_jcs(*signed_v);
    std::vector<uint8_t> bytes_vec(final_bytes.begin(), final_bytes.end());
    return {std::move(bytes_vec), cid};
}

}  // namespace

MintedEnvelope mint_property(const MintPropertyArgs& args) {
    ValuePtr body = Value::object({
        {"irFormula", args.ir_formula},
        {"scope", args.scope},
        {"irKitVersion", Value::string(args.ir_kit_version)},
    });
    ValuePtr evidence = Value::object({
        {"kind", Value::string("property")},
        {"schema", Value::string(SCHEMA_CID_PROPERTY)},
        {"body", body},
    });
    return mint_internal(
        args.binding_hash, args.property_hash, "holds",
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
    return mint_internal(
        args.binding_hash, args.property_hash, "holds",
        args.produced_by, args.produced_at,
        {args.target_contract_cid},
        evidence, args.signer_seed);
}

}  // namespace provekit::claim_envelope
