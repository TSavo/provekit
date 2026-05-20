/* SPDX-License-Identifier: Apache-2.0 */
/*
 * provekit-self-contracts (C kit) — test runner.
 *
 * Coverage:
 *   1. JCS canonicalization (RFC 8785) — sort, escapes, Unicode passthrough.
 *   2. Deterministic CBOR (RFC 8949 §4.2.1) — shortest-form integers, heads.
 *   3. Ed25519 (libsodium) — determinism, sig length, round-trip,
 *      spec-form ("ed25519:<b64>") sign/verify, foundation-v0 pubkey.
 *   4. Proof envelope — round-trip, CID stability, verify-rejects-tampered.
 *   5. CROSS-KIT BYTE EQUIVALENCE — pinned hex from the Rust reference,
 *      duplicated from the Python cross-kit test (which itself pins the
 *      Rust output). C output MUST match this hex EXACTLY for the same
 *      input. Failure surfaces the first divergent byte.
 *
 * Run via `make test` in this directory or `make test-c` from repo root.
 */

/* strdup is POSIX (since 2008) but not C11. Without this macro and the
 * Makefile's -std=c11, <string.h> does not declare strdup; the resulting
 * implicit-declaration warning means the int return is truncated when
 * cast to a pointer on 64-bit hosts, the cross-kit byte-equivalence
 * tests pass corrupt key/bytes pointers to proof_envelope_build, and
 * the proof-envelope section segfaults. Must precede any include.
 */
#define _POSIX_C_SOURCE 200809L

#include "provekit/self_contracts.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int g_failures = 0;
static int g_passes   = 0;

#define EXPECT(expr, label)                                           \
    do {                                                              \
        if (expr) {                                                   \
            ++g_passes;                                               \
            fprintf(stderr, "  ok    %s\n", (label));                 \
        } else {                                                      \
            ++g_failures;                                             \
            fprintf(stderr, "  FAIL  %s  (line %d)\n", (label), __LINE__); \
        }                                                             \
    } while (0)

#define EXPECT_STR_EQ(actual, expected, label)                        \
    do {                                                              \
        const char *_a = (actual);                                    \
        const char *_e = (expected);                                  \
        int _ok = (_a && _e && strcmp(_a, _e) == 0);                  \
        if (_ok) {                                                    \
            ++g_passes;                                               \
            fprintf(stderr, "  ok    %s\n", (label));                 \
        } else {                                                      \
            ++g_failures;                                             \
            fprintf(stderr, "  FAIL  %s\n    got:      %s\n    expected: %s\n", \
                    (label), _a ? _a : "(null)", _e ? _e : "(null)"); \
        }                                                             \
    } while (0)

/* ------------------------------------------------------------------- */
/* Hex helpers                                                          */
/* ------------------------------------------------------------------- */

static int hex_digit(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return 10 + (c - 'a');
    if (c >= 'A' && c <= 'F') return 10 + (c - 'A');
    return -1;
}

/* Decode a contiguous hex string (no whitespace) into a malloc'd byte
 * buffer. Returns 0 on success and sets *out / *out_len. */
static int hex_decode(const char *hex, uint8_t **out, size_t *out_len) {
    size_t n = strlen(hex);
    if (n % 2 != 0) return -1;
    uint8_t *buf = (uint8_t *)malloc(n / 2);
    if (!buf) return -1;
    for (size_t i = 0; i < n; i += 2) {
        int hi = hex_digit(hex[i]);
        int lo = hex_digit(hex[i+1]);
        if (hi < 0 || lo < 0) { free(buf); return -1; }
        buf[i/2] = (uint8_t)((hi << 4) | lo);
    }
    *out = buf;
    *out_len = n / 2;
    return 0;
}

static char *bytes_to_hex(const uint8_t *bytes, size_t n) {
    char *s = (char *)malloc(2 * n + 1);
    if (!s) return NULL;
    static const char hex[] = "0123456789abcdef";
    for (size_t i = 0; i < n; i++) {
        s[2*i] = hex[(bytes[i] >> 4) & 0xF];
        s[2*i + 1] = hex[bytes[i] & 0xF];
    }
    s[2*n] = '\0';
    return s;
}

/* ------------------------------------------------------------------- */
/* JCS                                                                  */
/* ------------------------------------------------------------------- */

static void test_jcs_simple_object_sorts_keys(void) {
    pksc_value *v = pksc_v_obj_new();
    pksc_v_obj_set(v, "b", pksc_v_int(1));
    pksc_v_obj_set(v, "a", pksc_v_str("x"));
    char *s = pksc_jcs_encode_string(v);
    EXPECT_STR_EQ(s, "{\"a\":\"x\",\"b\":1}", "jcs sorts keys");
    free(s);
    pksc_value_free(v);
}

static void test_jcs_escapes_quotes_and_backslash(void) {
    pksc_value *v = pksc_v_str("a\"b\\c");
    char *s = pksc_jcs_encode_string(v);
    EXPECT_STR_EQ(s, "\"a\\\"b\\\\c\"", "jcs escapes quote+backslash");
    free(s);
    pksc_value_free(v);
}

static void test_jcs_escapes_control_chars_lowercase_hex(void) {
    pksc_value *v = pksc_v_obj_new();
    /* Use direct raw bytes via JCS encoding by going through a string. */
    char *str = (char *)malloc(4);
    str[0] = 'a'; str[1] = 0x01; str[2] = 'b'; str[3] = '\0';
    pksc_v_obj_set(v, "x", pksc_v_str(str));
    free(str);
    char *s = pksc_jcs_encode_string(v);
    EXPECT_STR_EQ(s, "{\"x\":\"a\\u0001b\"}", "jcs escapes U+0001 as \\u0001");
    free(s);
    pksc_value_free(v);
}

static void test_jcs_unicode_passthrough(void) {
    /* >= U+0080 emitted verbatim (UTF-8 bytes preserved). */
    pksc_value *v = pksc_v_str("\xe2\x89\xa5");  /* U+2265 GREATER-THAN OR EQUAL TO */
    char *s = pksc_jcs_encode_string(v);
    EXPECT_STR_EQ(s, "\"\xe2\x89\xa5\"", "jcs unicode passthrough");
    free(s);
    pksc_value_free(v);
}

static void test_jcs_empty_object_and_array(void) {
    pksc_value *o = pksc_v_obj_new();
    pksc_value *a = pksc_v_arr_new();
    char *so = pksc_jcs_encode_string(o);
    char *sa = pksc_jcs_encode_string(a);
    EXPECT_STR_EQ(so, "{}", "jcs empty object");
    EXPECT_STR_EQ(sa, "[]", "jcs empty array");
    free(so); free(sa);
    pksc_value_free(o); pksc_value_free(a);
}

static void test_jcs_nested_array_object(void) {
    pksc_value *root = pksc_v_obj_new();
    pksc_value *xs   = pksc_v_arr_new();
    pksc_v_arr_push(xs, pksc_v_int(1));
    pksc_v_arr_push(xs, pksc_v_int(2));
    pksc_v_obj_set(root, "xs", xs);
    char *s = pksc_jcs_encode_string(root);
    EXPECT_STR_EQ(s, "{\"xs\":[1,2]}", "jcs nested array");
    free(s);
    pksc_value_free(root);
}

/* ------------------------------------------------------------------- */
/* CBOR                                                                 */
/* ------------------------------------------------------------------- */

static void test_cbor_shortest_form_uint(void) {
    pksc_bytes b;
    pksc_bytes_init(&b);

    pksc_cbor_encode_uint(&b, 0);
    EXPECT(b.len == 1 && b.data[0] == 0x00, "cbor uint 0");
    pksc_bytes_free(&b);

    pksc_bytes_init(&b);
    pksc_cbor_encode_uint(&b, 23);
    EXPECT(b.len == 1 && b.data[0] == 0x17, "cbor uint 23");
    pksc_bytes_free(&b);

    pksc_bytes_init(&b);
    pksc_cbor_encode_uint(&b, 24);
    EXPECT(b.len == 2 && b.data[0] == 0x18 && b.data[1] == 24, "cbor uint 24 (u8 form)");
    pksc_bytes_free(&b);

    pksc_bytes_init(&b);
    pksc_cbor_encode_uint(&b, 256);
    EXPECT(b.len == 3 && b.data[0] == 0x19 && b.data[1] == 0x01 && b.data[2] == 0x00, "cbor uint 256 (u16 form)");
    pksc_bytes_free(&b);

    pksc_bytes_init(&b);
    pksc_cbor_encode_uint(&b, 65536);
    EXPECT(b.len == 5 && b.data[0] == 0x1a, "cbor uint 65536 (u32 form)");
    pksc_bytes_free(&b);
}

static void test_cbor_tstr(void) {
    pksc_bytes b;
    pksc_bytes_init(&b);
    pksc_cbor_encode_tstr(&b, "hello");
    /* major 3 (text string), len 5 short form: 0x65, then "hello" */
    EXPECT(b.len == 6 && b.data[0] == 0x65 && memcmp(b.data + 1, "hello", 5) == 0, "cbor tstr 'hello'");
    pksc_bytes_free(&b);
}

/* ------------------------------------------------------------------- */
/* Ed25519                                                              */
/* ------------------------------------------------------------------- */

static void test_ed25519_deterministic_signature(void) {
    uint8_t seed[32];
    memset(seed, 0x42, sizeof seed);
    uint8_t a[64], b[64];
    pksc_ed25519_sign_with_seed(a, seed, (const uint8_t *)"hello", 5);
    pksc_ed25519_sign_with_seed(b, seed, (const uint8_t *)"hello", 5);
    EXPECT(memcmp(a, b, 64) == 0, "ed25519 same seed/msg => same sig");
}

static void test_ed25519_round_trip(void) {
    uint8_t seed[32];
    memset(seed, 0x42, sizeof seed);
    uint8_t pk[32];
    pksc_ed25519_pubkey_from_seed(pk, seed);
    uint8_t sig[64];
    pksc_ed25519_sign_with_seed(sig, seed, (const uint8_t *)"hello world", 11);

    EXPECT(pksc_ed25519_verify(pk, sig, (const uint8_t *)"hello world", 11) == 1,
           "ed25519 verify accepts good sig");
    EXPECT(pksc_ed25519_verify(pk, sig, (const uint8_t *)"goodbye world", 13) == 0,
           "ed25519 verify rejects wrong msg");
}

static void test_ed25519_string_form_round_trip(void) {
    uint8_t seed[32];
    memset(seed, 0x42, sizeof seed);
    char *pk = pksc_ed25519_pubkey_string(seed);
    char *sig = pksc_ed25519_sign_string(seed, (const uint8_t *)"hello world", 11);
    EXPECT(pk && strncmp(pk, "ed25519:", 8) == 0, "pubkey string has prefix");
    EXPECT(sig && strncmp(sig, "ed25519:", 8) == 0, "sig string has prefix");

    EXPECT(pksc_ed25519_verify_string(pk, sig, (const uint8_t *)"hello world", 11) == 1,
           "ed25519 verify_string accepts good");
    EXPECT(pksc_ed25519_verify_string(pk, sig, (const uint8_t *)"tampered", 8) == 0,
           "ed25519 verify_string rejects tampered msg");
    EXPECT(pksc_ed25519_verify_string("not-prefixed", sig, (const uint8_t *)"x", 1) == 0,
           "ed25519 verify_string rejects malformed pubkey");
    EXPECT(pksc_ed25519_verify_string(pk, "ed25519:!!!!", (const uint8_t *)"x", 1) == 0,
           "ed25519 verify_string rejects malformed sig (bad b64)");

    free(pk); free(sig);
}

static void test_ed25519_foundation_v0_seed(void) {
    /* The seed constant must be exactly [0x42; 32] across all kits. */
    int all_42 = 1;
    for (int i = 0; i < 32; i++) {
        if (PKSC_FOUNDATION_V0_SEED[i] != 0x42) { all_42 = 0; break; }
    }
    EXPECT(all_42, "foundation v0 seed is [0x42; 32]");
}

/* ------------------------------------------------------------------- */
/* Proof envelope                                                       */
/* ------------------------------------------------------------------- */

/* Two-member fixture, identical to Python's _two_member_input(). */
static int build_two_member_input(pksc_proof_input *in,
                                  pksc_member members[2]) {
    members[0].key   = strdup("blake3-512:aa");
    members[0].bytes = (uint8_t *)strdup("{\"hello\":\"world\"}");
    members[0].len   = strlen((char *)members[0].bytes);
    members[1].key   = strdup("blake3-512:bb");
    members[1].bytes = (uint8_t *)strdup("{\"goodbye\":\"world\"}");
    members[1].len   = strlen((char *)members[1].bytes);

    in->name        = "@test/cat";
    in->version     = "1.0.0";
    in->binary_cid  = NULL;
    in->metadata    = NULL;
    in->n_metadata  = 0;
    in->members     = members;
    in->n_members   = 2;
    in->signer_cid  = "blake3-512:cc";
    in->declared_at = "2026-04-30T00:00:00.000Z";
    in->signer_seed = PKSC_FOUNDATION_V0_SEED;
    return 0;
}

static void free_members(pksc_member *members, size_t n) {
    for (size_t i = 0; i < n; i++) {
        free(members[i].key);
        free(members[i].bytes);
    }
}

static void test_proof_round_trip(void) {
    pksc_member members[2];
    pksc_proof_input in;
    build_two_member_input(&in, members);

    pksc_proof_output out = {0};
    int rc = pksc_proof_build(&in, &out);
    EXPECT(rc == 0, "proof_build returns 0");
    EXPECT(out.bytes != NULL && out.len > 0, "proof bytes populated");
    EXPECT(out.cid && strncmp(out.cid, "blake3-512:", 11) == 0, "proof cid has prefix");
    /* 7-key map head: major 5 (0xA0) + count 7 = 0xA7. */
    EXPECT(out.bytes[0] == 0xA7, "signed map head 0xA7 (7 keys)");

    /* Verify with the foundation public key. */
    uint8_t pk[32];
    pksc_ed25519_pubkey_from_seed(pk, PKSC_FOUNDATION_V0_SEED);
    EXPECT(pksc_proof_verify(out.bytes, out.len, out.cid, pk) == 1,
           "proof_verify accepts our own output");

    /* Tampered CID is rejected. */
    char fake_cid[140];
    snprintf(fake_cid, sizeof fake_cid, "blake3-512:%0*d", 128, 0);
    EXPECT(pksc_proof_verify(out.bytes, out.len, fake_cid, pk) == 0,
           "proof_verify rejects tampered cid");

    /* Wrong pubkey is rejected. */
    uint8_t wrong_seed[32];
    memset(wrong_seed, 0x99, sizeof wrong_seed);
    uint8_t wrong_pk[32];
    pksc_ed25519_pubkey_from_seed(wrong_pk, wrong_seed);
    EXPECT(pksc_proof_verify(out.bytes, out.len, out.cid, wrong_pk) == 0,
           "proof_verify rejects wrong pubkey");

    pksc_proof_output_free(&out);
    free_members(members, 2);
}

static void test_proof_deterministic_across_runs(void) {
    pksc_member m1[2], m2[2];
    pksc_proof_input in1, in2;
    build_two_member_input(&in1, m1);
    build_two_member_input(&in2, m2);

    pksc_proof_output a = {0}, b = {0};
    pksc_proof_build(&in1, &a);
    pksc_proof_build(&in2, &b);

    int eq = (a.len == b.len) && a.bytes && b.bytes && memcmp(a.bytes, b.bytes, a.len) == 0;
    EXPECT(eq, "proof_build is deterministic across runs");
    EXPECT(a.cid && b.cid && strcmp(a.cid, b.cid) == 0, "proof_build cid is deterministic");

    pksc_proof_output_free(&a);
    pksc_proof_output_free(&b);
    free_members(m1, 2);
    free_members(m2, 2);
}

static void test_proof_changing_name_changes_cid(void) {
    pksc_member m1[2], m2[2];
    pksc_proof_input in1, in2;
    build_two_member_input(&in1, m1);
    build_two_member_input(&in2, m2);
    in2.name = "@other/name";

    pksc_proof_output a = {0}, b = {0};
    pksc_proof_build(&in1, &a);
    pksc_proof_build(&in2, &b);
    EXPECT(strcmp(a.cid, b.cid) != 0, "different name => different cid");
    pksc_proof_output_free(&a);
    pksc_proof_output_free(&b);
    free_members(m1, 2);
    free_members(m2, 2);
}

static void test_proof_empty_members(void) {
    pksc_proof_input in = {
        .name = "x", .version = "1",
        .binary_cid = NULL, .metadata = NULL, .n_metadata = 0,
        .members = NULL, .n_members = 0,
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = PKSC_FOUNDATION_V0_SEED,
    };
    pksc_proof_output out = {0};
    EXPECT(pksc_proof_build(&in, &out) == 0, "empty-members proof builds");
    EXPECT(out.bytes && out.bytes[0] == 0xA7, "empty-members signed-map head 0xA7");
    uint8_t pk[32];
    pksc_ed25519_pubkey_from_seed(pk, PKSC_FOUNDATION_V0_SEED);
    EXPECT(pksc_proof_verify(out.bytes, out.len, out.cid, pk) == 1,
           "empty-members proof verifies");
    pksc_proof_output_free(&out);
}

static void test_proof_binary_cid_changes_cid(void) {
    pksc_member m[1];
    m[0].key   = strdup("blake3-512:aa");
    m[0].bytes = (uint8_t *)strdup("data");
    m[0].len   = 4;

    pksc_proof_input in_no = {
        .name = "@test/cat", .version = "1.0.0",
        .binary_cid = NULL, .metadata = NULL, .n_metadata = 0,
        .members = m, .n_members = 1,
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = PKSC_FOUNDATION_V0_SEED,
    };
    pksc_proof_input in_yes = in_no;
    in_yes.binary_cid = "blake3-512:deadbeef";

    pksc_proof_output a = {0}, b = {0};
    pksc_proof_build(&in_no, &a);
    pksc_proof_build(&in_yes, &b);
    EXPECT(strcmp(a.cid, b.cid) != 0, "binary_cid present => different cid");

    /* Both verify under foundation pubkey. */
    uint8_t pk[32];
    pksc_ed25519_pubkey_from_seed(pk, PKSC_FOUNDATION_V0_SEED);
    EXPECT(pksc_proof_verify(a.bytes, a.len, a.cid, pk) == 1, "no-binary-cid verifies");
    EXPECT(pksc_proof_verify(b.bytes, b.len, b.cid, pk) == 1, "with-binary-cid verifies");

    pksc_proof_output_free(&a);
    pksc_proof_output_free(&b);
    free_members(m, 1);
}

/* ------------------------------------------------------------------- */
/* Cross-kit byte equivalence — pinned from the Rust reference output.  */
/*                                                                       */
/* Source: implementations/python/provekit-lift-py-tests/tests/         */
/*         test_proof_envelope.py RUST_FIXTURE_BYTES_HEX_FULL            */
/*                                                                       */
/* If this fails, the divergence is real cross-kit drift; do not paper   */
/* over it. The first divergent byte is reported for diagnostics.        */
/* ------------------------------------------------------------------- */

static const char RUST_FIXTURE_CID[] =
    "blake3-512:5ed1e1f705622ad52ae4683e3d12df5586d364d66bb3186f5be512415edf290"
    "844d74e73a2857cd858f37803e4b11fe5c7cba7884caa6b9ff847521ce32ea056";

/* Single contiguous hex string (split across C string-concat lines for
 * source readability; the C compiler concatenates adjacent string
 * literals at compile time so the byte content is one logical run). */
static const char RUST_FIXTURE_BYTES_HEX[] =
    "a7646b696e6467636174616c6f67646e616d656940746573742f636174667369676e65"
    "726d626c616b65332d3531323a6363676d656d62657273a26d626c616b65332d353132"
    "3a6161517b2268656c6c6f223a22776f726c64227d6d626c616b65332d3531323a6262"
    "537b22676f6f64627965223a22776f726c64227d6776657273696f6e65312e302e3069"
    "7369676e617475726558406a21dd428a54e22c82ca6d6125a7293c4a723786cb1840e8"
    "91cefa03e63246eb97ef13dab86b7b1469d67302fadc969cd88c92c29495d13c75fc02"
    "01a7263b066a6465636c6172656441747818323032362d30342d33305430303a30303a"
    "30302e3030305a";

static void test_cross_kit_byte_equivalence(void) {
    uint8_t *expected_bytes = NULL;
    size_t   expected_len   = 0;
    if (hex_decode(RUST_FIXTURE_BYTES_HEX, &expected_bytes, &expected_len) != 0) {
        ++g_failures;
        fprintf(stderr, "  FAIL  cross-kit hex_decode (test wiring bug)\n");
        return;
    }

    pksc_member members[2];
    pksc_proof_input in;
    build_two_member_input(&in, members);

    pksc_proof_output out = {0};
    if (pksc_proof_build(&in, &out) != 0) {
        ++g_failures;
        fprintf(stderr, "  FAIL  cross-kit proof_build returned -1\n");
        free(expected_bytes);
        free_members(members, 2);
        return;
    }

    if (out.len != expected_len) {
        ++g_failures;
        char *got_hex = bytes_to_hex(out.bytes, out.len);
        fprintf(stderr,
                "  FAIL  cross-kit byte length mismatch: got=%zu want=%zu\n"
                "    got hex: %s\n"
                "    want hex: %s\n",
                out.len, expected_len, got_hex, RUST_FIXTURE_BYTES_HEX);
        free(got_hex);
    } else if (memcmp(out.bytes, expected_bytes, out.len) != 0) {
        ++g_failures;
        size_t i;
        for (i = 0; i < out.len; i++) {
            if (out.bytes[i] != expected_bytes[i]) break;
        }
        fprintf(stderr,
                "  FAIL  cross-kit byte divergence at byte %zu: got=0x%02x want=0x%02x\n",
                i, out.bytes[i], expected_bytes[i]);
        char *got_hex = bytes_to_hex(out.bytes, out.len);
        fprintf(stderr, "    got hex:  %s\n", got_hex);
        fprintf(stderr, "    want hex: %s\n", RUST_FIXTURE_BYTES_HEX);
        free(got_hex);
    } else {
        ++g_passes;
        fprintf(stderr, "  ok    cross-kit byte equivalence (Rust reference)\n");
    }

    /* CID must match too. */
    if (out.cid && strcmp(out.cid, RUST_FIXTURE_CID) == 0) {
        ++g_passes;
        fprintf(stderr, "  ok    cross-kit CID equivalence (Rust reference)\n");
    } else {
        ++g_failures;
        fprintf(stderr, "  FAIL  cross-kit CID mismatch:\n    got:  %s\n    want: %s\n",
                out.cid ? out.cid : "(null)", RUST_FIXTURE_CID);
    }

    /* Reverse direction: Rust-produced bytes verify under our foundation pubkey. */
    uint8_t pk[32];
    pksc_ed25519_pubkey_from_seed(pk, PKSC_FOUNDATION_V0_SEED);
    EXPECT(pksc_proof_verify(expected_bytes, expected_len, RUST_FIXTURE_CID, pk) == 1,
           "Rust-produced bytes verify under our verifier (foundation pubkey)");

    pksc_proof_output_free(&out);
    free_members(members, 2);
    free(expected_bytes);
}

/* ------------------------------------------------------------------- */
/* Driver                                                               */
/* ------------------------------------------------------------------- */

int main(void) {
    fprintf(stderr, "provekit-self-contracts (C) test runner\n\n");

    fprintf(stderr, "== JCS ==\n");
    test_jcs_simple_object_sorts_keys();
    test_jcs_escapes_quotes_and_backslash();
    test_jcs_escapes_control_chars_lowercase_hex();
    test_jcs_unicode_passthrough();
    test_jcs_empty_object_and_array();
    test_jcs_nested_array_object();

    fprintf(stderr, "\n== CBOR ==\n");
    test_cbor_shortest_form_uint();
    test_cbor_tstr();

    fprintf(stderr, "\n== Ed25519 ==\n");
    test_ed25519_deterministic_signature();
    test_ed25519_round_trip();
    test_ed25519_string_form_round_trip();
    test_ed25519_foundation_v0_seed();

    fprintf(stderr, "\n== Proof envelope ==\n");
    test_proof_round_trip();
    test_proof_deterministic_across_runs();
    test_proof_changing_name_changes_cid();
    test_proof_empty_members();
    test_proof_binary_cid_changes_cid();

    fprintf(stderr, "\n== Cross-kit byte equivalence (Rust reference) ==\n");
    test_cross_kit_byte_equivalence();

    fprintf(stderr, "\n--------------------\n");
    fprintf(stderr, "passed: %d\n", g_passes);
    fprintf(stderr, "failed: %d\n", g_failures);
    return g_failures == 0 ? 0 : 1;
}
