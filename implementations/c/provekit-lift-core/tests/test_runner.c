#include "provekit/c_lift_core.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

static void assert_eq(const char *got, const char *want, const char *label) {
    if (strcmp(got, want) != 0) {
        fprintf(stderr, "FAIL: %s\nwant: %s\ngot:  %s\n", label, want, got);
        failures++;
    }
}

static void assert_int_eq(int got, int want, const char *label) {
    if (got != want) {
        fprintf(stderr, "FAIL: %s\nwant: %d\ngot:  %d\n", label, want, got);
        failures++;
    }
}

static void test_empty_result_json(void) {
    pk_c_lift_result *r = pk_c_lift_result_new();
    char *json = pk_c_lift_result_to_json(r);
    assert_eq(json,
        "{\"declarations\":[],\"callEdges\":[],\"diagnostics\":[],\"opacityReport\":[],\"refusals\":[]}",
        "empty result JSON");
    free(json);
    pk_c_lift_result_free(r);
}

static void test_populated_result_json(void) {
    pk_c_lift_result *r = pk_c_lift_result_new();
    char declaration[] = "{\"kind\":\"function\",\"name\":\"original\"}";
    char *json;

    assert_int_eq(pk_c_lift_result_add_declaration(r, declaration), 0,
        "add declaration copies mutable JSON");
    strcpy(declaration, "{\"kind\":\"mutated\"}");
    assert_int_eq(pk_c_lift_result_add_declaration(r, "{\"kind\":\"variable\",\"name\":\"second\"}"), 0,
        "add second declaration");
    assert_int_eq(pk_c_lift_result_add_call_edge(r, "{\"from\":\"main\",\"to\":\"helper\"}"), 0,
        "add call edge");
    assert_int_eq(pk_c_lift_result_add_diagnostic(r, "{\"severity\":\"warning\"}"), 0,
        "add diagnostic");
    assert_int_eq(pk_c_lift_result_add_opacity(r, "{\"symbol\":\"opaque\"}"), 0,
        "add opacity");
    assert_int_eq(pk_c_lift_result_add_refusal(r, "{\"reason\":\"unsupported\"}"), 0,
        "add refusal");

    json = pk_c_lift_result_to_json(r);
    assert_eq(json,
        "{\"declarations\":[{\"kind\":\"function\",\"name\":\"original\"},{\"kind\":\"variable\",\"name\":\"second\"}],\"callEdges\":[{\"from\":\"main\",\"to\":\"helper\"}],\"diagnostics\":[{\"severity\":\"warning\"}],\"opacityReport\":[{\"symbol\":\"opaque\"}],\"refusals\":[{\"reason\":\"unsupported\"}]}",
        "populated result JSON");
    free(json);
    pk_c_lift_result_free(r);
}

static void test_array_growth_overflow_is_rejected(void) {
    pk_c_lift_result result = {0};
    int rc;

    result.declarations.len = (SIZE_MAX / 2) + 1;
    result.declarations.cap = (SIZE_MAX / 2) + 1;
    rc = pk_c_lift_result_add_declaration(&result, "{\"kind\":\"overflow\"}");
    if (rc == 0) {
        fprintf(stderr, "FAIL: array growth overflow should be rejected\n");
        failures++;
    }
}

int main(void) {
    test_empty_result_json();
    test_populated_result_json();
    test_array_growth_overflow_is_rejected();
    if (failures != 0) {
        fprintf(stderr, "%d failures\n", failures);
        return 1;
    }
    puts("provekit-lift-core tests passed");
    return 0;
}
