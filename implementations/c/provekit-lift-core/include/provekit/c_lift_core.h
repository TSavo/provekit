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

pk_c_lift_result *pk_c_lift_result_new(void);
void pk_c_lift_result_free(pk_c_lift_result *result);
int pk_c_lift_result_add_declaration(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_call_edge(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_diagnostic(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_opacity(pk_c_lift_result *result, const char *json);
int pk_c_lift_result_add_refusal(pk_c_lift_result *result, const char *json);
char *pk_c_lift_result_to_json(const pk_c_lift_result *result);

#endif
