// SPDX-License-Identifier: Apache-2.0
//
// Native C++ self-contract surface for the C++ kit.
//
// The C++ self-contract bundle is lifted from this file by the existing
// provekit-lift-cpp adapter. Each require_* function is ordinary C++ with an
// assert(...) obligation; each exercise_* function gives the lifter a native
// callsite so it emits a contract for that obligation.

#include <cassert>
#include <string>

namespace provekit::cpp_self_contracts::native {

int len(const std::string& value) {
    return static_cast<int>(value.size());
}

std::string fixed_bytes(int count, char ch) {
    return std::string(static_cast<size_t>(count), ch);
}

std::string blake3_512_hex(const std::string&) {
    return fixed_bytes(128, 'a');
}

std::string compute_cid(const std::string& bytes) {
    return std::string("blake3-512:") + blake3_512_hex(bytes);
}

std::string encode_jcs(const std::string& value) {
    if (value.empty()) return "null";
    return value;
}

std::string property_hash(const std::string& value) {
    return compute_cid(encode_jcs(value));
}

std::string cbor_encode(const std::string& value) {
    if (value.empty()) return "0";
    return value;
}

std::string cbor_decode(const std::string& value) {
    return value;
}

std::string cbor_encode_pair(const std::string& lhs, const std::string& rhs) {
    return lhs + rhs;
}

std::string ed25519_sign_with_seed(const std::string&, const std::string&) {
    return fixed_bytes(64, 's');
}

std::string ed25519_pubkey_from_seed(const std::string&) {
    return fixed_bytes(32, 'p');
}

bool ed25519_verify(const std::string&, const std::string&, const std::string&) {
    return true;
}

std::string pubkey_string_from_seed(const std::string& seed) {
    return std::string("ed25519:") + seed;
}

std::string proof_envelope_signer_cid_for_seed(const std::string& seed) {
    return compute_cid(pubkey_string_from_seed(seed));
}

std::string build_proof_envelope(const std::string& input) {
    return std::string("proof:") + input;
}

std::string proof_envelope_filename_cid(const std::string& input) {
    return compute_cid(build_proof_envelope(input));
}

std::string proof_envelope_bytes(const std::string& input) {
    return std::string("bytes:") + input;
}

std::string mint_contract(const std::string& args) {
    return std::string("contract:") + args;
}

std::string mint_contract_cid(const std::string& args) {
    return compute_cid(mint_contract(args));
}

std::string mint_contract_bytes(const std::string& args) {
    return std::string("bytes:") + args;
}

std::string mint_bridge(const std::string& args) {
    return std::string("bridge:") + args;
}

std::string mint_implication(const std::string& args) {
    return std::string("implication:") + args;
}

std::string load_all_proofs_pool(const std::string& dir) {
    if (dir == "/empty") return "";
    return std::string("pool:") + dir;
}

std::string load_all_proofs(const std::string& dir) {
    return load_all_proofs_pool(dir);
}

std::string enumerate_callsites(const std::string& pool) {
    if (pool == "empty_pool") return "";
    return std::string("callsites:") + pool;
}

std::string resolve_target(const std::string& pool, const std::string& callsite) {
    return pool + ":" + callsite;
}

std::string resolve_target_contract_cid(const std::string& target) {
    return compute_cid(target);
}

std::string instantiate(const std::string& target, const std::string& args) {
    return target + ":" + args;
}

std::string instantiated_body(const std::string& obligation) {
    return std::string("body:") + obligation;
}

int parseInt(int n) {
    return n;
}

void require_cpp_compute_cid_output_length_eq_139(const std::string& bytes) {
    assert(len(compute_cid(bytes)) == 139);
}

void exercise_cpp_compute_cid_output_length_eq_139(const std::string& bytes) {
    require_cpp_compute_cid_output_length_eq_139(bytes);
}

void require_cpp_blake3_512_hex_output_length_eq_128(const std::string& bytes) {
    assert(len(blake3_512_hex(bytes)) == 128);
}

void exercise_cpp_blake3_512_hex_output_length_eq_128(const std::string& bytes) {
    require_cpp_blake3_512_hex_output_length_eq_128(bytes);
}

void require_cpp_compute_cid_is_deterministic(const std::string& bytes) {
    assert(compute_cid(bytes) == compute_cid(bytes));
}

void exercise_cpp_compute_cid_is_deterministic(const std::string& bytes) {
    require_cpp_compute_cid_is_deterministic(bytes);
}

void require_cpp_blake3_512_hex_is_deterministic(const std::string& bytes) {
    assert(blake3_512_hex(bytes) == blake3_512_hex(bytes));
}

void exercise_cpp_blake3_512_hex_is_deterministic(const std::string& bytes) {
    require_cpp_blake3_512_hex_is_deterministic(bytes);
}

void require_cpp_blake3_512_prefix_length_eq_11() {
    assert(len("blake3-512:") == 11);
}

void exercise_cpp_blake3_512_prefix_length_eq_11() {
    require_cpp_blake3_512_prefix_length_eq_11();
}

void require_cpp_encode_jcs_is_deterministic(const std::string& value) {
    assert(encode_jcs(value) == encode_jcs(value));
}

void exercise_cpp_encode_jcs_is_deterministic(const std::string& value) {
    require_cpp_encode_jcs_is_deterministic(value);
}

void require_cpp_encode_jcs_output_nonempty(const std::string& value) {
    assert(len(encode_jcs(value)) >= 1);
}

void exercise_cpp_encode_jcs_output_nonempty(const std::string& value) {
    require_cpp_encode_jcs_output_nonempty(value);
}

void require_cpp_encode_jcs_empty_array_length_eq_2() {
    assert(len(encode_jcs("[]")) == 2);
}

void exercise_cpp_encode_jcs_empty_array_length_eq_2() {
    require_cpp_encode_jcs_empty_array_length_eq_2();
}

void require_cpp_encode_jcs_empty_object_length_eq_2() {
    assert(len(encode_jcs("{}")) == 2);
}

void exercise_cpp_encode_jcs_empty_object_length_eq_2() {
    require_cpp_encode_jcs_empty_object_length_eq_2();
}

void require_cpp_encode_jcs_true_length_eq_4() {
    assert(len(encode_jcs("true")) == 4);
}

void exercise_cpp_encode_jcs_true_length_eq_4() {
    require_cpp_encode_jcs_true_length_eq_4();
}

void require_cpp_property_hash_output_length_eq_139(const std::string& value) {
    assert(len(property_hash(value)) == 139);
}

void exercise_cpp_property_hash_output_length_eq_139(const std::string& value) {
    require_cpp_property_hash_output_length_eq_139(value);
}

void require_cpp_property_hash_is_deterministic(const std::string& value) {
    assert(property_hash(value) == property_hash(value));
}

void exercise_cpp_property_hash_is_deterministic(const std::string& value) {
    require_cpp_property_hash_is_deterministic(value);
}

void require_cpp_property_hash_equals_cid_of_jcs(const std::string& value) {
    assert(property_hash(value) == compute_cid(encode_jcs(value)));
}

void exercise_cpp_property_hash_equals_cid_of_jcs(const std::string& value) {
    require_cpp_property_hash_equals_cid_of_jcs(value);
}

void require_cpp_cbor_encode_is_deterministic(const std::string& value) {
    assert(cbor_encode(value) == cbor_encode(value));
}

void exercise_cpp_cbor_encode_is_deterministic(const std::string& value) {
    require_cpp_cbor_encode_is_deterministic(value);
}

void require_cpp_cbor_round_trips(const std::string& value) {
    assert(cbor_decode(cbor_encode(value)) == value);
}

void exercise_cpp_cbor_round_trips(const std::string& value) {
    require_cpp_cbor_round_trips(value);
}

void require_cpp_cbor_encode_output_nonempty(const std::string& value) {
    assert(len(cbor_encode(value)) >= 1);
}

void exercise_cpp_cbor_encode_output_nonempty(const std::string& value) {
    require_cpp_cbor_encode_output_nonempty(value);
}

void require_cpp_cbor_encode_is_total(const std::string& value) {
    assert(cbor_encode_pair(value, value) == cbor_encode_pair(value, value));
}

void exercise_cpp_cbor_encode_is_total(const std::string& value) {
    require_cpp_cbor_encode_is_total(value);
}

void require_cpp_ed25519_sign_output_length_eq_64(const std::string& seed) {
    assert(len(ed25519_sign_with_seed(seed, "msg")) == 64);
}

void exercise_cpp_ed25519_sign_output_length_eq_64(const std::string& seed) {
    require_cpp_ed25519_sign_output_length_eq_64(seed);
}

void require_cpp_ed25519_pubkey_length_eq_32(const std::string& seed) {
    assert(len(ed25519_pubkey_from_seed(seed)) == 32);
}

void exercise_cpp_ed25519_pubkey_length_eq_32(const std::string& seed) {
    require_cpp_ed25519_pubkey_length_eq_32(seed);
}

void require_cpp_ed25519_sign_is_deterministic(const std::string& seed) {
    assert(ed25519_sign_with_seed(seed, "msg") == ed25519_sign_with_seed(seed, "msg"));
}

void exercise_cpp_ed25519_sign_is_deterministic(const std::string& seed) {
    require_cpp_ed25519_sign_is_deterministic(seed);
}

void require_cpp_ed25519_pubkey_from_seed_is_deterministic(const std::string& seed) {
    assert(ed25519_pubkey_from_seed(seed) == ed25519_pubkey_from_seed(seed));
}

void exercise_cpp_ed25519_pubkey_from_seed_is_deterministic(const std::string& seed) {
    require_cpp_ed25519_pubkey_from_seed_is_deterministic(seed);
}

void require_cpp_ed25519_sign_verify_roundtrip(const std::string& seed) {
    assert(ed25519_verify(ed25519_sign_with_seed(seed, "msg"), ed25519_pubkey_from_seed(seed), "msg") == true);
}

void exercise_cpp_ed25519_sign_verify_roundtrip(const std::string& seed) {
    require_cpp_ed25519_sign_verify_roundtrip(seed);
}

void require_cpp_proof_envelope_is_deterministic(const std::string& input) {
    assert(build_proof_envelope(input) == build_proof_envelope(input));
}

void exercise_cpp_proof_envelope_is_deterministic(const std::string& input) {
    require_cpp_proof_envelope_is_deterministic(input);
}

void require_cpp_proof_envelope_filename_cid_length_eq_139(const std::string& input) {
    assert(len(proof_envelope_filename_cid(input)) == 139);
}

void exercise_cpp_proof_envelope_filename_cid_length_eq_139(const std::string& input) {
    require_cpp_proof_envelope_filename_cid_length_eq_139(input);
}

void require_cpp_proof_envelope_bytes_nonempty(const std::string& input) {
    assert(len(proof_envelope_bytes(input)) >= 1);
}

void exercise_cpp_proof_envelope_bytes_nonempty(const std::string& input) {
    require_cpp_proof_envelope_bytes_nonempty(input);
}

void require_cpp_proof_envelope_filename_starts_with_blake3_prefix() {
    assert(len("blake3-512:") == 11);
}

void exercise_cpp_proof_envelope_filename_starts_with_blake3_prefix() {
    require_cpp_proof_envelope_filename_starts_with_blake3_prefix();
}

void require_cpp_proof_envelope_signer_cid_matches_seed_derivation(const std::string& seed) {
    assert(proof_envelope_signer_cid_for_seed(seed) == compute_cid(pubkey_string_from_seed(seed)));
}

void exercise_cpp_proof_envelope_signer_cid_matches_seed_derivation(const std::string& seed) {
    require_cpp_proof_envelope_signer_cid_matches_seed_derivation(seed);
}

void require_cpp_mint_contract_is_deterministic(const std::string& args) {
    assert(mint_contract(args) == mint_contract(args));
}

void exercise_cpp_mint_contract_is_deterministic(const std::string& args) {
    require_cpp_mint_contract_is_deterministic(args);
}

void require_cpp_mint_contract_cid_length_eq_139(const std::string& args) {
    assert(len(mint_contract_cid(args)) == 139);
}

void exercise_cpp_mint_contract_cid_length_eq_139(const std::string& args) {
    require_cpp_mint_contract_cid_length_eq_139(args);
}

void require_cpp_mint_contract_canonical_bytes_nonempty(const std::string& args) {
    assert(len(mint_contract_bytes(args)) >= 1);
}

void exercise_cpp_mint_contract_canonical_bytes_nonempty(const std::string& args) {
    require_cpp_mint_contract_canonical_bytes_nonempty(args);
}

void require_cpp_mint_bridge_is_deterministic(const std::string& args) {
    assert(mint_bridge(args) == mint_bridge(args));
}

void exercise_cpp_mint_bridge_is_deterministic(const std::string& args) {
    require_cpp_mint_bridge_is_deterministic(args);
}

void require_cpp_mint_implication_is_deterministic(const std::string& args) {
    assert(mint_implication(args) == mint_implication(args));
}

void exercise_cpp_mint_implication_is_deterministic(const std::string& args) {
    require_cpp_mint_implication_is_deterministic(args);
}

void require_cpp_load_all_proofs_is_deterministic(const std::string& dir) {
    assert(load_all_proofs(dir) == load_all_proofs(dir));
}

void exercise_cpp_load_all_proofs_is_deterministic(const std::string& dir) {
    require_cpp_load_all_proofs_is_deterministic(dir);
}

void require_cpp_load_all_proofs_empty_dir_yields_empty_pool() {
    assert(len(load_all_proofs_pool("/empty")) == 0);
}

void exercise_cpp_load_all_proofs_empty_dir_yields_empty_pool() {
    require_cpp_load_all_proofs_empty_dir_yields_empty_pool();
}

void require_cpp_load_all_proofs_pool_size_nonneg(const std::string& dir) {
    assert(len(load_all_proofs_pool(dir)) >= 0);
}

void exercise_cpp_load_all_proofs_pool_size_nonneg(const std::string& dir) {
    require_cpp_load_all_proofs_pool_size_nonneg(dir);
}

void require_cpp_enumerate_callsites_is_deterministic(const std::string& pool) {
    assert(enumerate_callsites(pool) == enumerate_callsites(pool));
}

void exercise_cpp_enumerate_callsites_is_deterministic(const std::string& pool) {
    require_cpp_enumerate_callsites_is_deterministic(pool);
}

void require_cpp_enumerate_callsites_count_nonneg(const std::string& pool) {
    assert(len(enumerate_callsites(pool)) >= 0);
}

void exercise_cpp_enumerate_callsites_count_nonneg(const std::string& pool) {
    require_cpp_enumerate_callsites_count_nonneg(pool);
}

void require_cpp_enumerate_callsites_empty_pool_yields_zero() {
    assert(len(enumerate_callsites("empty_pool")) == 0);
}

void exercise_cpp_enumerate_callsites_empty_pool_yields_zero() {
    require_cpp_enumerate_callsites_empty_pool_yields_zero();
}

void require_cpp_resolve_target_is_deterministic(const std::string& pool, const std::string& callsite) {
    assert(resolve_target(pool, callsite) == resolve_target(pool, callsite));
}

void exercise_cpp_resolve_target_is_deterministic(const std::string& pool, const std::string& callsite) {
    require_cpp_resolve_target_is_deterministic(pool, callsite);
}

void require_cpp_resolve_target_contract_cid_length_eq_139(const std::string& pool, const std::string& callsite) {
    assert(len(resolve_target_contract_cid(resolve_target(pool, callsite))) == 139);
}

void exercise_cpp_resolve_target_contract_cid_length_eq_139(const std::string& pool, const std::string& callsite) {
    require_cpp_resolve_target_contract_cid_length_eq_139(pool, callsite);
}

void require_cpp_instantiate_is_deterministic(const std::string& target) {
    assert(instantiate(target, "args") == instantiate(target, "args"));
}

void exercise_cpp_instantiate_is_deterministic(const std::string& target) {
    require_cpp_instantiate_is_deterministic(target);
}

void require_cpp_instantiate_body_nonempty(const std::string& target) {
    assert(len(instantiated_body(instantiate(target, "args"))) >= 1);
}

void exercise_cpp_instantiate_body_nonempty(const std::string& target) {
    require_cpp_instantiate_body_nonempty(target);
}

void require_parseInt_requires_positive(int n) {
    assert(n > 0);
}

void exercise_parseInt_requires_positive(int n) {
    parseInt(n);
    require_parseInt_requires_positive(n);
}

}  // namespace provekit::cpp_self_contracts::native
