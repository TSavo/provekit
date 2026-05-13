/* SPDX-License-Identifier: Apache-2.0 */

#include "provekit/ir.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ----------------------------------------------------------------------- */
/* Helpers                                                                 */
/* ----------------------------------------------------------------------- */

static int failures = 0;

static void assert_eq_str(const char *a, const char *b, const char *msg) {
    if (strcmp(a, b) != 0) {
        fprintf(stderr, "FAIL: %s\n  expected: %s\n  got:      %s\n", msg, b, a);
        failures++;
        /* Don't abort immediately so we can see all failures. */
    }
}

static void assert_true(int condition, const char *msg) {
    if (!condition) {
        fprintf(stderr, "FAIL: %s\n", msg);
        failures++;
    }
}

static int cicp_blast_radius_input_closure_is_closed(const char *canonical_json);

static char *emit_formula(pk_formula *f) {
    pk_buffer *buf = pk_buffer_new();
    pk_emit_formula(buf, f);
    char *s = pk_buffer_steal(buf);
    pk_buffer_free(buf);
    return s;
}

static int find_json_string_field(const char *json, const char *key,
                                  const char **value, size_t *value_len) {
    char needle[128];
    int n = snprintf(needle, sizeof(needle), "\"%s\":\"", key);
    if (n < 0 || (size_t)n >= sizeof(needle)) return 0;

    const char *start = strstr(json, needle);
    if (!start) return 0;
    start += (size_t)n;

    const char *end = strchr(start, '"');
    if (!end) return 0;
    *value = start;
    *value_len = (size_t)(end - start);
    return 1;
}

static int input_cids_contains(const char *json, const char *value, size_t value_len) {
    const char *start = strstr(json, "\"inputCids\":[");
    if (!start) return 0;

    const char *end = strchr(start, ']');
    if (!end) return 0;

    const char *cursor = start;
    while ((cursor = strstr(cursor, "\"blake3-512:")) && cursor < end) {
        cursor++;
        const char *cid_start = cursor;
        const char *cid_end = strchr(cid_start, '"');
        if (!cid_end || cid_end > end) return 0;

        size_t cid_len = (size_t)(cid_end - cid_start);
        if (cid_len == value_len && strncmp(cid_start, value, value_len) == 0) {
            return 1;
        }
        cursor = cid_end + 1;
    }
    return 0;
}

static int cicp_blast_radius_input_closure_is_closed(const char *canonical_json) {
    const char *required_fields[] = {
        "commandCid",
        "jobDefinitionCid",
        "policyCid",
        "protocolCatalogCid",
        "runnerIdentityCid",
        "sourceClosureCid",
    };

    for (size_t i = 0; i < sizeof(required_fields) / sizeof(required_fields[0]); i++) {
        const char *value = NULL;
        size_t value_len = 0;
        if (!find_json_string_field(canonical_json, required_fields[i], &value, &value_len)) {
            return 0;
        }
        if (!input_cids_contains(canonical_json, value, value_len)) {
            return 0;
        }
    }
    return 1;
}

/* ----------------------------------------------------------------------- */
/* Test: eq atomic with ctor (matches Rust pinned test)                    */
/* ----------------------------------------------------------------------- */

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

    assert_eq_str(got, expected, "eq atomic JCS");

    free(got);
    pk_formula_free(f);
}

/* ----------------------------------------------------------------------- */
/* Test: bounded loop quantifier (matches Rust pinned test)                */
/* ----------------------------------------------------------------------- */

static void test_pattern1_bounded_loop_jcs(void) {
    /* The C kit takes ownership of every term/sort handed to a constructor;
     * there is no ref-counting. Construct fresh terms per atomic so the
     * single ownership chain rooted at `q` does not double-free shared
     * leaves when `pk_formula_free(q)` walks the tree. */
    pk_term *x1 = pk_term_var_new("x");
    pk_term *zero1 = pk_term_const_int(0, pk_sort_primitive("Int"));
    pk_term *gte_args[] = { x1, zero1 };
    pk_formula *lower = pk_formula_atomic_new("≥", gte_args, 2);

    pk_term *x2 = pk_term_var_new("x");
    pk_term *hundred = pk_term_const_int(100, pk_sort_primitive("Int"));
    pk_term *lt_args[] = { x2, hundred };
    pk_formula *upper = pk_formula_atomic_new("<", lt_args, 2);

    pk_formula *conj_ops[] = { lower, upper };
    pk_formula *antecedent = pk_formula_connective_new("and", conj_ops, 2);

    pk_term *x3 = pk_term_var_new("x");
    pk_term *zero2 = pk_term_const_int(0, pk_sort_primitive("Int"));
    pk_term *gte2_args[] = { x3, zero2 };
    pk_formula *inner = pk_formula_atomic_new("≥", gte2_args, 2);

    pk_formula *impl_ops[] = { antecedent, inner };
    pk_formula *body = pk_formula_connective_new("implies", impl_ops, 2);

    pk_formula *q = pk_formula_quantifier_new("forall", "x", pk_sort_primitive("Int"), body);

    char *got = emit_formula(q);

    const char *expected =
        "{\"body\":{\"kind\":\"implies\",\"operands\":[{\"kind\":\"and\",\"operands\":[{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],\"kind\":\"atomic\",\"name\":\"≥\"},"
        "{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":100}],"
        "\"kind\":\"atomic\",\"name\":\"<\"}]},{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],\"kind\":\"atomic\",\"name\":\"≥\"}]},"
        "\"kind\":\"forall\",\"name\":\"x\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";

    assert_eq_str(got, expected, "pattern1 bounded loop JCS");

    free(got);
    pk_formula_free(q);
}

/* ----------------------------------------------------------------------- */
/* Test: contract declaration                                              */
/* ----------------------------------------------------------------------- */

static void test_contract_decl_jcs(void) {
    pk_term *x = pk_term_var_new("x");
    pk_term *zero = pk_term_const_int(0, pk_sort_primitive("Int"));
    pk_term *args[] = { x, zero };
    pk_formula *pre = pk_formula_atomic_new("≥", args, 2);

    pk_decl *d = pk_decl_contract_new("parseInt", "out", pre, NULL, NULL);
    pk_buffer *buf = pk_buffer_new();
    pk_emit_decl(buf, d);
    char *got = pk_buffer_steal(buf);
    pk_buffer_free(buf);

    const char *expected =
        "{\"kind\":\"contract\",\"name\":\"parseInt\",\"outBinding\":\"out\","
        "\"pre\":{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],"
        "\"kind\":\"atomic\",\"name\":\"≥\"}}";

    assert_eq_str(got, expected, "contract decl JCS");

    free(got);
    pk_decl_free(d);
}

/* ----------------------------------------------------------------------- */
/* Test: bridge declaration                                                */
/* ----------------------------------------------------------------------- */

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

    /* JCS sorts keys by code point, so `notes` (n-o) lands between `name`
     * (n-a) and `sourceContractCid` (s). This expected string mirrors the
     * canonical-on-disk emit order, not any spec-listing order. */
    const char *expected =
        "{\"kind\":\"bridge\",\"name\":\"myBridge\",\"notes\":\"some notes\","
        "\"sourceContractCid\":\"bafySource\",\"sourceLayer\":\"c-kit\","
        "\"sourceSymbol\":\"source\",\"targetContractCid\":\"bafyTarget\","
        "\"targetLayer\":\"coq\",\"targetProofCid\":\"bafyProof\"}";

    assert_eq_str(got, expected, "bridge decl JCS");

    free(got);
    pk_decl_free(d);
}

/* ----------------------------------------------------------------------- */
/* Test: hash matches Rust pinned value                                    */
/* ----------------------------------------------------------------------- */

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

    assert_eq_str(hash, expected_hash, "eq atomic hash");

    free(jcs);
    free(hash);
    pk_formula_free(f);
}

/* ----------------------------------------------------------------------- */
/* Test: CICP golden vectors                                               */
/* ----------------------------------------------------------------------- */

typedef struct {
    const char *name;
    const char *canonical_body;
    const char *expected_cid;
} cicp_vector;

static void test_cicp_golden_vector_cids(void) {
    const cicp_vector vectors[] = {
        {
            "blast-radius-rust-kit",
            "{\"commandCid\":\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"fixtureCids\":[\"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888\"],\"generatedInputCids\":[\"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777\"],\"inputCids\":[\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444\",\"blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47\",\"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f\",\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666\",\"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777\",\"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888\",\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"],\"jobDefinitionCid\":\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"jobKey\":\"provekit/conformance/rust\",\"kind\":\"CIBlastRadius\",\"lockfileCids\":[\"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666\"],\"nondeterminism\":{\"clock\":\"forbidden\",\"network\":\"forbidden\",\"randomness\":\"forbidden\",\"secrets\":\"forbidden\"},\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"protocolCatalogCid\":\"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f\",\"relevantSpecCids\":[\"blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47\"],\"runnerIdentityCid\":\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"schemaVersion\":\"1\",\"sourceClosureCid\":\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"subject\":\"rust\",\"subjectKind\":\"kit\",\"toolchainCids\":[\"blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444\"]}",
            "blake3-512:b46ed4acaa333e1c67d34435914543235529eee7beb8e70ca5075fd5d4417a3a5685625532e3631f81c65d748e6d0b354c158b5f8043dc70aa1eb654a4ee9550"
        },
        {
            "blast-radius-rust-kit-next-catalog",
            "{\"commandCid\":\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"fixtureCids\":[\"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888\"],\"generatedInputCids\":[\"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777\"],\"inputCids\":[\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444\",\"blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47\",\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666\",\"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777\",\"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888\",\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"],\"jobDefinitionCid\":\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"jobKey\":\"provekit/conformance/rust\",\"kind\":\"CIBlastRadius\",\"lockfileCids\":[\"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666\"],\"nondeterminism\":{\"clock\":\"forbidden\",\"network\":\"forbidden\",\"randomness\":\"forbidden\",\"secrets\":\"forbidden\"},\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"protocolCatalogCid\":\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",\"relevantSpecCids\":[\"blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47\"],\"runnerIdentityCid\":\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"schemaVersion\":\"1\",\"sourceClosureCid\":\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"subject\":\"rust\",\"subjectKind\":\"kit\",\"toolchainCids\":[\"blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444\"]}",
            "blake3-512:add810e8496aa2b72de4db0e15f789a76e092b13dd0233d7e0e78c4155d17916f5a6104cfa011af734eec760f1c627bd13357cd64d1670407686d9136ac1f7b8"
        },
        {
            "job-result-pass",
            "{\"blastRadiusCid\":\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"finishedAt\":\"2026-05-07T00:01:00Z\",\"inputCids\":[\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\",\"blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\"],\"jobKey\":\"provekit/conformance/rust\",\"kind\":\"CIJobResultBodyClaim\",\"logCid\":\"blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\",\"outputCid\":\"blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\",\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"producer\":{\"kind\":\"ci-runner\",\"name\":\"github-actions\",\"version\":\"cicp-vector\"},\"result\":\"pass\",\"runnerIdentityCid\":\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"schemaVersion\":\"1\",\"startedAt\":\"2026-05-07T00:00:00Z\"}",
            "blake3-512:1c426f1cc560a02623931abd9349b855150f64507d1f5a231312fb0c017fe38a9224f60dc611087aaffb225411cbc5dbe936a2b2a469546387a5f304052cc141"
        },
        {
            "reuse-identical",
            "{\"bridgeWitnessCids\":[],\"currentBlastRadiusCid\":\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"inputCids\":[\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"],\"jobKey\":\"provekit/conformance/rust\",\"kind\":\"CIReuseBodyClaim\",\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"previousBlastRadiusCid\":\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"previousResultWitnessCid\":\"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\",\"reuseReason\":\"identical-input-closure\",\"schemaVersion\":\"1\"}",
            "blake3-512:4236da5414741b5c24e2347e5308ee60adf764ccf741f97865f0e149f2869547bde3e3e5d8d5b43e7be0389fc97ec2e4bcee37bfe5797f907db84337e921c961"
        },
        {
            "reuse-bridged-by-evolution",
            "{\"bridgeWitnessCids\":[\"blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5\"],\"currentBlastRadiusCid\":\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",\"inputCids\":[\"blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5\",\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",\"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"],\"jobKey\":\"provekit/conformance/java\",\"kind\":\"CIReuseBodyClaim\",\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"previousBlastRadiusCid\":\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"previousResultWitnessCid\":\"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\",\"reuseReason\":\"bridged-by-evolution\",\"schemaVersion\":\"1\"}",
            "blake3-512:1c83f9e79533e2c0254ec66e76e1478e332fc6c72ea751566f3698976c7050269a45cb149af0f22c931edb3d5724b7df112fe89df93c47c750b0718cc0b16dbd"
        },
        {
            "impact-protocol-extension-only",
            "{\"baseStateCid\":\"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\",\"candidateStateCid\":\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",\"changedBlastRadiusCids\":[\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"],\"inputCids\":[\"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\",\"blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5\",\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",\"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"],\"kind\":\"CIImpactBodyClaim\",\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"protocolEvolutionWitnessCids\":[\"blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5\"],\"refusalCids\":[],\"requiredJobKeys\":[\"provekit/conformance/rust\"],\"reusableWitnessCids\":[\"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"],\"schemaVersion\":\"1\",\"unchangedBlastRadiusCids\":[\"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"]}",
            "blake3-512:53f2eed2f4b6b87f62ae3348d5686ec9177c140965110abacaa5a68330fc27dbc6bca42673798bc0ab7f1e9a665b74aa4fa39900fd10ae0258a5a0efedcd817e"
        },
    };

    for (size_t i = 0; i < sizeof(vectors) / sizeof(vectors[0]); i++) {
        char msg[128];
        char *hash = pk_hash_jcs(vectors[i].canonical_body);
        snprintf(msg, sizeof(msg), "CICP vector %s CID", vectors[i].name);
        assert_eq_str(hash, vectors[i].expected_cid, msg);
        free(hash);
    }
}

static void test_cicp_invalid_blast_radius_fails_closed(void) {
    const char *valid_blast_radius =
        "{\"commandCid\":\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"fixtureCids\":[\"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888\"],\"generatedInputCids\":[\"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777\"],\"inputCids\":[\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444\",\"blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47\",\"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f\",\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666\",\"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777\",\"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888\",\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"],\"jobDefinitionCid\":\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"jobKey\":\"provekit/conformance/rust\",\"kind\":\"CIBlastRadius\",\"lockfileCids\":[\"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666\"],\"nondeterminism\":{\"clock\":\"forbidden\",\"network\":\"forbidden\",\"randomness\":\"forbidden\",\"secrets\":\"forbidden\"},\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"protocolCatalogCid\":\"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f\",\"relevantSpecCids\":[\"blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47\"],\"runnerIdentityCid\":\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"schemaVersion\":\"1\",\"sourceClosureCid\":\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"subject\":\"rust\",\"subjectKind\":\"kit\",\"toolchainCids\":[\"blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444\"]}";
    const char *invalid_blast_radius =
        "{\"commandCid\":\"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222\",\"fixtureCids\":[],\"generatedInputCids\":[],\"inputCids\":[\"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f\"],\"jobDefinitionCid\":\"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111\",\"jobKey\":\"provekit/conformance/rust\",\"kind\":\"CIBlastRadius\",\"lockfileCids\":[],\"nondeterminism\":{\"clock\":\"forbidden\",\"network\":\"forbidden\",\"randomness\":\"forbidden\",\"secrets\":\"forbidden\"},\"policyCid\":\"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"protocolCatalogCid\":\"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f\",\"relevantSpecCids\":[],\"runnerIdentityCid\":\"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333\",\"schemaVersion\":\"1\",\"sourceClosureCid\":\"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555\",\"subject\":\"rust\",\"subjectKind\":\"kit\",\"toolchainCids\":[]}";

    assert_true(cicp_blast_radius_input_closure_is_closed(valid_blast_radius),
                "valid blast-radius vector has closed inputCids");
    assert_true(!cicp_blast_radius_input_closure_is_closed(invalid_blast_radius),
                "invalid blast-radius vector fails closed on missing inputCids dependency");
}

/* ----------------------------------------------------------------------- */
/* Main                                                                    */
/* ----------------------------------------------------------------------- */

int main(void) {
    printf("Running C kit tests...\n"); fflush(stdout);
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
    test_cicp_golden_vector_cids();
    printf("  test_cicp_golden_vector_cids passed\n"); fflush(stdout);
    test_cicp_invalid_blast_radius_fails_closed();
    printf("  test_cicp_invalid_blast_radius_fails_closed passed\n"); fflush(stdout);
    printf("Done.\n");
    return failures == 0 ? 0 : 1;
}
