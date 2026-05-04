/* SPDX-License-Identifier: Apache-2.0 */

#include "provekit/ir.h"
#include "provekit/value.h"
#include "provekit/sign.h"
#include "provekit/cbor.h"
#include "provekit/claim_envelope.h"
#include "provekit/proof_envelope.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* -----------------------------------------------------------------------
 * Test helpers
 * --------------------------------------------------------------------- */

static int g_failures = 0;

static void check(int cond, const char *msg) {
    if (!cond) {
        fprintf(stderr, "FAIL: %s\n", msg);
        g_failures++;
    }
}

static void check_str(const char *got, const char *expected, const char *msg) {
    if (!got || strcmp(got, expected) != 0) {
        fprintf(stderr, "FAIL: %s\n  expected: %s\n  got:      %s\n",
                msg, expected, got ? got : "(null)");
        g_failures++;
    }
}

static char *emit_formula(pk_formula *f) {
    pk_buffer *buf = pk_buffer_new();
    pk_emit_formula(buf, f);
    char *s = pk_buffer_steal(buf);
    pk_buffer_free(buf);
    return s;
}

/* Hex-encode bytes into a stack buffer. dst must be at least 2*len+1 bytes. */
static void bytes_to_hex(const uint8_t *b, size_t len, char *dst) {
    static const char h[] = "0123456789abcdef";
    for (size_t i = 0; i < len; i++) {
        dst[2*i]   = h[(b[i]>>4)&0xf];
        dst[2*i+1] = h[b[i]&0xf];
    }
    dst[2*len] = '\0';
}

/* -----------------------------------------------------------------------
 * Original IR / JCS tests (unchanged from before)
 * --------------------------------------------------------------------- */

static void test_eq_atomic_jcs(void) {
    pk_term *parse_int_arg = pk_term_const_str("42", pk_sort_primitive("String"));
    pk_term *parse_int_args[] = { parse_int_arg };
    pk_term *lhs = pk_term_ctor_new("parse_int", parse_int_args, 1);

    pk_term *rhs = pk_term_const_int(42, pk_sort_primitive("Int"));

    pk_term *args[] = { lhs, rhs };
    pk_formula *f = pk_formula_atomic_new("=", args, 2);

    char *got = emit_formula(f);

    const char *expected =
        "{\"args\":[{\"args\":[{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"},\"value\":\"42\"}],\"kind\":\"ctor\",\"name\":\"parse_int\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":42}],"
        "\"kind\":\"atomic\",\"name\":\"=\"}";

    check_str(got, expected, "eq atomic JCS");

    free(got);
    pk_formula_free(f);
}

static void test_pattern1_bounded_loop_jcs(void) {
    pk_term *x1 = pk_term_var_new("x");
    pk_term *zero1 = pk_term_const_int(0, pk_sort_primitive("Int"));
    pk_term *gte_args[] = { x1, zero1 };
    pk_formula *lower = pk_formula_atomic_new("\xe2\x89\xa5", gte_args, 2);

    pk_term *x2 = pk_term_var_new("x");
    pk_term *hundred = pk_term_const_int(100, pk_sort_primitive("Int"));
    pk_term *lt_args[] = { x2, hundred };
    pk_formula *upper = pk_formula_atomic_new("<", lt_args, 2);

    pk_formula *conj_ops[] = { lower, upper };
    pk_formula *antecedent = pk_formula_connective_new("and", conj_ops, 2);

    pk_term *x3 = pk_term_var_new("x");
    pk_term *zero2 = pk_term_const_int(0, pk_sort_primitive("Int"));
    pk_term *gte2_args[] = { x3, zero2 };
    pk_formula *inner = pk_formula_atomic_new("\xe2\x89\xa5", gte2_args, 2);

    pk_formula *impl_ops[] = { antecedent, inner };
    pk_formula *body = pk_formula_connective_new("implies", impl_ops, 2);

    pk_formula *q = pk_formula_quantifier_new("forall", "x", pk_sort_primitive("Int"), body);

    char *got = emit_formula(q);

    const char *expected =
        "{\"body\":{\"kind\":\"implies\",\"operands\":[{\"kind\":\"and\",\"operands\":[{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],\"kind\":\"atomic\",\"name\":\"\xe2\x89\xa5\"},"
        "{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":100}],"
        "\"kind\":\"atomic\",\"name\":\"<\"}]},{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],\"kind\":\"atomic\",\"name\":\"\xe2\x89\xa5\"}]},"
        "\"kind\":\"forall\",\"name\":\"x\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";

    check_str(got, expected, "pattern1 bounded loop JCS");

    free(got);
    pk_formula_free(q);
}

static void test_contract_decl_jcs(void) {
    pk_term *x = pk_term_var_new("x");
    pk_term *zero = pk_term_const_int(0, pk_sort_primitive("Int"));
    pk_term *args[] = { x, zero };
    pk_formula *pre = pk_formula_atomic_new("\xe2\x89\xa5", args, 2);

    pk_decl *d = pk_decl_contract_new("parseInt", "out", pre, NULL, NULL);
    pk_buffer *buf = pk_buffer_new();
    pk_emit_decl(buf, d);
    char *got = pk_buffer_steal(buf);
    pk_buffer_free(buf);

    const char *expected =
        "{\"kind\":\"contract\",\"name\":\"parseInt\",\"outBinding\":\"out\","
        "\"pre\":{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],"
        "\"kind\":\"atomic\",\"name\":\"\xe2\x89\xa5\"}}";

    check_str(got, expected, "contract decl JCS");

    free(got);
    pk_decl_free(d);
}

static void test_bridge_decl_jcs(void) {
    pk_decl *d = pk_decl_bridge_new(
        "myBridge", "source", "c-kit",
        "bafySource", "bafyTarget", "bafyProof",
        "coq", "some notes"
    );
    pk_buffer *buf = pk_buffer_new();
    pk_emit_decl(buf, d);
    char *got = pk_buffer_steal(buf);
    pk_buffer_free(buf);

    const char *expected =
        "{\"kind\":\"bridge\",\"name\":\"myBridge\",\"notes\":\"some notes\","
        "\"sourceContractCid\":\"bafySource\",\"sourceLayer\":\"c-kit\","
        "\"sourceSymbol\":\"source\",\"targetContractCid\":\"bafyTarget\","
        "\"targetLayer\":\"coq\",\"targetProofCid\":\"bafyProof\"}";

    check_str(got, expected, "bridge decl JCS");

    free(got);
    pk_decl_free(d);
}

static void test_eq_atomic_hash(void) {
    pk_term *parse_int_arg = pk_term_const_str("42", pk_sort_primitive("String"));
    pk_term *parse_int_args[] = { parse_int_arg };
    pk_term *lhs = pk_term_ctor_new("parse_int", parse_int_args, 1);
    pk_term *rhs = pk_term_const_int(42, pk_sort_primitive("Int"));
    pk_term *args[] = { lhs, rhs };
    pk_formula *f = pk_formula_atomic_new("=", args, 2);

    char *jcs = emit_formula(f);
    char *hash = pk_hash_jcs(jcs);

    const char *expected_hash =
        "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18"
        "539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa";

    check_str(hash, expected_hash, "eq atomic hash");

    free(jcs);
    free(hash);
    pk_formula_free(f);
}

/* -----------------------------------------------------------------------
 * Generic pk_value + JCS tests
 * --------------------------------------------------------------------- */

static void test_value_jcs_object_sorted(void) {
    pk_value *obj = pk_value_object_empty();
    pk_value_object_push(obj, "z", pk_value_integer(3));
    pk_value_object_push(obj, "a", pk_value_string("x"));
    pk_value_object_push(obj, "m", pk_value_bool(1));
    char *jcs = pk_value_to_jcs(obj);
    /* Keys must be sorted: a, m, z */
    check_str(jcs, "{\"a\":\"x\",\"m\":true,\"z\":3}", "value JCS sorts keys");
    free(jcs);
    pk_value_free(obj);
}

static void test_value_jcs_null_and_bool(void) {
    pk_value *arr = pk_value_array_empty();
    pk_value_array_push(arr, pk_value_null());
    pk_value_array_push(arr, pk_value_bool(1));
    pk_value_array_push(arr, pk_value_bool(0));
    char *jcs = pk_value_to_jcs(arr);
    check_str(jcs, "[null,true,false]", "value JCS null and bool");
    free(jcs);
    pk_value_free(arr);
}

static void test_value_jcs_escape(void) {
    pk_value *v = pk_value_string("a\"b\\c\x01z");
    char *jcs = pk_value_to_jcs(v);
    check_str(jcs, "\"a\\\"b\\\\c\\u0001z\"", "value JCS string escaping");
    free(jcs);
    pk_value_free(v);
}

/* -----------------------------------------------------------------------
 * Ed25519 cross-kit pin test (seed=[0x42;32], msg="hello")
 * --------------------------------------------------------------------- */

static void test_ed25519_cross_kit_pin(void) {
    uint8_t seed[32];
    memset(seed, 0x42, 32);

    /* --- pubkey string --- */
    char *pubkey_str = pk_sign_pubkey_string(seed);
    check_str(pubkey_str,
              "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
              "ed25519 pubkey string cross-kit pin (seed=[0x42;32])");

    /* --- signature string --- */
    char *sig_str = pk_sign_string((const uint8_t *)"hello", 5, seed);
    check_str(sig_str,
              "ed25519:+hDqZG1+6AmUvd3QOUJHm2Gp1Uliz/4+YpU3JmuK3Ea1pgsgTxeYvzI5i8LU7415Gk5K58OetD6DVjzmfTnkBQ==",
              "ed25519 sig string cross-kit pin (seed=[0x42;32], msg=hello)");

    /* --- verify round-trip --- */
    int ok = pk_verify_string(pubkey_str, sig_str,
                              (const uint8_t *)"hello", 5);
    check(ok, "ed25519 verify round-trip");

    /* --- verify rejects wrong message --- */
    int bad = pk_verify_string(pubkey_str, sig_str,
                               (const uint8_t *)"world", 5);
    check(!bad, "ed25519 verify rejects wrong message");

    free(pubkey_str);
    free(sig_str);
}

/* -----------------------------------------------------------------------
 * CBOR deterministic encoding tests
 * --------------------------------------------------------------------- */

static void test_cbor_uint_shortest_form(void) {
    pk_cbor_buf *b;
    char hex[8];

    b = pk_cbor_buf_new();
    pk_cbor_encode_uint(b, 0);
    bytes_to_hex(b->data, b->len, hex);
    check_str(hex, "00", "CBOR uint 0 = 0x00");
    pk_cbor_buf_free(b);

    b = pk_cbor_buf_new();
    pk_cbor_encode_uint(b, 23);
    bytes_to_hex(b->data, b->len, hex);
    check_str(hex, "17", "CBOR uint 23 = 0x17");
    pk_cbor_buf_free(b);

    b = pk_cbor_buf_new();
    pk_cbor_encode_uint(b, 24);
    bytes_to_hex(b->data, b->len, hex);
    check_str(hex, "1818", "CBOR uint 24 = 0x18 0x18");
    pk_cbor_buf_free(b);

    b = pk_cbor_buf_new();
    pk_cbor_encode_uint(b, 256);
    bytes_to_hex(b->data, b->len, hex);
    check_str(hex, "190100", "CBOR uint 256");
    pk_cbor_buf_free(b);
}

static void test_cbor_tstr(void) {
    pk_cbor_buf *b = pk_cbor_buf_new();
    pk_cbor_encode_tstrz(b, "hello");
    /* major 3, len 5 short = 0x65, then "hello" */
    check(b->len == 6, "CBOR tstr hello length");
    check(b->data[0] == 0x65, "CBOR tstr hello head byte");
    check(memcmp(b->data + 1, "hello", 5) == 0, "CBOR tstr hello bytes");
    pk_cbor_buf_free(b);
}

/* -----------------------------------------------------------------------
 * claim_envelope cross-kit byte-equivalence pin
 *
 * Pinned from Rust:
 *   cargo run -p provekit-claim-envelope --bin emit_c_fixtures
 * (commit that added the bin is removed after fixtures are captured)
 * --------------------------------------------------------------------- */

static pk_value *make_pre_n_gt_0(void) {
    /* pre = {kind: atomic, name: ">", args: [{kind: var, name: n}, {kind: const, value: 0, sort: {kind: primitive, name: Int}}]} */
    pk_value *arg0 = pk_value_object_empty();
    pk_value_object_push(arg0, "kind", pk_value_string("var"));
    pk_value_object_push(arg0, "name", pk_value_string("n"));

    pk_value *sort = pk_value_object_empty();
    pk_value_object_push(sort, "kind", pk_value_string("primitive"));
    pk_value_object_push(sort, "name", pk_value_string("Int"));

    pk_value *arg1 = pk_value_object_empty();
    pk_value_object_push(arg1, "kind",  pk_value_string("const"));
    pk_value_object_push(arg1, "value", pk_value_integer(0));
    pk_value_object_push(arg1, "sort",  sort);

    pk_value *args = pk_value_array_empty();
    pk_value_array_push(args, arg0);
    pk_value_array_push(args, arg1);

    pk_value *pre = pk_value_object_empty();
    pk_value_object_push(pre, "kind", pk_value_string("atomic"));
    pk_value_object_push(pre, "name", pk_value_string(">"));
    pk_value_object_push(pre, "args", args);
    return pre;
}

static void test_claim_envelope_cross_kit_pin(void) {
    /* Fixture: mint_contract with:
     *   name="parseInt", outBinding="out", pre=n>0 (no post/inv)
     *   produced_by="c-kit@1.0", produced_at="2026-04-30T00:00:00.000Z"
     *   signer_seed=[0x42;32], authoring=KitAuthor{author="c-kit@1.0"}
     *
     * Pinned from Rust emit_c_fixtures output:
     *   MINT_CONTRACT_CID = blake3-512:1b018b056a06be...
     *   MINT_CONTRACT_CONTRACT_CID = blake3-512:23fc5f326cdb...
     */
    uint8_t seed[32];
    memset(seed, 0x42, 32);

    pk_value *pre = make_pre_n_gt_0();

    pk_mint_contract_args_t args;
    memset(&args, 0, sizeof(args));
    args.contract_name  = "parseInt";
    args.pre            = pre;
    args.out_binding    = "out";
    args.produced_by    = "c-kit@1.0";
    args.produced_at    = "2026-04-30T00:00:00.000Z";
    args.authoring.author = "c-kit@1.0";
    args.authoring.note   = NULL;
    memcpy(args.signer_seed, seed, 32);

    pk_minted_envelope_t env;
    int rc = pk_mint_contract(&args, &env);
    check(rc == 0, "pk_mint_contract returns 0");

    if (rc != 0) { pk_value_free(pre); return; }

    /* Contract CID (content CID) must match Rust pin */
    check_str(env.contract_cid,
              "blake3-512:23fc5f326cdb9c28fea3bbf4ea73077cd4676a6d1431d17007d3fa6c631d24d5637d07c2bf1bd4c9e7e3d0afda07e603d83ab71b72b7da4328a77c6ad7d1ad25",
              "mint_contract contract_cid cross-kit pin");

    /* Attestation CID must match Rust pin */
    check_str(env.attestation_cid,
              "blake3-512:1b018b056a06be694ce92619528ccb4daf23f678671f52d51a8288109fed032adff5a844980f4e6a28852e563627a70c2af14d8e4d827a8e01c5c1a853c5e0c0",
              "mint_contract attestation_cid cross-kit pin");

    /* canonical_bytes length must match Rust */
    check(env.canonical_len == 1240,
          "mint_contract canonical_bytes length == 1240 (Rust pin)");

    /* canonical_bytes hex must match Rust */
    char *got_hex = (char *)malloc(env.canonical_len * 2 + 1);
    if (got_hex) {
        bytes_to_hex(env.canonical_bytes, env.canonical_len, got_hex);
        check_str(got_hex,
            "7b22656e76656c6f7065223a7b226465636c617265644174223a22323032362d30342d33305430303a30303a30302e3030305a222c227369676e6174757265223a22656432353531393a4b4e666845346a38377344652b773136796d463545334b686a57725758596b76774733694241784b39464f36507656686c5755712f3553424772513344614d71666c7637394d59712b5a304838335a696c497a5a44513d3d222c227369676e6572223a22656432353531393a49564c34305a7435485352464d6b4c6858793672624c66502b6e747158744d416c35594f427069423278493d227d2c22686561646572223a7b2262696e64696e6748617368223a22626c616b65332d3531323a3835613433323832396262396639623434313864613839343239366363333961333962383464636531316637363836363964383536616635653466616537396464343465666337636633303066303731356437326563633263373238393930663535393031356138303666653836363534613033356536626138623666356366222c22636964223a22626c616b65332d3531323a3233666335663332366364623963323866656133626266346561373330373763643436373661366431343331643137303037643366613663363331643234643536333764303763326266316264346339653765336430616664613037653630336438336162373162373262376461343332386137376336616437643161643235222c22696e70757443696473223a5b5d2c226b696e64223a22636f6e7472616374222c226e616d65223a227061727365496e74222c226f757442696e64696e67223a226f7574222c22707265223a7b2261726773223a5b7b226b696e64223a22766172222c226e616d65223a226e227d2c7b226b696e64223a22636f6e7374222c22736f7274223a7b226b696e64223a227072696d6974697665222c226e616d65223a22496e74227d2c2276616c7565223a307d5d2c226b696e64223a2261746f6d6963222c226e616d65223a223e227d2c2270726f706572747948617368223a22626c616b65332d3531323a3935333336646532393461623933666365633235643437633064363238643166646461343761366665623739646639383935366237616262316264643962663337626262396331316134373766306262363533343832353632623431363735663262666435613935646138323935353539343533383263353165346664326433222c22736368656d6156657273696f6e223a2232222c2276657264696374223a22686f6c6473227d2c226d65746164617461223a7b22617574686f72696e67223a7b22617574686f72223a22632d6b697440312e30222c2270726f64756365724b696e64223a226b69742d617574686f72227d2c2270726548617368223a22626c616b65332d3531323a6263306265373533306134316130646663393065613538323565636333646533393861633062393332333139613166376635643530366237363737646662323438303935326536313239303739336164636233646633376435386666643761356562633339633430333336613262333730663966623439373336633163346661222c2270726f64756365644174223a22323032362d30342d33305430303a30303a30302e3030305a222c2270726f64756365644279223a22632d6b697440312e30227d7d",
            "mint_contract canonical_bytes hex cross-kit pin");
        free(got_hex);
    }

    pk_minted_envelope_free(&env);
    pk_value_free(pre);
}

/* -----------------------------------------------------------------------
 * proof_envelope cross-kit byte-equivalence pin
 * --------------------------------------------------------------------- */

static void test_proof_envelope_cross_kit_pin(void) {
    uint8_t seed[32];
    memset(seed, 0x42, 32);

    char *signer_cid = pk_sign_pubkey_string(seed);

    /* Build the member bytes from the mint_contract fixture */
    pk_value *pre = make_pre_n_gt_0();
    pk_mint_contract_args_t args;
    memset(&args, 0, sizeof(args));
    args.contract_name   = "parseInt";
    args.pre             = pre;
    args.out_binding     = "out";
    args.produced_by     = "c-kit@1.0";
    args.produced_at     = "2026-04-30T00:00:00.000Z";
    args.authoring.author = "c-kit@1.0";
    memcpy(args.signer_seed, seed, 32);

    pk_minted_envelope_t env;
    int rc = pk_mint_contract(&args, &env);
    check(rc == 0, "proof_envelope: mint_contract for member");
    if (rc != 0) { free(signer_cid); pk_value_free(pre); return; }

    pk_proof_member_t member;
    member.cid   = env.attestation_cid;
    member.bytes = env.canonical_bytes;
    member.len   = env.canonical_len;

    pk_proof_envelope_input_t inp;
    memset(&inp, 0, sizeof(inp));
    inp.name        = "@provekit/c-test";
    inp.version     = "0.0.1";
    inp.binary_cid  = NULL;
    inp.members     = &member;
    inp.n_members   = 1;
    inp.signer_cid  = signer_cid;
    inp.declared_at = "2026-04-30T00:00:00.000Z";
    memcpy(inp.signer_seed, seed, 32);

    pk_proof_envelope_output_t proof;
    rc = pk_build_proof_envelope(&inp, &proof);
    check(rc == 0, "pk_build_proof_envelope returns 0");

    if (rc == 0) {
        check_str(proof.cid,
                  "blake3-512:9193c2e7deb36a0e358f6efe12c12bd63de6fefa7376d7894e0f1030392d80ad3553cd57a8ea4b2102cfeb3a1646ab50857adc806215908685eacddd7e9adfe7",
                  "proof_envelope CID cross-kit pin");

        /* Round-trip verify */
        int ok = pk_verify_proof(proof.bytes, proof.len, proof.cid);
        check(ok, "pk_verify_proof round-trip");

        /* Reject wrong CID */
        int bad = pk_verify_proof(proof.bytes, proof.len,
                                  "blake3-512:0000000000000000000000000000000000000000000000000000000000000000"
                                  "0000000000000000000000000000000000000000000000000000000000000000");
        check(!bad, "pk_verify_proof rejects wrong cid");

        pk_proof_envelope_output_free(&proof);
    }

    pk_minted_envelope_free(&env);
    pk_value_free(pre);
    free(signer_cid);
}

/* -----------------------------------------------------------------------
 * Main
 * --------------------------------------------------------------------- */

int main(void) {
    printf("Running C kit tests...\n"); fflush(stdout);

    /* Original IR / JCS tests */
    test_eq_atomic_jcs();
    printf("  test_eq_atomic_jcs passed\n"); fflush(stdout);
    test_pattern1_bounded_loop_jcs();
    printf("  test_pattern1_bounded_loop_jcs passed\n"); fflush(stdout);
    test_contract_decl_jcs();
    printf("  test_contract_decl_jcs passed\n"); fflush(stdout);
    test_bridge_decl_jcs();
    printf("  test_bridge_decl_jcs passed\n"); fflush(stdout);
    test_eq_atomic_hash();
    printf("  test_eq_atomic_hash passed\n"); fflush(stdout);

    /* Generic pk_value / JCS */
    test_value_jcs_object_sorted();
    printf("  test_value_jcs_object_sorted passed\n"); fflush(stdout);
    test_value_jcs_null_and_bool();
    printf("  test_value_jcs_null_and_bool passed\n"); fflush(stdout);
    test_value_jcs_escape();
    printf("  test_value_jcs_escape passed\n"); fflush(stdout);

    /* Ed25519 cross-kit pin */
    test_ed25519_cross_kit_pin();
    printf("  test_ed25519_cross_kit_pin passed\n"); fflush(stdout);

    /* CBOR */
    test_cbor_uint_shortest_form();
    printf("  test_cbor_uint_shortest_form passed\n"); fflush(stdout);
    test_cbor_tstr();
    printf("  test_cbor_tstr passed\n"); fflush(stdout);

    /* claim_envelope cross-kit pin */
    test_claim_envelope_cross_kit_pin();
    printf("  test_claim_envelope_cross_kit_pin passed\n"); fflush(stdout);

    /* proof_envelope cross-kit pin + round-trip */
    test_proof_envelope_cross_kit_pin();
    printf("  test_proof_envelope_cross_kit_pin passed\n"); fflush(stdout);

    if (g_failures > 0) {
        printf("FAILED: %d test(s) failed.\n", g_failures);
        return 1;
    }
    printf("Done. All tests passed.\n");
    return 0;
}
