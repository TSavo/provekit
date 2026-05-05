/* SPDX-License-Identifier: Apache-2.0 */

#include "provekit/ir.h"
#include <stdlib.h>
#include <string.h>

static char *pk_strdup(const char *s) {
    if (!s) return NULL;
    size_t n = strlen(s) + 1;
    char *d = (char *)malloc(n);
    if (d) memcpy(d, s, n);
    return d;
}

/* ----------------------------------------------------------------------- */
/* Sort                                                                    */
/* ----------------------------------------------------------------------- */

pk_sort *pk_sort_primitive(const char *name) {
    pk_sort *s = (pk_sort *)calloc(1, sizeof(pk_sort));
    if (!s) return NULL;
    s->kind = PK_SORT_PRIMITIVE;
    s->data.primitive.name = pk_strdup(name);
    return s;
}

pk_sort *pk_sort_function(pk_sort **args, size_t n_args, pk_sort *ret) {
    pk_sort *s = (pk_sort *)calloc(1, sizeof(pk_sort));
    if (!s) return NULL;
    s->kind = PK_SORT_FUNCTION;
    s->data.function.args = args;
    s->data.function.n_args = n_args;
    s->data.function.ret = ret;
    return s;
}

pk_sort *pk_sort_dependent(const char *name, const char *index_var, pk_sort *index_sort) {
    pk_sort *s = (pk_sort *)calloc(1, sizeof(pk_sort));
    if (!s) return NULL;
    s->kind = PK_SORT_DEPENDENT;
    s->data.dependent.name = pk_strdup(name);
    s->data.dependent.index_var = pk_strdup(index_var);
    s->data.dependent.index_sort = index_sort;
    return s;
}

void pk_sort_free(pk_sort *s) {
    if (!s) return;
    switch (s->kind) {
        case PK_SORT_PRIMITIVE:
            free(s->data.primitive.name);
            break;
        case PK_SORT_FUNCTION:
            for (size_t i = 0; i < s->data.function.n_args; i++) {
                pk_sort_free(s->data.function.args[i]);
            }
            free(s->data.function.args);
            pk_sort_free(s->data.function.ret);
            break;
        case PK_SORT_DEPENDENT:
            free(s->data.dependent.name);
            free(s->data.dependent.index_var);
            pk_sort_free(s->data.dependent.index_sort);
            break;
    }
    free(s);
}

/* ----------------------------------------------------------------------- */
/* Term                                                                    */
/* ----------------------------------------------------------------------- */

pk_term *pk_term_var_new(const char *name) {
    pk_term *t = (pk_term *)calloc(1, sizeof(pk_term));
    if (!t) return NULL;
    t->kind = PK_TERM_VAR;
    t->data.var.name = pk_strdup(name);
    return t;
}

pk_term *pk_term_const_int(int64_t value, pk_sort *sort) {
    pk_term *t = (pk_term *)calloc(1, sizeof(pk_term));
    if (!t) return NULL;
    t->kind = PK_TERM_CONST;
    int64_t *v = (int64_t *)malloc(sizeof(int64_t));
    if (!v) { free(t); return NULL; }
    *v = value;
    t->data.constant.value = v;
    t->data.constant.sort = sort;
    t->data.constant.is_string = 0;
    return t;
}

pk_term *pk_term_const_str(const char *value, pk_sort *sort) {
    pk_term *t = (pk_term *)calloc(1, sizeof(pk_term));
    if (!t) return NULL;
    t->kind = PK_TERM_CONST;
    t->data.constant.value = pk_strdup(value);
    t->data.constant.sort = sort;
    t->data.constant.is_string = 1;
    return t;
}

pk_term *pk_term_const_bool(int value, pk_sort *sort) {
    pk_term *t = (pk_term *)calloc(1, sizeof(pk_term));
    if (!t) return NULL;
    t->kind = PK_TERM_CONST;
    int *v = (int *)malloc(sizeof(int));
    if (!v) { free(t); return NULL; }
    *v = value ? 1 : 0;
    t->data.constant.value = v;
    t->data.constant.sort = sort;
    t->data.constant.is_string = 2;
    return t;
}

pk_term *pk_term_const_null(pk_sort *sort) {
    pk_term *t = (pk_term *)calloc(1, sizeof(pk_term));
    if (!t) return NULL;
    t->kind = PK_TERM_CONST;
    t->data.constant.value = NULL;
    t->data.constant.sort = sort;
    t->data.constant.is_string = 3;
    return t;
}

pk_term *pk_term_ctor_new(const char *name, pk_term **args, size_t n_args) {
    pk_term *t = (pk_term *)calloc(1, sizeof(pk_term));
    if (!t) return NULL;
    t->kind = PK_TERM_CTOR;
    t->data.ctor.name = pk_strdup(name);
    t->data.ctor.n_args = n_args;
    if (n_args > 0) {
        t->data.ctor.args = (pk_term **)calloc(n_args, sizeof(pk_term *));
        if (!t->data.ctor.args) { free(t); return NULL; }
        for (size_t i = 0; i < n_args; i++) {
            t->data.ctor.args[i] = args[i];
        }
    }
    return t;
}

void pk_term_free(pk_term *t) {
    if (!t) return;
    switch (t->kind) {
        case PK_TERM_VAR:
            free(t->data.var.name);
            break;
        case PK_TERM_CONST:
            free(t->data.constant.value);
            pk_sort_free(t->data.constant.sort);
            break;
        case PK_TERM_CTOR:
            free(t->data.ctor.name);
            for (size_t i = 0; i < t->data.ctor.n_args; i++) {
                pk_term_free(t->data.ctor.args[i]);
            }
            free(t->data.ctor.args);
            break;
    }
    free(t);
}

/* ----------------------------------------------------------------------- */
/* Formula                                                                 */
/* ----------------------------------------------------------------------- */

pk_formula *pk_formula_atomic_new(const char *name, pk_term **args, size_t n_args) {
    pk_formula *f = (pk_formula *)calloc(1, sizeof(pk_formula));
    if (!f) return NULL;
    f->kind = PK_FORMULA_ATOMIC;
    f->data.atomic.name = pk_strdup(name);
    f->data.atomic.n_args = n_args;
    if (n_args > 0) {
        f->data.atomic.args = (pk_term **)calloc(n_args, sizeof(pk_term *));
        if (!f->data.atomic.args) { free(f); return NULL; }
        for (size_t i = 0; i < n_args; i++) {
            f->data.atomic.args[i] = args[i];
        }
    }
    return f;
}

pk_formula *pk_formula_connective_new(const char *kind, pk_formula **operands, size_t n_operands) {
    pk_formula *f = (pk_formula *)calloc(1, sizeof(pk_formula));
    if (!f) return NULL;
    f->kind = PK_FORMULA_CONNECTIVE;
    f->data.connective.kind = pk_strdup(kind);
    f->data.connective.n_operands = n_operands;
    if (n_operands > 0) {
        f->data.connective.operands = (pk_formula **)calloc(n_operands, sizeof(pk_formula *));
        if (!f->data.connective.operands) { free(f); return NULL; }
        for (size_t i = 0; i < n_operands; i++) {
            f->data.connective.operands[i] = operands[i];
        }
    }
    return f;
}

pk_formula *pk_formula_quantifier_new(const char *kind, const char *name, pk_sort *sort, pk_formula *body) {
    pk_formula *f = (pk_formula *)calloc(1, sizeof(pk_formula));
    if (!f) return NULL;
    f->kind = PK_FORMULA_QUANTIFIER;
    f->data.quantifier.kind = pk_strdup(kind);
    f->data.quantifier.name = pk_strdup(name);
    f->data.quantifier.sort = sort;
    f->data.quantifier.body = body;
    return f;
}

void pk_formula_free(pk_formula *f) {
    if (!f) return;
    switch (f->kind) {
        case PK_FORMULA_ATOMIC:
            free(f->data.atomic.name);
            for (size_t i = 0; i < f->data.atomic.n_args; i++) {
                pk_term_free(f->data.atomic.args[i]);
            }
            free(f->data.atomic.args);
            break;
        case PK_FORMULA_CONNECTIVE:
            free(f->data.connective.kind);
            for (size_t i = 0; i < f->data.connective.n_operands; i++) {
                pk_formula_free(f->data.connective.operands[i]);
            }
            free(f->data.connective.operands);
            break;
        case PK_FORMULA_QUANTIFIER:
            free(f->data.quantifier.kind);
            free(f->data.quantifier.name);
            pk_sort_free(f->data.quantifier.sort);
            pk_formula_free(f->data.quantifier.body);
            break;
    }
    free(f);
}

/* ----------------------------------------------------------------------- */
/* Declaration                                                             */
/* ----------------------------------------------------------------------- */

pk_decl *pk_decl_contract_new(const char *name, const char *out_binding,
                               pk_formula *pre, pk_formula *post, pk_formula *inv) {
    pk_decl *d = (pk_decl *)calloc(1, sizeof(pk_decl));
    if (!d) return NULL;
    d->kind = PK_DECL_CONTRACT;
    d->data.contract.name = pk_strdup(name);
    d->data.contract.out_binding = pk_strdup(out_binding);
    d->data.contract.pre = pre;
    d->data.contract.post = post;
    d->data.contract.inv = inv;
    return d;
}

pk_decl *pk_decl_bridge_new(const char *name, const char *source_symbol,
                            const char *source_layer, const char *source_contract_cid,
                            const char *target_contract_cid, const char *target_proof_cid,
                            const char *target_layer, const char *notes) {
    pk_decl *d = (pk_decl *)calloc(1, sizeof(pk_decl));
    if (!d) return NULL;
    d->kind = PK_DECL_BRIDGE;
    d->data.bridge.name = pk_strdup(name);
    d->data.bridge.source_symbol = pk_strdup(source_symbol);
    d->data.bridge.source_layer = pk_strdup(source_layer);
    d->data.bridge.source_contract_cid = pk_strdup(source_contract_cid);
    d->data.bridge.target_contract_cid = pk_strdup(target_contract_cid);
    d->data.bridge.target_proof_cid = pk_strdup(target_proof_cid);
    d->data.bridge.target_layer = pk_strdup(target_layer);
    d->data.bridge.notes = pk_strdup(notes);
    return d;
}

void pk_decl_free(pk_decl *d) {
    if (!d) return;
    switch (d->kind) {
        case PK_DECL_CONTRACT:
            free(d->data.contract.name);
            free(d->data.contract.out_binding);
            pk_formula_free(d->data.contract.pre);
            pk_formula_free(d->data.contract.post);
            pk_formula_free(d->data.contract.inv);
            break;
        case PK_DECL_BRIDGE:
            free(d->data.bridge.name);
            free(d->data.bridge.source_symbol);
            free(d->data.bridge.source_layer);
            free(d->data.bridge.source_contract_cid);
            free(d->data.bridge.target_contract_cid);
            free(d->data.bridge.target_proof_cid);
            free(d->data.bridge.target_layer);
            free(d->data.bridge.notes);
            break;
    }
    free(d);
}

/* ----------------------------------------------------------------------- */
/* Buffer                                                                  */
/* ----------------------------------------------------------------------- */

pk_buffer *pk_buffer_new(void) {
    pk_buffer *buf = (pk_buffer *)calloc(1, sizeof(pk_buffer));
    if (!buf) return NULL;
    buf->cap = 256;
    buf->data = (char *)malloc(buf->cap);
    if (!buf->data) { free(buf); return NULL; }
    buf->data[0] = '\0';
    return buf;
}

void pk_buffer_free(pk_buffer *buf) {
    if (!buf) return;
    free(buf->data);
    free(buf);
}

static void pk_buffer_grow(pk_buffer *buf, size_t need) {
    if (buf->len + need + 1 <= buf->cap) return;
    size_t new_cap = buf->cap * 2;
    while (new_cap < buf->len + need + 1) new_cap *= 2;
    char *new_data = (char *)realloc(buf->data, new_cap);
    if (!new_data) return; /* silent failure — real code would abort */
    buf->data = new_data;
    buf->cap = new_cap;
}

void pk_buffer_append(pk_buffer *buf, const char *s) {
    if (!s) return;
    size_t n = strlen(s);
    pk_buffer_grow(buf, n);
    memcpy(buf->data + buf->len, s, n + 1);
    buf->len += n;
}

void pk_buffer_append_char(pk_buffer *buf, char c) {
    pk_buffer_grow(buf, 1);
    buf->data[buf->len] = c;
    buf->data[buf->len + 1] = '\0';
    buf->len++;
}

char *pk_buffer_steal(pk_buffer *buf) {
    if (!buf) return NULL;
    char *s = buf->data;
    buf->data = NULL;
    buf->len = buf->cap = 0;
    return s;
}
