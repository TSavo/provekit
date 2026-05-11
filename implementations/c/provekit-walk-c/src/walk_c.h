/* SPDX-License-Identifier: Apache-2.0 */
#ifndef PROVEKIT_WALK_C_INTERNAL_H
#define PROVEKIT_WALK_C_INTERNAL_H

#include <stddef.h>

#include "provekit/c_lift_core.h"

#ifdef PK_C_ENABLE_CLANG_AST
#include <clang-c/Index.h>
#endif

typedef struct pk_c_walk_term pk_c_walk_term;
typedef struct pk_c_walk_formula pk_c_walk_formula;

typedef enum {
    PK_C_WALK_TERM_VAR = 0,
    PK_C_WALK_TERM_CONST_INT = 1,
    PK_C_WALK_TERM_CTOR = 2
} pk_c_walk_term_kind;

typedef enum {
    PK_C_WALK_FORMULA_ATOMIC = 0,
    PK_C_WALK_FORMULA_AND = 1,
    PK_C_WALK_FORMULA_OR = 2,
    PK_C_WALK_FORMULA_NOT = 3,
    PK_C_WALK_FORMULA_IMPLIES = 4
} pk_c_walk_formula_kind;

struct pk_c_walk_term {
    pk_c_walk_term_kind kind;
    char *name;
    long value;
    pk_c_walk_term **args;
    size_t n_args;
};

struct pk_c_walk_formula {
    pk_c_walk_formula_kind kind;
    char *name;
    pk_c_walk_term **args;
    size_t n_args;
    pk_c_walk_formula **ops;
    size_t n_ops;
};

typedef struct {
    char *kind;
    char *name;
    char *branch;
    char *actuals_json;
    size_t stmt_index;
    size_t join_stmt_index;
    int line;
    int column;
    pk_c_walk_formula *cond;
    pk_c_walk_formula *wp;
} pk_c_walk_arrival;

typedef struct {
    char *caller_name;
    char *callee_name;
    char **formals;
    size_t n_formals;
    char *path;
    int line;
    int column;
    pk_c_walk_arrival *arrivals;
    size_t n_arrivals;
    size_t cap_arrivals;
} pk_c_walk_chain;

char *pk_c_walk_copy_n(const char *src, size_t len);
char *pk_c_walk_copy(const char *src);

pk_c_walk_term *pk_c_walk_term_var(const char *name);
pk_c_walk_term *pk_c_walk_term_const_int(long value);
pk_c_walk_term *pk_c_walk_term_ctor_take(const char *name, pk_c_walk_term **args, size_t n_args);
pk_c_walk_term *pk_c_walk_term_clone(const pk_c_walk_term *term);
void pk_c_walk_term_free(pk_c_walk_term *term);

pk_c_walk_formula *pk_c_walk_formula_true(void);
pk_c_walk_formula *pk_c_walk_formula_atomic_take(const char *name, pk_c_walk_term **args, size_t n_args);
pk_c_walk_formula *pk_c_walk_formula_and_take(pk_c_walk_formula *lhs, pk_c_walk_formula *rhs);
pk_c_walk_formula *pk_c_walk_formula_or_take(pk_c_walk_formula *lhs, pk_c_walk_formula *rhs);
pk_c_walk_formula *pk_c_walk_formula_implies_take(pk_c_walk_formula *lhs, pk_c_walk_formula *rhs);
pk_c_walk_formula *pk_c_walk_formula_not_take(pk_c_walk_formula *inner);
pk_c_walk_formula *pk_c_walk_formula_negate_take(pk_c_walk_formula *formula);
pk_c_walk_formula *pk_c_walk_formula_clone(const pk_c_walk_formula *formula);
void pk_c_walk_formula_free(pk_c_walk_formula *formula);
pk_c_walk_formula *pk_c_walk_substitute_formula(
    const pk_c_walk_formula *formula,
    const char *var_name,
    const pk_c_walk_term *replacement);
int pk_c_walk_formula_is_true(const pk_c_walk_formula *formula);
char *pk_c_walk_formula_json(const pk_c_walk_formula *formula);
char *pk_c_walk_term_json(const pk_c_walk_term *term);
pk_c_walk_term *pk_c_walk_term_from_text(const char *start, size_t len);
pk_c_walk_term *pk_c_walk_term_from_text_full(const char *start, size_t len);
pk_c_walk_formula *pk_c_walk_formula_from_text(const char *start, size_t len);

void pk_c_walk_chain_init(pk_c_walk_chain *chain);
void pk_c_walk_chain_free(pk_c_walk_chain *chain);
int pk_c_walk_chain_add_arrival(
    pk_c_walk_chain *chain,
    const char *kind,
    const char *name,
    size_t stmt_index,
    int line,
    int column,
    const pk_c_walk_formula *wp);
int pk_c_walk_chain_add_callsite_arrival(
    pk_c_walk_chain *chain,
    const char *name,
    size_t stmt_index,
    int line,
    int column,
    const char *actuals_json,
    const pk_c_walk_formula *wp);
int pk_c_walk_chain_add_conditional_arm_arrival(
    pk_c_walk_chain *chain,
    const char *branch,
    size_t stmt_index,
    size_t join_stmt_index,
    int line,
    int column,
    const pk_c_walk_formula *cond,
    const pk_c_walk_formula *wp);

#ifdef PK_C_ENABLE_CLANG_AST
char *pk_c_walk_cursor_source(CXCursor cursor);
char *pk_c_walk_call_callee(CXCursor cursor);
pk_c_walk_term *pk_c_walk_lift_term(CXCursor cursor);
pk_c_walk_term *pk_c_walk_lift_term_full(CXCursor cursor);
pk_c_walk_formula *pk_c_walk_lift_condition(CXCursor cursor);
pk_c_walk_formula *pk_c_walk_lift_function_pre(CXCursor function_cursor);
pk_c_walk_formula *pk_c_walk_lift_if_exit_pre(CXCursor if_cursor);
char **pk_c_walk_lift_formals(CXCursor function_cursor, size_t *len_out);
void pk_c_walk_lift_string_list_free(char **items, size_t len);
void pk_c_walk_locus(CXCursor cursor, int *line, int *column);
int pk_c_walk_emit_chain_contract(pk_c_lift_result *result, const pk_c_walk_chain *chain);
int pk_c_walk_apply_call_context(
    CXCursor top_stmt,
    CXCursor call_cursor,
    pk_c_walk_formula **wp,
    pk_c_walk_chain *chain,
    size_t stmt_index);
int pk_c_walk_apply_two_armed_if_stmt(
    CXCursor stmt,
    pk_c_walk_formula **wp,
    pk_c_walk_chain *chain,
    size_t stmt_index);
#endif

pk_c_lift_result *pk_c_walk_lift_source(const char *path, const char *source);
pk_c_lift_result *pk_c_walk_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options);

#endif
