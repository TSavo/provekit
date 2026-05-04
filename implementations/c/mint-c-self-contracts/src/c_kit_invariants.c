/* SPDX-License-Identifier: Apache-2.0 */
/*
 * C kit-internal invariants. Each slab's contracts describe the
 * shape-level guarantees of the public surface in
 * implementations/c/provekit-self-contracts/src/ and the matching
 * include/provekit/self_contracts.h.
 *
 * Mirrors the cross-kit pattern (java JavaKitInvariants.java, csharp
 * .invariant.cs, cpp .invariant.cpp): IR cannot model collision
 * resistance / signature soundness, but it CAN say things like "output
 * length is exactly N", "function is deterministic", "self-identifying
 * prefix is N chars".
 *
 * Naming convention: c_<slab>_<predicate>.
 *
 * Slab list:
 *   blake3            5 contracts
 *   cbor              5 contracts
 *   claim_envelope    5 contracts
 *   ed25519           5 contracts
 *   jcs               5 contracts
 *   proof_envelope    5 contracts
 */

#include "c_kit_invariants.h"
#include "slab.h"

#include <stdlib.h>

/* ----------------------------------------------------------------------- */
/* tiny helpers — reduce repetition without sacrificing clarity            */
/* ----------------------------------------------------------------------- */

/* Build ctor("name", arg). Convenience: most invariants apply a single
 * input through a constructor representing an op (e.g. blake3_512(b)). */
static pksc_value *ctor1(const char *name, pksc_value *arg) {
    pksc_value *args[1] = { arg };
    return mcsc_f_ctor(name, args, 1);
}

/* `b -> eq(len(blake3_512(b)), 139)` style body builders are passed as
 * mcsc_body_fn callbacks. Each closure variant is implemented as a
 * dedicated static function with a `void *` ctx for any literal it
 * needs. C does not have closures, so each forall body is its own
 * named function. The pattern keeps the slab authoring readable. */

/* ============================================================== */
/* Slab: blake3                                                    */
/* ============================================================== */

static pksc_value *body_blake3_len_eq_139(pksc_value *b, void *ctx) {
    (void)ctx;
    /* eq(len(blake3_512(b)), 139) */
    return mcsc_f_eq(
        ctor1("len", ctor1("blake3_512", b)),
        mcsc_f_num(139));
}

static pksc_value *body_blake3_deterministic(pksc_value *b, void *ctx) {
    (void)ctx;
    /* eq(blake3_512(b), blake3_512(b))
     * NOTE: the pksc value tree owns each child uniquely. We need TWO
     * copies of the variable. We can't reuse `b`. We rebuild the rhs
     * from the var name string. */
    pksc_value *b2 = mcsc_f_var("b");
    if (!b2) { pksc_value_free(b); return NULL; }
    return mcsc_f_eq(ctor1("blake3_512", b),
                     ctor1("blake3_512", b2));
}

static pksc_value *body_blake3_starts_with_prefix(pksc_value *b, void *ctx) {
    (void)ctx;
    return mcsc_f_starts_with(
        ctor1("blake3_512", b),
        mcsc_f_str("blake3-512:"));
}

static pksc_value *body_blake3_digest_64(pksc_value *b, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("len_bytes", ctor1("blake3_digest", b)),
        mcsc_f_num(64));
}

static int author_blake3(mcsc_slab_list *out) {
    mcsc_slab *s = mcsc_slab_new(
        "blake3",
        "implementations/c/provekit-self-contracts/src/hash.c");
    if (!s) return -1;

    /* c_blake3_512_output_length_eq_139: forall b. len(blake3_512(b)) == 139 */
    pksc_value *f1 = mcsc_f_forall("b", mcsc_f_sort("String"),
                                   body_blake3_len_eq_139, NULL);
    if (mcsc_slab_must(s, "c_blake3_512_output_length_eq_139", f1) != 0) goto fail;

    /* c_blake3_512_is_deterministic: forall b. blake3_512(b) == blake3_512(b) */
    pksc_value *f2 = mcsc_f_forall("b", mcsc_f_sort("String"),
                                   body_blake3_deterministic, NULL);
    if (mcsc_slab_must(s, "c_blake3_512_is_deterministic", f2) != 0) goto fail;

    /* c_blake3_512_starts_with_self_identifying_prefix */
    pksc_value *f3 = mcsc_f_forall("b", mcsc_f_sort("String"),
                                   body_blake3_starts_with_prefix, NULL);
    if (mcsc_slab_must(s, "c_blake3_512_starts_with_self_identifying_prefix", f3) != 0) goto fail;

    /* c_blake3_prefix_length_eq_11: eq(len("blake3-512:"), 11)  (no forall) */
    pksc_value *f4 = mcsc_f_eq(ctor1("len", mcsc_f_str("blake3-512:")),
                               mcsc_f_num(11));
    if (mcsc_slab_must(s, "c_blake3_prefix_length_eq_11", f4) != 0) goto fail;

    /* c_blake3_digest_bytes_eq_64: forall b. len_bytes(blake3_digest(b)) == 64 */
    pksc_value *f5 = mcsc_f_forall("b", mcsc_f_sort("String"),
                                   body_blake3_digest_64, NULL);
    if (mcsc_slab_must(s, "c_blake3_digest_bytes_eq_64", f5) != 0) goto fail;

    return mcsc_slab_list_push(out, s);
fail:
    mcsc_slab_free(s);
    return -1;
}

/* ============================================================== */
/* Slab: cbor                                                      */
/* ============================================================== */

static pksc_value *body_cbor_uint_major_zero(pksc_value *n, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("cbor_major_of", ctor1("cbor_encode_uint", n)),
        mcsc_f_num(0));
}
static pksc_value *body_cbor_bstr_major_two(pksc_value *b, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("cbor_major_of", ctor1("cbor_encode_bstr", b)),
        mcsc_f_num(2));
}
static pksc_value *body_cbor_tstr_major_three(pksc_value *s, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("cbor_major_of", ctor1("cbor_encode_tstr", s)),
        mcsc_f_num(3));
}
static pksc_value *body_cbor_map_major_five(pksc_value *count, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("cbor_major_of", ctor1("cbor_encode_map_head", count)),
        mcsc_f_num(5));
}
static pksc_value *body_cbor_shortest_form(pksc_value *n, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("cbor_extra_arg_bytes_of", ctor1("cbor_encode_uint", n)),
        mcsc_f_num(0));
}

static int author_cbor(mcsc_slab_list *out) {
    mcsc_slab *s = mcsc_slab_new(
        "cbor",
        "implementations/c/provekit-self-contracts/src/cbor.c");
    if (!s) return -1;

    pksc_value *f;
    f = mcsc_f_forall("n", mcsc_f_sort("String"), body_cbor_uint_major_zero, NULL);
    if (mcsc_slab_must(s, "c_cbor_encode_uint_major_type_zero", f) != 0) goto fail;
    f = mcsc_f_forall("b", mcsc_f_sort("String"), body_cbor_bstr_major_two, NULL);
    if (mcsc_slab_must(s, "c_cbor_encode_bstr_major_type_two", f) != 0) goto fail;
    f = mcsc_f_forall("s", mcsc_f_sort("String"), body_cbor_tstr_major_three, NULL);
    if (mcsc_slab_must(s, "c_cbor_encode_tstr_major_type_three", f) != 0) goto fail;
    f = mcsc_f_forall("count", mcsc_f_sort("String"), body_cbor_map_major_five, NULL);
    if (mcsc_slab_must(s, "c_cbor_encode_map_head_major_type_five", f) != 0) goto fail;
    f = mcsc_f_forall("n", mcsc_f_sort("String"), body_cbor_shortest_form, NULL);
    if (mcsc_slab_must(s, "c_cbor_shortest_form_under_24_no_extra_bytes", f) != 0) goto fail;

    return mcsc_slab_list_push(out, s);
fail:
    mcsc_slab_free(s);
    return -1;
}

/* ============================================================== */
/* Slab: claim_envelope                                            */
/* ============================================================== */

static pksc_value *body_claim_signer_independent(pksc_value *args, void *ctx) {
    (void)ctx;
    pksc_value *args_b = mcsc_f_var("args");
    if (!args_b) { pksc_value_free(args); return NULL; }
    return mcsc_f_eq(
        ctor1("contract_cid_with_seed_a", args),
        ctor1("contract_cid_with_seed_b", args_b));
}

static pksc_value *body_claim_rejects_all_null(pksc_value *args, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("mint_with_no_pre_post_inv", args),
        ctor1("rejects", mcsc_f_str("")));
}

static pksc_value *body_claim_set_cid_order_independent(pksc_value *cids, void *ctx) {
    (void)ctx;
    pksc_value *cids_b = mcsc_f_var("cids");
    if (!cids_b) { pksc_value_free(cids); return NULL; }
    return mcsc_f_eq(
        ctor1("compute_contract_set_cid", cids),
        ctor1("compute_contract_set_cid_reversed", cids_b));
}

static pksc_value *body_claim_attestation_cid(pksc_value *e, void *ctx) {
    (void)ctx;
    pksc_value *e_b = mcsc_f_var("envelope");
    if (!e_b) { pksc_value_free(e); return NULL; }
    return mcsc_f_eq(
        ctor1("attestation_cid_of", e),
        ctor1("blake3_512",
              ctor1("jcs_encode",
                    ctor1("envelope_with_signature", e_b))));
}

static int author_claim_envelope(mcsc_slab_list *out) {
    mcsc_slab *s = mcsc_slab_new(
        "claim_envelope",
        "implementations/c/provekit-self-contracts/src/proof_envelope.c");
    if (!s) return -1;

    pksc_value *f;
    f = mcsc_f_forall("args", mcsc_f_sort("String"), body_claim_signer_independent, NULL);
    if (mcsc_slab_must(s, "c_claim_envelope_contract_cid_is_signer_independent", f) != 0) goto fail;

    f = mcsc_f_forall("args", mcsc_f_sort("String"), body_claim_rejects_all_null, NULL);
    if (mcsc_slab_must(s, "c_claim_envelope_mint_contract_rejects_all_null", f) != 0) goto fail;

    f = mcsc_f_forall("cids", mcsc_f_sort("String"), body_claim_set_cid_order_independent, NULL);
    if (mcsc_slab_must(s, "c_claim_envelope_contract_set_cid_is_order_independent", f) != 0) goto fail;

    /* Layered schema version is "2". */
    f = mcsc_f_eq(ctor1("layered_schema_version", mcsc_f_str("")),
                  mcsc_f_str("2"));
    if (mcsc_slab_must(s, "c_claim_envelope_layered_schema_version_is_2", f) != 0) goto fail;

    f = mcsc_f_forall("envelope", mcsc_f_sort("String"), body_claim_attestation_cid, NULL);
    if (mcsc_slab_must(s, "c_claim_envelope_attestation_cid_is_blake3_of_jcs_envelope", f) != 0) goto fail;

    return mcsc_slab_list_push(out, s);
fail:
    mcsc_slab_free(s);
    return -1;
}

/* ============================================================== */
/* Slab: ed25519                                                   */
/* ============================================================== */

static pksc_value *body_ed25519_sign_64(pksc_value *msg, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("len_bytes", ctor1("ed25519_sign_with_seed", msg)),
        mcsc_f_num(64));
}

static pksc_value *body_ed25519_sign_string_prefix(pksc_value *msg, void *ctx) {
    (void)ctx;
    return mcsc_f_starts_with(
        ctor1("ed25519_sign_string", msg),
        mcsc_f_str("ed25519:"));
}

static int author_ed25519(mcsc_slab_list *out) {
    mcsc_slab *s = mcsc_slab_new(
        "ed25519",
        "implementations/c/provekit-self-contracts/src/sign.c");
    if (!s) return -1;

    pksc_value *f;
    f = mcsc_f_eq(ctor1("ed25519_seed_bytes_const", mcsc_f_str("")),
                  mcsc_f_num(32));
    if (mcsc_slab_must(s, "c_ed25519_seed_bytes_eq_32", f) != 0) goto fail;

    f = mcsc_f_eq(ctor1("ed25519_pubkey_bytes_const", mcsc_f_str("")),
                  mcsc_f_num(32));
    if (mcsc_slab_must(s, "c_ed25519_pubkey_bytes_eq_32", f) != 0) goto fail;

    f = mcsc_f_eq(ctor1("ed25519_signature_bytes_const", mcsc_f_str("")),
                  mcsc_f_num(64));
    if (mcsc_slab_must(s, "c_ed25519_signature_bytes_eq_64", f) != 0) goto fail;

    f = mcsc_f_forall("msg", mcsc_f_sort("String"), body_ed25519_sign_64, NULL);
    if (mcsc_slab_must(s, "c_ed25519_sign_with_seed_output_length_eq_64", f) != 0) goto fail;

    f = mcsc_f_forall("msg", mcsc_f_sort("String"), body_ed25519_sign_string_prefix, NULL);
    if (mcsc_slab_must(s, "c_ed25519_sign_string_starts_with_self_identifying_prefix", f) != 0) goto fail;

    return mcsc_slab_list_push(out, s);
fail:
    mcsc_slab_free(s);
    return -1;
}

/* ============================================================== */
/* Slab: jcs                                                       */
/* ============================================================== */

static pksc_value *body_jcs_non_empty(pksc_value *v, void *ctx) {
    (void)ctx;
    return mcsc_f_gte(
        ctor1("len_bytes", ctor1("jcs_encode_utf8", v)),
        mcsc_f_num(1));
}

static pksc_value *body_jcs_deterministic(pksc_value *v, void *ctx) {
    (void)ctx;
    pksc_value *v_b = mcsc_f_var("v");
    if (!v_b) { pksc_value_free(v); return NULL; }
    return mcsc_f_eq(
        ctor1("jcs_encode_utf8", v),
        ctor1("jcs_encode_utf8", v_b));
}

static pksc_value *body_jcs_key_order(pksc_value *v, void *ctx) {
    (void)ctx;
    pksc_value *v_b = mcsc_f_var("v");
    if (!v_b) { pksc_value_free(v); return NULL; }
    return mcsc_f_eq(
        ctor1("jcs_encode_utf8", v),
        ctor1("jcs_encode_utf8",
              ctor1("reorder_object_keys", v_b)));
}

static pksc_value *body_jcs_control_chars(pksc_value *ch, void *ctx) {
    (void)ctx;
    pksc_value *ch_b = mcsc_f_var("c");
    if (!ch_b) { pksc_value_free(ch); return NULL; }
    return mcsc_f_eq(
        ctor1("jcs_escape", ch),
        ctor1("backslash_u00",
              ctor1("hex_lower", ch_b)));
}

static int author_jcs(mcsc_slab_list *out) {
    mcsc_slab *s = mcsc_slab_new(
        "jcs",
        "implementations/c/provekit-self-contracts/src/jcs.c");
    if (!s) return -1;

    pksc_value *f;
    f = mcsc_f_forall("v", mcsc_f_sort("String"), body_jcs_non_empty, NULL);
    if (mcsc_slab_must(s, "c_jcs_encode_non_empty", f) != 0) goto fail;

    f = mcsc_f_forall("v", mcsc_f_sort("String"), body_jcs_deterministic, NULL);
    if (mcsc_slab_must(s, "c_jcs_encode_is_deterministic", f) != 0) goto fail;

    f = mcsc_f_forall("v", mcsc_f_sort("String"), body_jcs_key_order, NULL);
    if (mcsc_slab_must(s, "c_jcs_object_key_order_independent", f) != 0) goto fail;

    f = mcsc_f_forall("c", mcsc_f_sort("String"), body_jcs_control_chars, NULL);
    if (mcsc_slab_must(s, "c_jcs_control_chars_encode_as_lowercase_uxxxx", f) != 0) goto fail;

    /* Double-quote escapes to backslash + double-quote. (no forall) */
    f = mcsc_f_eq(ctor1("jcs_escape", mcsc_f_str("\"")),
                  mcsc_f_str("\\\""));
    if (mcsc_slab_must(s, "c_jcs_double_quote_escapes_with_backslash", f) != 0) goto fail;

    return mcsc_slab_list_push(out, s);
fail:
    mcsc_slab_free(s);
    return -1;
}

/* ============================================================== */
/* Slab: proof_envelope                                            */
/* ============================================================== */

static pksc_value *body_proof_filename_blake3(pksc_value *b, void *ctx) {
    (void)ctx;
    pksc_value *b_b = mcsc_f_var("bytes");
    if (!b_b) { pksc_value_free(b); return NULL; }
    return mcsc_f_eq(
        ctor1("proof_envelope_filename_cid", b),
        ctor1("blake3_512", b_b));
}

static pksc_value *body_proof_filename_prefix(pksc_value *b, void *ctx) {
    (void)ctx;
    return mcsc_f_starts_with(
        ctor1("proof_envelope_filename_cid", b),
        mcsc_f_str("blake3-512:"));
}

static pksc_value *body_proof_keys_sorted(pksc_value *m, void *ctx) {
    (void)ctx;
    pksc_value *m_b = mcsc_f_var("m");
    if (!m_b) { pksc_value_free(m); return NULL; }
    return mcsc_f_eq(
        ctor1("emit_sorted_map_bytes", m),
        ctor1("emit_sorted_map_bytes",
              ctor1("permute_pairs", m_b)));
}

static pksc_value *body_proof_verify_rejects(pksc_value *b, void *ctx) {
    (void)ctx;
    return mcsc_f_eq(
        ctor1("verify_with_wrong_expected_cid", b),
        ctor1("rejects", mcsc_f_str("")));
}

static int author_proof_envelope(mcsc_slab_list *out) {
    mcsc_slab *s = mcsc_slab_new(
        "proof_envelope",
        "implementations/c/provekit-self-contracts/src/proof_envelope.c");
    if (!s) return -1;

    pksc_value *f;
    /* kind field is "catalog" (no forall) */
    f = mcsc_f_eq(ctor1("proof_envelope_kind_field", mcsc_f_str("")),
                  mcsc_f_str("catalog"));
    if (mcsc_slab_must(s, "c_proof_envelope_kind_is_literal_catalog", f) != 0) goto fail;

    f = mcsc_f_forall("bytes", mcsc_f_sort("String"), body_proof_filename_blake3, NULL);
    if (mcsc_slab_must(s, "c_proof_envelope_filename_cid_is_blake3_of_bytes", f) != 0) goto fail;

    f = mcsc_f_forall("bytes", mcsc_f_sort("String"), body_proof_filename_prefix, NULL);
    if (mcsc_slab_must(s, "c_proof_envelope_filename_cid_starts_with_blake3_prefix", f) != 0) goto fail;

    f = mcsc_f_forall("m", mcsc_f_sort("String"), body_proof_keys_sorted, NULL);
    if (mcsc_slab_must(s, "c_proof_envelope_map_keys_sorted_bytewise", f) != 0) goto fail;

    f = mcsc_f_forall("bytes", mcsc_f_sort("String"), body_proof_verify_rejects, NULL);
    if (mcsc_slab_must(s, "c_proof_envelope_verify_rejects_cid_mismatch", f) != 0) goto fail;

    return mcsc_slab_list_push(out, s);
fail:
    mcsc_slab_free(s);
    return -1;
}

/* ============================================================== */
/* author_all                                                      */
/* ============================================================== */

mcsc_slab_list *mcsc_author_all(void) {
    mcsc_slab_list *l = mcsc_slab_list_new();
    if (!l) return NULL;
    if (author_blake3(l) != 0) goto fail;
    if (author_cbor(l) != 0) goto fail;
    if (author_claim_envelope(l) != 0) goto fail;
    if (author_ed25519(l) != 0) goto fail;
    if (author_jcs(l) != 0) goto fail;
    if (author_proof_envelope(l) != 0) goto fail;
    return l;
fail:
    mcsc_slab_list_free(l);
    return NULL;
}
