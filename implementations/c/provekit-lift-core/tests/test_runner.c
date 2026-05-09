#include "provekit/c_lift_core.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static int failures = 0;

static void assert_eq(const char *got, const char *want, const char *label) {
    if (got == NULL || want == NULL) {
        if (got != want) {
            fprintf(stderr, "FAIL: %s\nwant: %s\ngot:  %s\n",
                label, want ? want : "(null)", got ? got : "(null)");
            failures++;
        }
        return;
    }
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
    } else {
        assert_eq(facts->functions[0].name, "helper", "first function name");
        assert_eq(facts->functions[1].name, "compute", "second function name");
    }
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

static void test_parse_nested_macro_arguments(void) {
    const char *source =
        "int helper(int x) { return x; }\n"
        "int compute(int y) {\n"
        "    WARN_ON(helper(y));\n"
        "    return y;\n"
        "}\n";
    pk_c_source_facts *facts = pk_c_parse_source("fixture.c", source);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    if (facts->n_macro_calls != 1) {
        fprintf(stderr, "FAIL: expected 1 nested macro call, got %zu\n", facts->n_macro_calls);
        failures++;
    } else {
        assert_eq(facts->macro_calls[0].name, "WARN_ON", "nested macro call name");
        assert_eq(facts->macro_calls[0].argument_text, "helper(y)",
            "nested macro argument text");
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

static void test_contract_annotation_ignores_block_comment_marker(void) {
    const char *source =
        "/*\n"
        " //provekit:contract\n"
        "*/\n"
        "int not_annotated(void) { return 0; }\n";
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
            "block comment contract marker should not annotate function");
    }
    pk_c_source_facts_free(facts);
}

static void test_parse_with_regex_options_records_backend(void) {
    pk_c_parse_options options = {0};
    const char *source = "int one(void) { return 1; }\n";
    pk_c_source_facts *facts;

    options.backend = PK_C_PARSE_BACKEND_REGEX;
    facts = pk_c_parse_source_with_options("fixture.c", source, &options);
    if (!facts) {
        fprintf(stderr, "FAIL: parse returned null\n");
        failures++;
        return;
    }
    assert_eq(facts->parser_backend, "regex", "regex backend provenance");
    pk_c_source_facts_free(facts);
}

static void test_compile_context_from_kbuild_command_filters_for_clang(void) {
    const char *line =
        "cmd_kernel/foo.o := gcc -Wp,-MMD,kernel/.foo.o.d -nostdinc "
        "-I./arch/x86/include -I ./include -include ./include/linux/compiler_types.h "
        "-D__KERNEL__ -DCONFIG_SMP=1 --target=x86_64-linux-gnu "
        "-c -o kernel/foo.o kernel/foo.c";
    pk_c_compile_context *context = pk_c_compile_context_from_kbuild_cmdline("kernel/foo.c", line);
    const char *want[] = {
        "-x", "c", "-std=gnu11", "-nostdinc",
        "-I./arch/x86/include", "-I", "./include",
        "-include", "./include/linux/compiler_types.h",
        "-D__KERNEL__", "-DCONFIG_SMP=1",
        "--target=x86_64-linux-gnu"
    };

    if (!context) {
        fprintf(stderr, "FAIL: compile context returned null\n");
        failures++;
        return;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "kbuild clang arg");
        }
    }
    assert_eq(context->compile_command,
        "gcc -Wp,-MMD,kernel/.foo.o.d -nostdinc -I./arch/x86/include -I ./include -include ./include/linux/compiler_types.h -D__KERNEL__ -DCONFIG_SMP=1 --target=x86_64-linux-gnu -c -o kernel/foo.o kernel/foo.c",
        "kbuild compile command provenance");
    assert_eq(context->target_triple, "x86_64-linux-gnu", "kbuild target triple");
    if (context->extraction_result != NULL &&
        context->extraction_result->opacity_report.len != 0) {
        fprintf(stderr, "FAIL: expected no opacity for supported compile command\n");
        failures++;
    }
    pk_c_compile_context_free(context);
}

static void test_compile_context_from_quoted_command_preserves_shell_words(void) {
    const char *line =
        "clang -DNAME='value with space' -I\"quoted include\" -c quoted.c";
    pk_c_compile_context *context = pk_c_compile_context_from_command("quoted.c", line);
    const char *want[] = {
        "-x", "c", "-std=gnu11",
        "-DNAME=value with space",
        "-Iquoted include"
    };

    if (!context) {
        fprintf(stderr, "FAIL: quoted compile context returned null\n");
        failures++;
        return;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu quoted clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "quoted clang arg");
        }
    }
    pk_c_compile_context_free(context);
}

static void test_compile_context_reports_dropped_gcc_plugin_flags(void) {
    const char *line =
        "gcc -fplugin=scripts/gcc-plugins/stackleak_plugin.so "
        "-fplugin-arg-stackleak_plugin-track-min-size=100 -DOK=1 -c plugin.c";
    pk_c_compile_context *context = pk_c_compile_context_from_command("plugin.c", line);
    const char *want[] = {
        "-x", "c", "-std=gnu11", "-DOK=1"
    };

    if (!context) {
        fprintf(stderr, "FAIL: plugin compile context returned null\n");
        failures++;
        return;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu plugin-filtered clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "plugin-filtered clang arg");
        }
    }
    if (context->extraction_result == NULL ||
        context->extraction_result->opacity_report.len != 2 ||
        strstr(context->extraction_result->opacity_report.items[0], "compile-arg-dropped") == NULL ||
        strstr(context->extraction_result->opacity_report.items[1], "compile-arg-dropped") == NULL) {
        fprintf(stderr, "FAIL: expected opacity for dropped GCC plugin flags\n");
        failures++;
    }
    pk_c_compile_context_free(context);
}

static int write_text_file(const char *path, const char *contents) {
    FILE *f = fopen(path, "wb");

    if (f == NULL) {
        return -1;
    }
    if (fputs(contents, f) < 0) {
        fclose(f);
        return -1;
    }
    return fclose(f);
}

static int make_dir_path(const char *root, const char *relative, char *out, size_t out_len) {
    int written = snprintf(out, out_len, "%s/%s", root, relative);

    if (written < 0 || (size_t)written >= out_len) {
        return -1;
    }
    return mkdir(out, 0700);
}

static void test_kernel_compile_context_resolves_compile_commands_json(void) {
    char tmp[] = "/tmp/provekit-core-ccdb-XXXXXX";
    char path[512];
    char json[2048];
    char include_arg[768];
    pk_c_compile_context *context;
    const char *want[6];

    if (mkdtemp(tmp) == NULL) {
        fprintf(stderr, "FAIL: mkdtemp failed for compile_commands fixture\n");
        failures++;
        return;
    }
    (void)snprintf(path, sizeof(path), "%s/compile_commands.json", tmp);
    (void)snprintf(json,
        sizeof(json),
        "[{\"directory\":\"%s\",\"file\":\"kernel/sched/core.c\","
        "\"command\":\"clang -D__KERNEL__ -Iinclude --target=x86_64-linux-gnu "
        "-c -o kernel/sched/core.o kernel/sched/core.c\"}]",
        tmp);
    if (write_text_file(path, json) != 0) {
        fprintf(stderr, "FAIL: could not write compile_commands fixture\n");
        failures++;
        (void)rmdir(tmp);
        return;
    }
    (void)snprintf(include_arg, sizeof(include_arg), "-I%s/include", tmp);
    want[0] = "-x";
    want[1] = "c";
    want[2] = "-std=gnu11";
    want[3] = "-D__KERNEL__";
    want[4] = include_arg;
    want[5] = "--target=x86_64-linux-gnu";

    context = pk_c_compile_context_resolve_kernel(tmp, "kernel/sched/core.c");
    if (!context) {
        fprintf(stderr, "FAIL: kernel compile_commands resolver returned null\n");
        failures++;
        (void)unlink(path);
        (void)rmdir(tmp);
        return;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu compile_commands clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "compile_commands resolver clang arg");
        }
    }
    assert_eq(context->target_triple, "x86_64-linux-gnu",
        "compile_commands resolver target triple");
    pk_c_compile_context_free(context);
    (void)unlink(path);
    (void)rmdir(tmp);
}

static void test_kernel_compile_context_resolves_compile_commands_arguments(void) {
    char tmp[] = "/tmp/provekit-core-ccargs-XXXXXX";
    char path[512];
    char json[1024];
    char include_arg[768];
    pk_c_compile_context *context;
    const char *want[6];

    if (mkdtemp(tmp) == NULL) {
        fprintf(stderr, "FAIL: mkdtemp failed for compile_commands arguments fixture\n");
        failures++;
        return;
    }
    (void)snprintf(path, sizeof(path), "%s/compile_commands.json", tmp);
    (void)snprintf(json,
        sizeof(json),
        "[{\"directory\":\"%s\",\"file\":\"kernel/irq/chip.c\","
        "\"arguments\":[\"clang\",\"-D__KERNEL__\",\"-Iinclude\","
        "\"--target=riscv64-linux-gnu\",\"-c\",\"kernel/irq/chip.c\"]}]",
        tmp);
    if (write_text_file(path, json) != 0) {
        fprintf(stderr, "FAIL: could not write compile_commands arguments fixture\n");
        failures++;
        (void)rmdir(tmp);
        return;
    }
    (void)snprintf(include_arg, sizeof(include_arg), "-I%s/include", tmp);
    want[0] = "-x";
    want[1] = "c";
    want[2] = "-std=gnu11";
    want[3] = "-D__KERNEL__";
    want[4] = include_arg;
    want[5] = "--target=riscv64-linux-gnu";

    context = pk_c_compile_context_resolve_kernel(tmp, "kernel/irq/chip.c");
    if (!context) {
        fprintf(stderr, "FAIL: compile_commands arguments resolver returned null\n");
        failures++;
        (void)unlink(path);
        (void)rmdir(tmp);
        return;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu arguments clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "compile_commands arguments clang arg");
        }
    }
    assert_eq(context->target_triple, "riscv64-linux-gnu",
        "compile_commands arguments target triple");
    pk_c_compile_context_free(context);
    (void)unlink(path);
    (void)rmdir(tmp);
}

static void test_kernel_compile_context_matches_dot_relative_source(void) {
    char tmp[] = "/tmp/provekit-core-dotrel-XXXXXX";
    char cwd[1024];
    char path[512];
    char json[1024];
    char include_arg[768];
    pk_c_compile_context *context;
    const char *want[6];

    if (mkdtemp(tmp) == NULL) {
        fprintf(stderr, "FAIL: mkdtemp failed for dot-relative fixture\n");
        failures++;
        return;
    }
    (void)snprintf(path, sizeof(path), "%s/compile_commands.json", tmp);
    (void)snprintf(json,
        sizeof(json),
        "[{\"directory\":\"%s\",\"file\":\"kernel/sched/core.c\","
        "\"command\":\"clang -D__KERNEL__ -Iinclude --target=x86_64-linux-gnu "
        "-c -o kernel/sched/core.o kernel/sched/core.c\"}]",
        tmp);
    if (write_text_file(path, json) != 0) {
        fprintf(stderr, "FAIL: could not write dot-relative compile_commands fixture\n");
        failures++;
        (void)rmdir(tmp);
        return;
    }
    (void)snprintf(include_arg, sizeof(include_arg), "-I%s/include", tmp);
    want[0] = "-x";
    want[1] = "c";
    want[2] = "-std=gnu11";
    want[3] = "-D__KERNEL__";
    want[4] = include_arg;
    want[5] = "--target=x86_64-linux-gnu";
    if (getcwd(cwd, sizeof(cwd)) == NULL || chdir(tmp) != 0) {
        fprintf(stderr, "FAIL: could not enter dot-relative fixture directory\n");
        failures++;
        (void)unlink(path);
        (void)rmdir(tmp);
        return;
    }

    context = pk_c_compile_context_resolve_kernel(".", "./kernel/sched/core.c");
    if (chdir(cwd) != 0) {
        fprintf(stderr, "FAIL: could not restore test working directory\n");
        failures++;
    }
    if (!context) {
        fprintf(stderr, "FAIL: dot-relative resolver returned null\n");
        failures++;
        (void)unlink(path);
        (void)rmdir(tmp);
        return;
    }
    if (context->extraction_result != NULL &&
        context->extraction_result->opacity_report.len != 0) {
        fprintf(stderr, "FAIL: dot-relative resolver should not report missing context\n");
        failures++;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu dot-relative clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "dot-relative clang arg");
        }
    }
    pk_c_compile_context_free(context);
    (void)unlink(path);
    (void)rmdir(tmp);
}

static void test_kernel_compile_context_rebases_compile_commands_directory(void) {
    char tmp[] = "/tmp/provekit-core-ccdir-XXXXXX";
    char path[512];
    char build_dir[512] = {0};
    char json[2048];
    char include_arg[768];
    char isystem_arg[768];
    pk_c_compile_context *context;

    if (mkdtemp(tmp) == NULL) {
        fprintf(stderr, "FAIL: mkdtemp failed for compile_commands directory fixture\n");
        failures++;
        return;
    }
    if (make_dir_path(tmp, "build", build_dir, sizeof(build_dir)) != 0) {
        fprintf(stderr, "FAIL: could not create compile_commands directory fixture\n");
        failures++;
        (void)rmdir(tmp);
        return;
    }
    (void)snprintf(path, sizeof(path), "%s/compile_commands.json", tmp);
    (void)snprintf(json,
        sizeof(json),
        "[{\"directory\":\"%s\",\"file\":\"../kernel/sched/core.c\","
        "\"command\":\"clang -Iinclude -isystem sys --target=x86_64-linux-gnu "
        "-c -o ../kernel/sched/core.o ../kernel/sched/core.c\"}]",
        build_dir);
    if (write_text_file(path, json) != 0) {
        fprintf(stderr, "FAIL: could not write compile_commands directory fixture\n");
        failures++;
        (void)rmdir(build_dir);
        (void)rmdir(tmp);
        return;
    }
    (void)snprintf(include_arg, sizeof(include_arg), "-I%s/include", build_dir);
    (void)snprintf(isystem_arg, sizeof(isystem_arg), "%s/sys", build_dir);

    context = pk_c_compile_context_resolve_kernel(tmp, "kernel/sched/core.c");
    if (!context) {
        fprintf(stderr, "FAIL: compile_commands directory resolver returned null\n");
        failures++;
        (void)unlink(path);
        (void)rmdir(build_dir);
        (void)rmdir(tmp);
        return;
    }
    if (context->n_clang_args != 7) {
        fprintf(stderr, "FAIL: expected 7 rebased clang args, got %zu\n",
            context->n_clang_args);
        failures++;
    } else {
        assert_eq(context->clang_args[3], include_arg, "rebased attached include");
        assert_eq(context->clang_args[4], "-isystem", "rebased split include flag");
        assert_eq(context->clang_args[5], isystem_arg, "rebased split include path");
        assert_eq(context->clang_args[6], "--target=x86_64-linux-gnu",
            "rebased compile_commands target arg");
    }
    pk_c_compile_context_free(context);
    (void)unlink(path);
    (void)rmdir(build_dir);
    (void)rmdir(tmp);
}

static void test_kernel_compile_context_resolves_kbuild_cmd_file(void) {
    char tmp[] = "/tmp/provekit-core-kbuild-XXXXXX";
    char path[512];
    char kernel_dir[512] = {0};
    char sched_dir[512] = {0};
    pk_c_compile_context *context;
    const char *want[] = {
        "-x", "c", "-std=gnu11",
        "-D__KERNEL__", "-Iinclude", "--target=arm64-linux-gnu"
    };

    if (mkdtemp(tmp) == NULL) {
        fprintf(stderr, "FAIL: mkdtemp failed for kbuild fixture\n");
        failures++;
        return;
    }
    if (make_dir_path(tmp, "kernel", kernel_dir, sizeof(kernel_dir)) != 0 ||
        make_dir_path(tmp, "kernel/sched", sched_dir, sizeof(sched_dir)) != 0) {
        fprintf(stderr, "FAIL: could not create kbuild fixture directories\n");
        failures++;
        if (sched_dir[0] != '\0') {
            (void)rmdir(sched_dir);
        }
        if (kernel_dir[0] != '\0') {
            (void)rmdir(kernel_dir);
        }
        (void)rmdir(tmp);
        return;
    }
    (void)snprintf(path, sizeof(path), "%s/kernel/sched/.core.o.cmd", tmp);
    if (write_text_file(path,
        "cmd_kernel/sched/core.o := gcc -D__KERNEL__ -Iinclude "
        "--target=arm64-linux-gnu -c -o kernel/sched/core.o kernel/sched/core.c\n") != 0) {
        fprintf(stderr, "FAIL: could not write kbuild .cmd fixture\n");
        failures++;
        (void)rmdir(sched_dir);
        (void)rmdir(kernel_dir);
        (void)rmdir(tmp);
        return;
    }

    context = pk_c_compile_context_resolve_kernel(tmp, "kernel/sched/core.c");
    if (!context) {
        fprintf(stderr, "FAIL: kbuild resolver returned null\n");
        failures++;
        (void)unlink(path);
        (void)rmdir(sched_dir);
        (void)rmdir(kernel_dir);
        (void)rmdir(tmp);
        return;
    }
    if (context->n_clang_args != sizeof(want) / sizeof(want[0])) {
        fprintf(stderr, "FAIL: expected %zu kbuild clang args, got %zu\n",
            sizeof(want) / sizeof(want[0]), context->n_clang_args);
        failures++;
    } else {
        for (size_t i = 0; i < context->n_clang_args; i++) {
            assert_eq(context->clang_args[i], want[i], "kbuild resolver clang arg");
        }
    }
    assert_eq(context->target_triple, "arm64-linux-gnu",
        "kbuild resolver target triple");
    pk_c_compile_context_free(context);
    (void)unlink(path);
    (void)rmdir(sched_dir);
    (void)rmdir(kernel_dir);
    (void)rmdir(tmp);
}

static void test_kernel_compile_context_reports_missing_context_opacity(void) {
    char tmp[] = "/tmp/provekit-core-missing-XXXXXX";
    pk_c_compile_context *context;

    if (mkdtemp(tmp) == NULL) {
        fprintf(stderr, "FAIL: mkdtemp failed for missing context fixture\n");
        failures++;
        return;
    }
    context = pk_c_compile_context_resolve_kernel(tmp, "kernel/missing.c");
    if (!context) {
        fprintf(stderr, "FAIL: missing kernel context resolver returned null\n");
        failures++;
        (void)rmdir(tmp);
        return;
    }
    if (context->extraction_result == NULL ||
        context->extraction_result->opacity_report.len != 1 ||
        strstr(context->extraction_result->opacity_report.items[0],
            "kernel-compile-context-missing") == NULL) {
        fprintf(stderr, "FAIL: expected missing kernel compile context opacity\n");
        failures++;
    }
    pk_c_compile_context_free(context);
    (void)rmdir(tmp);
}

#ifdef PK_C_ENABLE_CLANG_AST
static void test_parse_with_clang_ast_extracts_functions_and_calls(void) {
    const char *args[] = {"-x", "c", "-std=c11"};
    pk_c_parse_options options = {0};
    const char *source =
        "int helper(int x) { return x + 1; }\n"
        "int compute(int y) { return helper(y); }\n";
    pk_c_source_facts *facts;

    options.backend = PK_C_PARSE_BACKEND_CLANG_AST;
    options.clang_args = args;
    options.n_clang_args = sizeof(args) / sizeof(args[0]);
    options.compile_command = "clang -x c -std=c11 fixture.c";
    facts = pk_c_parse_source_with_options("fixture.c", source, &options);
    if (!facts) {
        fprintf(stderr, "FAIL: clang AST parse returned null\n");
        failures++;
        return;
    }
    assert_eq(facts->parser_backend, "libclang", "clang backend provenance");
    assert_eq(facts->parser_compile_command, "clang -x c -std=c11 fixture.c",
        "clang compile command provenance");
    if (facts->n_functions != 2) {
        fprintf(stderr, "FAIL: expected 2 clang AST functions, got %zu\n", facts->n_functions);
        failures++;
    } else {
        assert_eq(facts->functions[0].name, "helper", "clang first function");
        assert_eq(facts->functions[1].name, "compute", "clang second function");
        assert_int_eq(facts->functions[0].has_body, 1, "clang helper has body");
        assert_int_eq(facts->functions[1].has_body, 1, "clang compute has body");
    }
    if (facts->n_call_sites != 1) {
        fprintf(stderr, "FAIL: expected 1 clang AST call site, got %zu\n", facts->n_call_sites);
        failures++;
    } else {
        assert_eq(facts->call_sites[0].caller, "compute", "clang call caller");
        assert_eq(facts->call_sites[0].callee, "helper", "clang call callee");
    }
    pk_c_source_facts_free(facts);
}

static void test_compile_context_feeds_clang_ast_options(void) {
    const char *source =
        "#ifndef CONFIG_SMP\n"
        "#error CONFIG_SMP missing\n"
        "#endif\n"
        "int helper(int x) { return x + 1; }\n"
        "int compute(int y) { return helper(y); }\n";
    pk_c_compile_context *context = pk_c_compile_context_from_command(
        "kernel/sched/core.c",
        "clang -DCONFIG_SMP=1 -c -o kernel/sched/core.o kernel/sched/core.c");
    pk_c_parse_options options = {0};
    pk_c_source_facts *facts;

    if (!context) {
        fprintf(stderr, "FAIL: AST compile context returned null\n");
        failures++;
        return;
    }
    pk_c_compile_context_configure_parse_options(context, PK_C_PARSE_BACKEND_CLANG_AST, &options);
    facts = pk_c_parse_source_with_options("kernel/sched/core.c", source, &options);
    if (!facts) {
        fprintf(stderr, "FAIL: clang AST parse with compile context returned null\n");
        failures++;
        pk_c_compile_context_free(context);
        return;
    }
    assert_eq(facts->parser_backend, "libclang", "compile context AST backend");
    assert_eq(facts->parser_compile_command,
        "clang -DCONFIG_SMP=1 -c -o kernel/sched/core.o kernel/sched/core.c",
        "compile context AST provenance");
    if (facts->n_functions != 2) {
        fprintf(stderr, "FAIL: expected 2 AST functions from compile context, got %zu\n",
            facts->n_functions);
        failures++;
    }
    pk_c_source_facts_free(facts);
    pk_c_compile_context_free(context);
}
#else
static void test_parse_with_clang_ast_stub_reports_opacity(void) {
    pk_c_parse_options options = {0};
    const char *source = "int one(void) { return 1; }\n";
    pk_c_source_facts *facts;

    options.backend = PK_C_PARSE_BACKEND_CLANG_AST;
    facts = pk_c_parse_source_with_options("fixture.c", source, &options);
    if (!facts) {
        fprintf(stderr, "FAIL: clang AST stub parse returned null\n");
        failures++;
        return;
    }
    assert_eq(facts->parser_backend, "regex", "clang stub fallback backend provenance");
    if (facts->extraction_result == NULL ||
        facts->extraction_result->opacity_report.len != 1 ||
        strstr(facts->extraction_result->opacity_report.items[0],
            "ast-backend-unavailable") == NULL) {
        fprintf(stderr, "FAIL: expected AST backend unavailable opacity entry\n");
        failures++;
    }
    pk_c_source_facts_free(facts);
}
#endif

int main(void) {
    test_empty_result_json();
    test_populated_result_json();
    test_opacity_and_refusal_are_separate();
    test_structured_helpers_escape_json_strings();
    test_array_growth_overflow_is_rejected();
    test_parse_functions_and_macros();
    test_parse_nested_macro_arguments();
    test_parse_same_line_function_body_call();
    test_parse_recursive_same_line_function_body_call();
    test_parse_ignores_comments_and_strings();
    test_parse_sparse_annotations();
    test_contract_annotation_ignores_string_literal_marker();
    test_contract_annotation_ignores_block_comment_marker();
    test_parse_with_regex_options_records_backend();
    test_compile_context_from_kbuild_command_filters_for_clang();
    test_compile_context_from_quoted_command_preserves_shell_words();
    test_compile_context_reports_dropped_gcc_plugin_flags();
    test_kernel_compile_context_resolves_compile_commands_json();
    test_kernel_compile_context_resolves_compile_commands_arguments();
    test_kernel_compile_context_matches_dot_relative_source();
    test_kernel_compile_context_rebases_compile_commands_directory();
    test_kernel_compile_context_resolves_kbuild_cmd_file();
    test_kernel_compile_context_reports_missing_context_opacity();
#ifdef PK_C_ENABLE_CLANG_AST
    test_parse_with_clang_ast_extracts_functions_and_calls();
    test_compile_context_feeds_clang_ast_options();
#else
    test_parse_with_clang_ast_stub_reports_opacity();
#endif
    if (failures != 0) {
        fprintf(stderr, "%d failures\n", failures);
        return 1;
    }
    puts("provekit-lift-core tests passed");
    return 0;
}
