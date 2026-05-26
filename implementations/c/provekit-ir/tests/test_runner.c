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

static char *emit_formula(pk_formula *f) {
    pk_buffer *buf = pk_buffer_new();
    pk_emit_formula(buf, f);
    char *s = pk_buffer_steal(buf);
    pk_buffer_free(buf);
    return s;
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
    printf("Done.\n");
    return failures == 0 ? 0 : 1;
}
