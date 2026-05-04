/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Slab + ContractDecl + tiny formula DSL for the c self-contracts
 * orchestrator. Mirrors implementations/java/.../Slab.java 1:1 in shape
 * and JCS Value-tree output, so the bytes minted by this kit cohabit
 * the same wire grammar as every other kit.
 *
 * Formula shapes track the cross-kit `formulaToValue` peer:
 *   var:      {"kind":"var","name":<name>}
 *   const:    {"kind":"const","value":<v>,"sort":<sort>}
 *   ctor:     {"kind":"ctor","name":<name>,"args":[<term>...]}
 *   atomic:   {"kind":"atomic","name":<name>,"args":[<term>...]}
 *   forall:   {"kind":"forall","name":<name>,"sort":<sort>,"body":<formula>}
 *   sort:     {"kind":"primitive","name":<name>}
 *
 * Each helper returns a freshly-malloc'd `pksc_value*` whose ownership
 * passes to the caller. Composing helpers (`mcsc_f_app`, `mcsc_f_eq`,
 * etc.) take ownership of their argument values and place them inside
 * the returned tree, so callers never need to free intermediates.
 */

#ifndef MCSC_SLAB_H
#define MCSC_SLAB_H

#include <stddef.h>
#include "provekit/self_contracts.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ----------------------------------------------------------------------- */
/* ContractDecl + Slab + Collector                                          */
/* ----------------------------------------------------------------------- */

typedef struct mcsc_contract {
    char       *name;        /* owned */
    pksc_value *pre;         /* owned, nullable */
    pksc_value *post;        /* owned, nullable */
    pksc_value *inv;         /* owned, nullable */
    char       *out_binding; /* owned */
} mcsc_contract;

typedef struct mcsc_slab {
    char           *label;     /* owned */
    char           *path;      /* owned */
    mcsc_contract **contracts; /* owned array of owned ptrs */
    size_t          n;
    size_t          cap;
} mcsc_slab;

typedef struct mcsc_slab_list {
    mcsc_slab **slabs; /* owned array of owned ptrs */
    size_t      n;
    size_t      cap;
} mcsc_slab_list;

mcsc_slab      *mcsc_slab_new(const char *label, const char *path);
void            mcsc_slab_free(mcsc_slab *s);
int             mcsc_slab_must(mcsc_slab *s, const char *name, pksc_value *post_formula);

mcsc_slab_list *mcsc_slab_list_new(void);
void            mcsc_slab_list_free(mcsc_slab_list *l);
int             mcsc_slab_list_push(mcsc_slab_list *l, mcsc_slab *s);

/* ----------------------------------------------------------------------- */
/* Formula DSL — every constructor returns a malloc'd pksc_value tree.     */
/* ----------------------------------------------------------------------- */

pksc_value *mcsc_f_sort(const char *name);
pksc_value *mcsc_f_var(const char *name);
pksc_value *mcsc_f_str(const char *literal);
pksc_value *mcsc_f_num(int64_t n);

/* `name` is copied; `args` array is consumed (each element placed into the
 * built tree; the `args` array itself is not freed by the helper). */
pksc_value *mcsc_f_ctor(const char *name, pksc_value **args, size_t n_args);
pksc_value *mcsc_f_app(const char *name, pksc_value **args, size_t n_args);

pksc_value *mcsc_f_eq(pksc_value *left, pksc_value *right);
pksc_value *mcsc_f_gte(pksc_value *left, pksc_value *right);
pksc_value *mcsc_f_starts_with(pksc_value *left, pksc_value *right);

/*
 * Forall over a fresh variable named `var_name` of sort `sort` (consumed).
 * `build_body` is called with the variable term to produce the body
 * formula; both args go into the returned tree.
 */
typedef pksc_value *(*mcsc_body_fn)(pksc_value *bound_var, void *ctx);
pksc_value *mcsc_f_forall(const char *var_name, pksc_value *sort,
                          mcsc_body_fn build_body, void *ctx);

#ifdef __cplusplus
}
#endif

#endif /* MCSC_SLAB_H */
