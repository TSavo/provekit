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

static void test_opacity_and_refusal_are_separate(void) {
    pk_c_lift_result *r = pk_c_lift_result_new();
    assert_int_eq(
        pk_c_lift_result_add_opacity_entry(
            r,
            "unexpanded-macro",
            "fixture.c",
            7,
            5,
            "macro body unavailable",
            "sparse"),
        0,
        "add structured opacity");
    assert_int_eq(
        pk_c_lift_result_add_refusal_entry(
            r,
            "unsupported-lock-transfer",
            "fixture.c",
            9,
            3,
            "lockdep",
            "lock released through function pointer"),
        0,
        "add structured refusal");
    char *json = pk_c_lift_result_to_json(r);
    const char *want =
        "{\"declarations\":[],\"callEdges\":[],\"diagnostics\":[],"
        "\"opacityReport\":[{\"affectedSurface\":\"sparse\",\"kind\":\"unexpanded-macro\","
        "\"locus\":{\"column\":5,\"line\":7,\"path\":\"fixture.c\"},\"reason\":\"macro body unavailable\"}],"
        "\"refusals\":[{\"kind\":\"unsupported-lock-transfer\","
        "\"locus\":{\"column\":3,\"line\":9,\"path\":\"fixture.c\"},"
        "\"reason\":\"lock released through function pointer\",\"surface\":\"lockdep\"}]}";
    assert_eq(json, want, "opacity and refusal separation");
    free(json);
    pk_c_lift_result_free(r);
}

static void test_structured_helpers_escape_json_strings(void) {
    pk_c_lift_result *r = pk_c_lift_result_new();
    char *json;
    const char *want =
        "{\"declarations\":[],\"callEdges\":[],\"diagnostics\":[],"
        "\"opacityReport\":[{\"affectedSurface\":\"surface\\fpage\","
        "\"kind\":\"quote\\\"slash\\\\kind\","
        "\"locus\":{\"column\":11,\"line\":10,\"path\":\"path\\nfile.c\"},"
        "\"reason\":\"tab\\tback\\bctrl\\u0001end\"}],"
        "\"refusals\":[{\"kind\":\"ref\\\"kind\","
        "\"locus\":{\"column\":13,\"line\":12,\"path\":\"ref\\\\path.c\"},"
        "\"reason\":\"line\\nform\\fctrl\\u0001done\","
        "\"surface\":\"surface\\tback\\b\"}]}";

    assert_int_eq(
        pk_c_lift_result_add_opacity_entry(
            r,
            "quote\"slash\\kind",
            "path\nfile.c",
            10,
            11,
            "tab\tback\bctrl\x01" "end",
            "surface\fpage"),
        0,
        "add escaping opacity");
    assert_int_eq(
        pk_c_lift_result_add_refusal_entry(
            r,
            "ref\"kind",
            "ref\\path.c",
            12,
            13,
            "surface\tback\b",
            "line\nform\fctrl\x01" "done"),
        0,
        "add escaping refusal");

    json = pk_c_lift_result_to_json(r);
    assert_eq(json, want, "structured helper JSON escaping");
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

static void test_parse_functions_and_macros(void) {
    const char *source =
        "int helper(int x) { return x + 1; }\n"
        "int compute(int y) {\n"
        "    WARN_ON(y < 0);\n"
        "    return helper(y);\n"
        "}\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_functions != 2) {
        fprintf(stderr, "FAIL: expected 2 functions, got %zu\n", facts->n_functions);
        failures++;
    }
    assert_eq(facts->functions[0].name, "helper", "first function name");
    assert_eq(facts->functions[1].name, "compute", "second function name");
    if (facts->n_macro_calls != 1) {
        fprintf(stderr, "FAIL: expected 1 macro call, got %zu\n", facts->n_macro_calls);
        failures++;
    } else {
        assert_eq(facts->macro_calls[0].name, "WARN_ON", "macro call name");
        assert_eq(facts->macro_calls[0].enclosing_function, "compute", "macro enclosing function");
    }
    if (facts->n_call_sites != 1) {
        fprintf(stderr, "FAIL: expected 1 call site, got %zu\n", facts->n_call_sites);
        failures++;
    } else {
        assert_eq(facts->call_sites[0].callee, "helper", "call callee");
        assert_eq(facts->call_sites[0].caller, "compute", "call caller");
    }
    pk_c_source_facts_free(facts);
}

static void test_parse_same_line_function_body_call(void) {
    const char *source =
        "int helper(int x) { return x + 1; }\n"
        "int compute(int y) { return helper(y); }\n"
        "int checker(int y) { WARN_ON(y < 0); }\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_call_sites != 1) {
        fprintf(stderr, "FAIL: expected 1 same-line call site, got %zu\n", facts->n_call_sites);
        failures++;
    } else {
        assert_eq(facts->call_sites[0].caller, "compute", "same-line call caller");
        assert_eq(facts->call_sites[0].callee, "helper", "same-line call callee");
        assert_int_eq(facts->call_sites[0].locus.column, 29, "same-line call column");
    }
    if (facts->n_macro_calls != 1) {
        fprintf(stderr, "FAIL: expected 1 same-line macro call, got %zu\n", facts->n_macro_calls);
        failures++;
    } else {
        assert_eq(facts->macro_calls[0].name, "WARN_ON", "same-line macro name");
        assert_eq(facts->macro_calls[0].enclosing_function, "checker", "same-line macro enclosing function");
        assert_int_eq(facts->macro_calls[0].locus.column, 22, "same-line macro column");
    }
    pk_c_source_facts_free(facts);
}

static void test_parse_recursive_same_line_function_body_call(void) {
    const char *source = "int fact(int n) { return n ? fact(n - 1) : 1; }\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_call_sites != 1) {
        fprintf(stderr, "FAIL: expected 1 recursive call site, got %zu\n", facts->n_call_sites);
        failures++;
    } else {
        assert_eq(facts->call_sites[0].caller, "fact", "recursive call caller");
        assert_eq(facts->call_sites[0].callee, "fact", "recursive call callee");
    }
    pk_c_source_facts_free(facts);
}

static void test_parse_ignores_comments_and_strings(void) {
    const char *source =
        "int quiet(void) {\n"
        "    /* WARN_ON(noise); */\n"
        "    const char *text = \"BUG_ON(noise)\";\n"
        "    return 0;\n"
        "}\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_macro_calls != 0) {
        fprintf(stderr, "FAIL: expected 0 macro calls from comments/strings, got %zu\n",
            facts->n_macro_calls);
        failures++;
    }
    pk_c_source_facts_free(facts);
}

static void test_parse_sparse_annotations(void) {
    const char *source =
        "#define __user\n"
        "#define __must_hold(x)\n"
        "int copy_name(char __user *buf, int len)\n"
        "{\n"
        "    return len;\n"
        "}\n"
        "void update_locked(int *state) __must_hold(lock)\n"
        "{\n"
        "    *state = 1;\n"
        "}\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_sparse_annotations != 2) {
        fprintf(stderr, "FAIL: expected 2 sparse annotations, got %zu\n",
            facts->n_sparse_annotations);
        failures++;
    } else {
        assert_eq(facts->sparse_annotations[0].name, "__user", "first sparse annotation name");
        assert_eq(facts->sparse_annotations[0].enclosing_function, "copy_name",
            "first sparse annotation function");
        assert_eq(facts->sparse_annotations[1].name, "__must_hold",
            "second sparse annotation name");
        assert_eq(facts->sparse_annotations[1].argument_text, "lock",
            "second sparse annotation argument");
        assert_eq(facts->sparse_annotations[1].enclosing_function, "update_locked",
            "second sparse annotation function");
    }
    pk_c_source_facts_free(facts);
}

static void test_contract_annotation_ignores_string_literal_marker(void) {
    const char *source =
        "const char *marker = \"//provekit:contract\";\n"
        "int accidental(void) { return 0; }\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_functions != 1) {
        fprintf(stderr, "FAIL: expected 1 function, got %zu\n", facts->n_functions);
        failures++;
    } else {
        assert_int_eq(facts->functions[0].has_contract_annotation, 0,
            "string literal contract marker should not annotate function");
    }
    pk_c_source_facts_free(facts);
}

int main(void) {
    test_empty_result_json();
    test_populated_result_json();
    test_opacity_and_refusal_are_separate();
    test_structured_helpers_escape_json_strings();
    test_array_growth_overflow_is_rejected();
    test_parse_functions_and_macros();
    test_parse_same_line_function_body_call();
    test_parse_recursive_same_line_function_body_call();
    test_parse_ignores_comments_and_strings();
    test_parse_sparse_annotations();
    test_contract_annotation_ignores_string_literal_marker();
    if (failures != 0) {
        fprintf(stderr, "%d failures\n", failures);
        return 1;
    }
    puts("provekit-lift-core tests passed");
    return 0;
}
