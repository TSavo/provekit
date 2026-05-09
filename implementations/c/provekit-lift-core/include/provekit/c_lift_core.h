/* SPDX-License-Identifier: Apache-2.0 */
#ifndef PROVEKIT_C_LIFT_CORE_H
#define PROVEKIT_C_LIFT_CORE_H

#include <stddef.h>

typedef struct {
    char *path;
    int line;
    int column;
} pk_c_locus;

typedef struct {
    /* Owned JSON fragment strings. */
    char **items;
    size_t len;
    size_t cap;
} pk_c_json_array;

typedef struct pk_c_lift_result {
    pk_c_json_array declarations;
    pk_c_json_array call_edges;
    pk_c_json_array diagnostics;
    pk_c_json_array opacity_report;
    pk_c_json_array refusals;
} pk_c_lift_result;

typedef enum {
    PK_C_PARSE_BACKEND_AUTO = 0,
    PK_C_PARSE_BACKEND_REGEX = 1,
    PK_C_PARSE_BACKEND_CLANG_AST = 2
} pk_c_parse_backend;

typedef struct {
    pk_c_parse_backend backend;
    const char *const *clang_args;
    size_t n_clang_args;
    const char *compile_command;
    const char *target_triple;
} pk_c_parse_options;

typedef struct {
    char **clang_args;
    size_t n_clang_args;
    size_t cap_clang_args;
    char *compile_command;
    char *target_triple;
    pk_c_lift_result *extraction_result;
} pk_c_compile_context;

typedef struct {
    char *name;
    pk_c_locus locus;
    int has_body;
    int has_contract_annotation;
} pk_c_function_fact;

typedef struct {
    char *name;
    char *enclosing_function;
    char *argument_text;
    pk_c_locus locus;
} pk_c_macro_call_fact;

typedef struct {
    char *name;
    char *enclosing_function;
    char *argument_text;
    pk_c_locus locus;
} pk_c_sparse_annotation_fact;

typedef struct {
    char *caller;
    char *callee;
    pk_c_locus locus;
} pk_c_call_site_fact;

typedef struct {
    pk_c_function_fact *functions;
    size_t n_functions;
    size_t cap_functions;
    pk_c_macro_call_fact *macro_calls;
    size_t n_macro_calls;
    size_t cap_macro_calls;
    pk_c_sparse_annotation_fact *sparse_annotations;
    size_t n_sparse_annotations;
    size_t cap_sparse_annotations;
    pk_c_call_site_fact *call_sites;
    size_t n_call_sites;
    size_t cap_call_sites;
    pk_c_lift_result *extraction_result;
    char *parser_backend;
    char *parser_compile_command;
    char *parser_target_triple;
} pk_c_source_facts;

pk_c_source_facts *pk_c_parse_source(const char *path, const char *source);
pk_c_source_facts *pk_c_parse_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options);
void pk_c_source_facts_free(pk_c_source_facts *facts);

pk_c_compile_context *pk_c_compile_context_from_command(
    const char *path,
    const char *command);
pk_c_compile_context *pk_c_compile_context_from_kbuild_cmdline(
    const char *path,
    const char *cmdline);
void pk_c_compile_context_configure_parse_options(
    const pk_c_compile_context *context,
    pk_c_parse_backend backend,
    pk_c_parse_options *options);
void pk_c_compile_context_free(pk_c_compile_context *context);

pk_c_lift_result *pk_c_lift_result_new(void);
void pk_c_lift_result_free(pk_c_lift_result *result);
int pk_c_lift_result_add_declaration(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_call_edge(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_diagnostic(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_opacity(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_refusal(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_opacity_entry(
    pk_c_lift_result *result,
    const char *kind,
    const char *path,
    int line,
    int column,
    const char *reason,
    const char *affected_surface);
int pk_c_lift_result_add_refusal_entry(
    pk_c_lift_result *result,
    const char *kind,
    const char *path,
    int line,
    int column,
    const char *surface,
    const char *reason);
char *pk_c_lift_result_to_json(const pk_c_lift_result *result);

#endif
