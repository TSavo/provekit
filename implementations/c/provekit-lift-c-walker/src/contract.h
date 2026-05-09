/* SPDX-License-Identifier: Apache-2.0 */
#ifndef PROVEKIT_C_WALKER_CONTRACT_H
#define PROVEKIT_C_WALKER_CONTRACT_H

#include <stddef.h>

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} pk_c_walker_json_list;

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} pk_c_walker_string_list;

typedef struct {
    pk_c_walker_json_list preconditions;
    pk_c_walker_json_list effects;
    pk_c_walker_string_list formals;
    char *post;
} pk_c_walker_contract;

typedef struct {
    const char *signature_start;
    const char *params_start;
    const char *params_end;
    const char *body_start;
    const char *body_end;
} pk_c_walker_function_span;

void pk_c_walker_contract_init(pk_c_walker_contract *contract);
void pk_c_walker_contract_free(pk_c_walker_contract *contract);
int pk_c_walker_contract_add_pre_take(pk_c_walker_contract *contract, char *json);
int pk_c_walker_contract_add_effect_take(pk_c_walker_contract *contract, char *json);
int pk_c_walker_contract_add_formal(pk_c_walker_contract *contract, const char *name);
int pk_c_walker_contract_set_post_take(pk_c_walker_contract *contract, char *json);

char *pk_c_walker_term_var(const char *name);
char *pk_c_walker_term_const_int(long value);
char *pk_c_walker_formula_atomic1_var(const char *name, const char *arg);
char *pk_c_walker_formula_atomic2_var_const(const char *name, const char *lhs, long rhs);
char *pk_c_walker_formula_atomic2_var_var(const char *name, const char *lhs, const char *rhs);

int pk_c_walker_find_function_source(
    const char *source,
    const char *fn_name,
    pk_c_walker_function_span *span);
int pk_c_walker_extract_defensive_patterns(
    const char *source,
    const char *fn_name,
    pk_c_walker_contract *contract);
int pk_c_walker_extract_type_predicates(
    const char *source,
    const char *fn_name,
    pk_c_walker_contract *contract);

#endif
