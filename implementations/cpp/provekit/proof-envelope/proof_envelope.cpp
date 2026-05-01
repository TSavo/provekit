// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder. Per RFC 8949 §4.2.1 + the .proof spec
// (protocol/specs/2026-04-30-proof-file-format.md):
//   1. Build the unsigned body as a CBOR map with sorted keys
//      (sorted by bytewise order of CBOR-encoded key).
//   2. ed25519-sign the unsigned-body bytes.
//   3. Re-emit the body with the signature added; the keys are
//      re-sorted by bytewise lex.
//   4. BLAKE3-512 the final bytes; filename CID =
//      "blake3-512:" + 128 lowercase hex chars (full digest, no truncation).

#include "proof_envelope.hpp"

#include "../canonicalizer/hash.hpp"
#include "cbor.hpp"

#include <algorithm>
#include <utility>
#include <vector>

namespace provekit::proof_envelope {

namespace {

// Construct a sorted-by-bytewise-CBOR-form pair list.
// Each entry has a key (string) and a fully-cbor-encoded value blob.
struct CborPair {
    std::string key;
    std::vector<uint8_t> value_cbor;
    // Pre-cached: CBOR-encoded form of the key alone (tstr head + bytes).
    std::vector<uint8_t> key_cbor;
};

void cbor_encode_key(std::vector<uint8_t>& out, const std::string& key) {
    cbor_encode_tstr(out, key);
}

// Emit a map by sorting pairs in §4.2.1 order and writing them.
void emit_sorted_map(std::vector<uint8_t>& out, std::vector<CborPair>& pairs) {
    std::sort(pairs.begin(), pairs.end(),
              [](const CborPair& a, const CborPair& b) {
                  return a.key_cbor < b.key_cbor;
              });
    cbor_encode_map_head(out, pairs.size());
    for (const auto& p : pairs) {
        out.insert(out.end(), p.key_cbor.begin(), p.key_cbor.end());
        out.insert(out.end(), p.value_cbor.begin(), p.value_cbor.end());
    }
}

CborPair make_string_pair(const std::string& key, const std::string& value) {
    CborPair p;
    p.key = key;
    cbor_encode_key(p.key_cbor, key);
    cbor_encode_tstr(p.value_cbor, value);
    return p;
}

CborPair make_bytes_pair(const std::string& key, const std::vector<uint8_t>& value) {
    CborPair p;
    p.key = key;
    cbor_encode_key(p.key_cbor, key);
    cbor_encode_bstr(p.value_cbor, value.data(), value.size());
    return p;
}

CborPair make_members_pair(
    const std::string& key,
    const std::map<std::string, std::vector<uint8_t>>& members) {
    CborPair p;
    p.key = key;
    cbor_encode_key(p.key_cbor, key);
    // Encode `members` as a CBOR map { tstr(cid) => bstr(envelope-bytes) }
    // Sort by bytewise CBOR-form of the cid keys (RFC 8949 §4.2.1).
    std::vector<CborPair> member_pairs;
    member_pairs.reserve(members.size());
    for (const auto& [cid, env_bytes] : members) {
        member_pairs.push_back(make_bytes_pair(cid, env_bytes));
    }
    emit_sorted_map(p.value_cbor, member_pairs);
    return p;
}

std::vector<CborPair> body_pairs_unsigned(const ProofEnvelopeInput& in) {
    std::vector<CborPair> pairs;
    pairs.push_back(make_string_pair("kind", "catalog"));
    pairs.push_back(make_string_pair("name", in.name));
    pairs.push_back(make_string_pair("version", in.version));
    pairs.push_back(make_members_pair("members", in.members));
    pairs.push_back(make_string_pair("signer", in.signer_cid));
    pairs.push_back(make_string_pair("declaredAt", in.declared_at));
    return pairs;
}

std::string compute_proof_cid(const std::vector<uint8_t>& bytes) {
    // BLAKE3-512 over the raw CBOR bytes; output is "blake3-512:" + 128 hex.
    std::string view(reinterpret_cast<const char*>(bytes.data()), bytes.size());
    return provekit::canonicalizer::compute_cid(view);
}

}  // namespace

ProofEnvelopeOutput build_proof_envelope(const ProofEnvelopeInput& input) {
    // Step 1: encode unsigned body.
    std::vector<CborPair> unsigned_pairs = body_pairs_unsigned(input);
    std::vector<uint8_t> unsigned_bytes;
    emit_sorted_map(unsigned_bytes, unsigned_pairs);

    // Step 2: ed25519 sign.
    Ed25519Signature sig = ed25519_sign_with_seed(
        input.signer_seed, unsigned_bytes.data(), unsigned_bytes.size());
    std::vector<uint8_t> sig_bytes(sig.begin(), sig.end());

    // Step 3: re-emit with signature added (keys re-sorted automatically
    // by emit_sorted_map per the bytewise-CBOR-form rule).
    std::vector<CborPair> signed_pairs = body_pairs_unsigned(input);
    signed_pairs.push_back(make_bytes_pair("signature", sig_bytes));
    std::vector<uint8_t> final_bytes;
    emit_sorted_map(final_bytes, signed_pairs);

    // Step 4: filename CID = "blake3-512:" + full hex (no truncation).
    std::string cid = compute_proof_cid(final_bytes);
    return {std::move(final_bytes), std::move(cid)};
}

}  // namespace provekit::proof_envelope
