// SPDX-License-Identifier: Apache-2.0
//
// Java kit-internal invariants. Each slab's contracts describe shape-level
// guarantees of the Java kit public surfaces.
//
// Mirrors the cross-kit pattern (rust .invariant.rs, csharp
// .invariant.cs, cpp .invariant.cpp): IR cannot model collision
// resistance / signature soundness, but it CAN say things like "output
// length is exactly N", "function is deterministic", "self-identifying
// prefix is N chars".
//
// Naming convention: {@code java_<slab>_<predicate>}.
//
// Slab list:
//   blake3            5 contracts
//   jcs               5 contracts
//   ed25519           5 contracts
//   cbor              5 contracts
//   proof_envelope    5 contracts
//   claim_envelope    5 contracts
//   realizer          1 contract

package com.provekit.selfcontracts;

import static com.provekit.selfcontracts.Slab.SORT_STRING;
import static com.provekit.selfcontracts.Slab.atomic;
import static com.provekit.selfcontracts.Slab.ctor;
import static com.provekit.selfcontracts.Slab.eq;
import static com.provekit.selfcontracts.Slab.forall;
import static com.provekit.selfcontracts.Slab.gte;
import static com.provekit.selfcontracts.Slab.num;
import static com.provekit.selfcontracts.Slab.startsWith;
import static com.provekit.selfcontracts.Slab.strConst;

import java.util.ArrayList;
import java.util.List;

import com.provekit.ir.ProofIrInvariants;
import com.provekit.selfcontracts.Slab.AuthoredSlab;
import com.provekit.selfcontracts.Slab.Collector;

public final class JavaKitInvariants {

    private JavaKitInvariants() {}

    /**
     * Author every slab. Slab order is fixed (alphabetical by label) so
     * the per-source diagnostic ordering is stable. The minted bytes
     * are CID-keyed and so do not depend on slab order; this is a
     * cosmetic stability for log diffing only.
     */
    public static List<AuthoredSlab> authorAll() {
        List<AuthoredSlab> out = new ArrayList<>();
        out.add(authorBlake3());
        out.add(authorCbor());
        out.add(authorClaimEnvelope());
        out.add(authorEd25519());
        out.add(authorJcs());
        out.add(authorProofEnvelope());
        out.add(authorRealizer());
        return out;
    }

    // -----------------------------------------------------------------
    // Blake3.java
    // -----------------------------------------------------------------

    private static AuthoredSlab authorBlake3() {
        Collector c = new Collector();

        // blake3_512 output length is exactly 139 (11-char prefix + 128 hex).
        c.must("java_blake3_512_output_length_eq_139",
            forall("b", SORT_STRING, b ->
                eq(ctor("len", ctor("blake3_512", b)), num(139))));

        // blake3_512 is deterministic: same input -> same output.
        c.must("java_blake3_512_is_deterministic",
            forall("b", SORT_STRING, b ->
                eq(ctor("blake3_512", b), ctor("blake3_512", b))));

        // blake3_512 output starts with the literal "blake3-512:".
        c.must("java_blake3_512_starts_with_self_identifying_prefix",
            forall("b", SORT_STRING, b ->
                startsWith(ctor("blake3_512", b), strConst("blake3-512:"))));

        // The PREFIX constant is exactly 11 chars long.
        c.must("java_blake3_prefix_length_eq_11",
            eq(ctor("len", strConst("blake3-512:")), num(11)));

        // Raw digest is exactly 64 bytes.
        c.must("java_blake3_digest_bytes_eq_64",
            forall("b", SORT_STRING, b ->
                eq(ctor("len_bytes", ctor("blake3_digest", b)), num(64))));

        return new AuthoredSlab(
            "blake3",
            "implementations/java/provekit-ir/src/main/java/com/provekit/ir/Blake3.java",
            c.drain());
    }

    // -----------------------------------------------------------------
    // Cbor.java
    // -----------------------------------------------------------------

    private static AuthoredSlab authorCbor() {
        Collector c = new Collector();

        // Encoded uint shape: head byte's major-type bits are 0 (MAJOR_UNSIGNED_INT).
        c.must("java_cbor_encode_uint_major_type_zero",
            forall("n", SORT_STRING, n ->
                eq(ctor("cbor_major_of", ctor("cbor_encode_uint", n)), num(0))));

        // Encoded byte string: major-type bits are 2 (MAJOR_BYTE_STRING).
        c.must("java_cbor_encode_bstr_major_type_two",
            forall("b", SORT_STRING, b ->
                eq(ctor("cbor_major_of", ctor("cbor_encode_bstr", b)), num(2))));

        // Encoded text string: major-type bits are 3 (MAJOR_TEXT_STRING).
        c.must("java_cbor_encode_tstr_major_type_three",
            forall("s", SORT_STRING, s ->
                eq(ctor("cbor_major_of", ctor("cbor_encode_tstr", s)), num(3))));

        // Map head: major-type bits are 5 (MAJOR_MAP).
        c.must("java_cbor_encode_map_head_major_type_five",
            forall("count", SORT_STRING, count ->
                eq(ctor("cbor_major_of", ctor("cbor_encode_map_head", count)), num(5))));

        // Shortest-form: arg < 24 fits in the head byte (no extra arg bytes).
        c.must("java_cbor_shortest_form_under_24_no_extra_bytes",
            forall("n", SORT_STRING, n ->
                eq(ctor("cbor_extra_arg_bytes_of", ctor("cbor_encode_uint", n)), num(0))));

        return new AuthoredSlab(
            "cbor",
            "implementations/java/provekit-claim-envelope/src/main/java/com/provekit/claimenvelope/Cbor.java",
            c.drain());
    }

    // -----------------------------------------------------------------
    // ClaimEnvelope.java
    // -----------------------------------------------------------------

    private static AuthoredSlab authorClaimEnvelope() {
        Collector c = new Collector();

        // contractCid is signer-independent: same args, different seed -> same CID.
        c.must("java_claim_envelope_contract_cid_is_signer_independent",
            forall("args", SORT_STRING, args ->
                eq(ctor("contract_cid_with_seed_a", args),
                   ctor("contract_cid_with_seed_b", args))));

        // mintContract requires at least one of pre/post/inv (rejects all-null).
        c.must("java_claim_envelope_mint_contract_rejects_all_null",
            forall("args", SORT_STRING, args ->
                eq(ctor("mint_with_no_pre_post_inv", args), ctor("rejects", strConst("")))));

        // computeContractSetCid is order-independent: sort-then-hash.
        c.must("java_claim_envelope_contract_set_cid_is_order_independent",
            forall("cids", SORT_STRING, cids ->
                eq(ctor("compute_contract_set_cid", cids),
                   ctor("compute_contract_set_cid_reversed", cids))));

        // Layered schema version is exactly "2".
        c.must("java_claim_envelope_layered_schema_version_is_2",
            eq(ctor("layered_schema_version", strConst("")), strConst("2")));

        // Attestation CID = blake3-512(JCS(envelope-with-signature)).
        c.must("java_claim_envelope_attestation_cid_is_blake3_of_jcs_envelope",
            forall("envelope", SORT_STRING, e ->
                eq(ctor("attestation_cid_of", e),
                   ctor("blake3_512", ctor("jcs_encode", ctor("envelope_with_signature", e))))));

        return new AuthoredSlab(
            "claim_envelope",
            "implementations/java/provekit-claim-envelope/src/main/java/com/provekit/claimenvelope/ClaimEnvelope.java",
            c.drain());
    }

    // -----------------------------------------------------------------
    // Ed25519.java
    // -----------------------------------------------------------------

    private static AuthoredSlab authorEd25519() {
        Collector c = new Collector();

        // SEED_BYTES constant is exactly 32.
        c.must("java_ed25519_seed_bytes_eq_32",
            eq(ctor("ed25519_seed_bytes_const", strConst("")), num(32)));

        // PUBKEY_BYTES constant is exactly 32.
        c.must("java_ed25519_pubkey_bytes_eq_32",
            eq(ctor("ed25519_pubkey_bytes_const", strConst("")), num(32)));

        // SIGNATURE_BYTES constant is exactly 64.
        c.must("java_ed25519_signature_bytes_eq_64",
            eq(ctor("ed25519_signature_bytes_const", strConst("")), num(64)));

        // signWithSeed output length is exactly 64.
        c.must("java_ed25519_sign_with_seed_output_length_eq_64",
            forall("msg", SORT_STRING, msg ->
                eq(ctor("len_bytes", ctor("ed25519_sign_with_seed", msg)), num(64))));

        // signString output starts with "ed25519:".
        c.must("java_ed25519_sign_string_starts_with_self_identifying_prefix",
            forall("msg", SORT_STRING, msg ->
                startsWith(ctor("ed25519_sign_string", msg), strConst("ed25519:"))));

        return new AuthoredSlab(
            "ed25519",
            "implementations/java/provekit-claim-envelope/src/main/java/com/provekit/claimenvelope/Ed25519.java",
            c.drain());
    }

    // -----------------------------------------------------------------
    // Jcs.java
    // -----------------------------------------------------------------

    private static AuthoredSlab authorJcs() {
        Collector c = new Collector();

        // Encoded JCS bytes are non-empty for any non-null value.
        c.must("java_jcs_encode_non_empty",
            forall("v", SORT_STRING, v ->
                gte(ctor("len_bytes", ctor("jcs_encode_utf8", v)), num(1))));

        // JCS encoding is deterministic.
        c.must("java_jcs_encode_is_deterministic",
            forall("v", SORT_STRING, v ->
                eq(ctor("jcs_encode_utf8", v), ctor("jcs_encode_utf8", v))));

        // JCS sorts object keys: re-ordering input -> identical output bytes.
        c.must("java_jcs_object_key_order_independent",
            forall("v", SORT_STRING, v ->
                eq(ctor("jcs_encode_utf8", v),
                   ctor("jcs_encode_utf8", ctor("reorder_object_keys", v)))));

        // U+0000..U+001F encode as \\u00XX (lowercase).
        c.must("java_jcs_control_chars_encode_as_lowercase_uxxxx",
            forall("c", SORT_STRING, ch ->
                eq(ctor("jcs_escape", ch), ctor("backslash_u00", ctor("hex_lower", ch)))));

        // Double-quote escapes to backslash + double-quote.
        c.must("java_jcs_double_quote_escapes_with_backslash",
            eq(ctor("jcs_escape", strConst("\"")), strConst("\\\"")));

        return new AuthoredSlab(
            "jcs",
            "implementations/java/provekit-ir/src/main/java/com/provekit/ir/Jcs.java",
            c.drain());
    }

    // -----------------------------------------------------------------
    // ProofEnvelope.java
    // -----------------------------------------------------------------

    private static AuthoredSlab authorProofEnvelope() {
        Collector c = new Collector();

        // Catalog top-level kind field is exactly the string "catalog".
        c.must("java_proof_envelope_kind_is_literal_catalog",
            eq(ctor("proof_envelope_kind_field", strConst("")), strConst("catalog")));

        // Catalog filename CID = blake3_512(catalog_bytes).
        c.must("java_proof_envelope_filename_cid_is_blake3_of_bytes",
            forall("bytes", SORT_STRING, b ->
                eq(ctor("proof_envelope_filename_cid", b),
                   ctor("blake3_512", b))));

        // Catalog filename CID starts with the self-identifying prefix.
        c.must("java_proof_envelope_filename_cid_starts_with_blake3_prefix",
            forall("bytes", SORT_STRING, b ->
                startsWith(ctor("proof_envelope_filename_cid", b),
                           strConst("blake3-512:"))));

        // CBOR map keys sort by bytewise lex order of CBOR-encoded form.
        c.must("java_proof_envelope_map_keys_sorted_bytewise",
            forall("m", SORT_STRING, m ->
                eq(ctor("emit_sorted_map_bytes", m),
                   ctor("emit_sorted_map_bytes", ctor("permute_pairs", m)))));

        // Verify rejects (false) when expectedCid does not match BLAKE3-512(bytes).
        c.must("java_proof_envelope_verify_rejects_cid_mismatch",
            forall("bytes", SORT_STRING, b ->
                eq(ctor("verify_with_wrong_expected_cid", b),
                   ctor("rejects", strConst("")))));

        return new AuthoredSlab(
            "proof_envelope",
            "implementations/java/provekit-claim-envelope/src/main/java/com/provekit/claimenvelope/ProofEnvelope.java",
            c.drain());
    }

    // -----------------------------------------------------------------
    // Java ProofIR-authored realizer invariants
    // -----------------------------------------------------------------

    private static AuthoredSlab authorRealizer() {
        List<Slab.ContractDecl> contracts = ProofIrInvariants.javaRealizerContracts()
            .stream()
            .map(Slab::fromIrContract)
            .toList();

        return new AuthoredSlab(
            "realizer",
            "implementations/java/provekit-ir/src/main/java/com/provekit/ir/ProofIrInvariants.java",
            contracts);
    }
}
