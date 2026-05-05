/* SPDX-License-Identifier: Apache-2.0 */
/*
 * JCS canonical JSON emitter.
 *
 * Rules (RFC 8785 + protocol canonicalization spec):
 *   - Object keys sorted by Unicode code-point order (strcmp for ASCII).
 *   - Strings: escape ", \, and U+0000..U+001F as \u00XX (lowercase hex).
 *   - Integers: plain decimal (snprintf %ld).
 *   - true / false / null verbatim.
 *   - No whitespace anywhere.
 */

#include "provekit/ir.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ----------------------------------------------------------------------- */
/* String escaping                                                         */
/* ----------------------------------------------------------------------- */

static void emit_string(pk_buffer *buf, const char *s) {
    pk_buffer_append_char(buf, '"');
    for (const char *p = s; *p; p++) {
        unsigned char c = (unsigned char)*p;
        if (c == '"') {
            pk_buffer_append(buf, "\\\"");
        } else if (c == '\\') {
            pk_buffer_append(buf, "\\\\");
        } else if (c < 0x20) {
            char esc[7];
            snprintf(esc, sizeof(esc), "\\u00%02x", c);
            pk_buffer_append(buf, esc);
        } else {
            pk_buffer_append_char(buf, *p);
        }
    }
    pk_buffer_append_char(buf, '"');
}

/* ----------------------------------------------------------------------- */
/* Generic sorted-object emitter                                           */
/* ----------------------------------------------------------------------- */

typedef struct {
    const char *key;
    void (*fn)(pk_buffer *buf, void *ctx);
    void *ctx;
} pk_field;

static int field_cmp(const void *a, const void *b) {
    const pk_field *fa = (const pk_field *)a;
    const pk_field *fb = (const pk_field *)b;
    return strcmp(fa->key, fb->key);
}

static void emit_object(pk_buffer *buf, pk_field *fields, size_t n) {
    qsort(fields, n, sizeof(pk_field), field_cmp);
    pk_buffer_append_char(buf, '{');
    for (size_t i = 0; i < n; i++) {
        if (i > 0) pk_buffer_append_char(buf, ',');
        emit_string(buf, fields[i].key);
        pk_buffer_append_char(buf, ':');
        fields[i].fn(buf, fields[i].ctx);
    }
    pk_buffer_append_char(buf, '}');
}

/* ----------------------------------------------------------------------- */
/* Forward declarations                                                    */
/* ----------------------------------------------------------------------- */

static void emit_sort(pk_buffer *buf, pk_sort *s);
static void emit_term(pk_buffer *buf, pk_term *t);
static void emit_formula(pk_buffer *buf, pk_formula *f);

static void emit_formula_wrapper2(pk_buffer *buf, void *ctx) {
    emit_formula(buf, (pk_formula *)ctx);
}

/* ----------------------------------------------------------------------- */
/* Sort                                                                    */
/* ----------------------------------------------------------------------- */

typedef struct { pk_sort **args; size_t n; } sort_array_ctx;

static void emit_sort_array_fn(pk_buffer *buf, void *ctx) {
    sort_array_ctx *c = (sort_array_ctx *)ctx;
    pk_buffer_append_char(buf, '[');
    for (size_t i = 0; i < c->n; i++) {
        if (i > 0) pk_buffer_append_char(buf, ',');
        emit_sort(buf, c->args[i]);
    }
    pk_buffer_append_char(buf, ']');
}

static void emit_sort_wrapper(pk_buffer *buf, void *ctx) {
    emit_sort(buf, (pk_sort *)ctx);
}

static void emit_sort(pk_buffer *buf, pk_sort *s) {
    switch (s->kind) {
        case PK_SORT_PRIMITIVE: {
            pk_field fields[] = {
                {"kind", (void (*)(pk_buffer *, void *))emit_string, "primitive"},
                {"name", (void (*)(pk_buffer *, void *))emit_string, s->data.primitive.name},
            };
            emit_object(buf, fields, 2);
            break;
        }
        case PK_SORT_FUNCTION: {
            sort_array_ctx args_ctx = { s->data.function.args, s->data.function.n_args };
            pk_field fields[] = {
                {"kind",   (void (*)(pk_buffer *, void *))emit_string, "function"},
                {"args",   emit_sort_array_fn, &args_ctx},
                {"return", emit_sort_wrapper, s->data.function.ret},
            };
            emit_object(buf, fields, 3);
            break;
        }
        case PK_SORT_DEPENDENT: {
            pk_field fields[] = {
                {"kind",      (void (*)(pk_buffer *, void *))emit_string, "dependent"},
                {"name",      (void (*)(pk_buffer *, void *))emit_string, s->data.dependent.name},
                {"indexVar",  (void (*)(pk_buffer *, void *))emit_string, s->data.dependent.index_var},
                {"indexSort", emit_sort_wrapper, s->data.dependent.index_sort},
            };
            emit_object(buf, fields, 4);
            break;
        }
    }
}

void pk_emit_sort(pk_buffer *buf, pk_sort *s) {
    emit_sort(buf, s);
}

/* ----------------------------------------------------------------------- */
/* Term                                                                    */
/* ----------------------------------------------------------------------- */

typedef struct { pk_term **args; size_t n; } term_array_ctx;

static void emit_term_array_fn(pk_buffer *buf, void *ctx) {
    term_array_ctx *c = (term_array_ctx *)ctx;
    pk_buffer_append_char(buf, '[');
    for (size_t i = 0; i < c->n; i++) {
        if (i > 0) pk_buffer_append_char(buf, ',');
        emit_term(buf, c->args[i]);
    }
    pk_buffer_append(buf, "]");
}

static void emit_term(pk_buffer *buf, pk_term *t) {
    switch (t->kind) {
        case PK_TERM_VAR: {
            pk_field fields[] = {
                {"kind", (void (*)(pk_buffer *, void *))emit_string, "var"},
                {"name", (void (*)(pk_buffer *, void *))emit_string, t->data.var.name},
            };
            emit_object(buf, fields, 2);
            break;
        }
        case PK_TERM_CONST: {
            /* Build object manually with sorted keys: kind, sort, value */
            pk_buffer_append_char(buf, '{');
            emit_string(buf, "kind"); pk_buffer_append_char(buf, ':'); emit_string(buf, "const"); pk_buffer_append_char(buf, ',');
            emit_string(buf, "sort"); pk_buffer_append_char(buf, ':'); emit_sort(buf, t->data.constant.sort); pk_buffer_append_char(buf, ',');
            emit_string(buf, "value"); pk_buffer_append_char(buf, ':');
            if (t->data.constant.is_string == 3) {
                pk_buffer_append(buf, "null");
            } else if (t->data.constant.is_string == 2) {
                int *v = (int *)t->data.constant.value;
                pk_buffer_append(buf, *v ? "true" : "false");
            } else if (t->data.constant.is_string == 1) {
                emit_string(buf, (const char *)t->data.constant.value);
            } else {
                int64_t *v = (int64_t *)t->data.constant.value;
                char numbuf[32];
                snprintf(numbuf, sizeof(numbuf), "%ld", (long)*v);
                pk_buffer_append(buf, numbuf);
            }
            pk_buffer_append_char(buf, '}');
            break;
        }
        case PK_TERM_CTOR: {
            term_array_ctx ctx = { t->data.ctor.args, t->data.ctor.n_args };
            pk_field fields[] = {
                {"args",  emit_term_array_fn, &ctx},
                {"kind",  (void (*)(pk_buffer *, void *))emit_string, "ctor"},
                {"name",  (void (*)(pk_buffer *, void *))emit_string, t->data.ctor.name},
            };
            emit_object(buf, fields, 3);
            break;
        }
    }
}

void pk_emit_term(pk_buffer *buf, pk_term *t) {
    emit_term(buf, t);
}

/* ----------------------------------------------------------------------- */
/* Formula                                                                 */
/* ----------------------------------------------------------------------- */

typedef struct { pk_formula **ops; size_t n; } formula_array_ctx;

static void emit_formula_array_fn(pk_buffer *buf, void *ctx) {
    formula_array_ctx *c = (formula_array_ctx *)ctx;
    pk_buffer_append_char(buf, '[');
    for (size_t i = 0; i < c->n; i++) {
        if (i > 0) pk_buffer_append_char(buf, ',');
        emit_formula(buf, c->ops[i]);
    }
    pk_buffer_append(buf, "]");
}

static void emit_formula(pk_buffer *buf, pk_formula *f) {
    switch (f->kind) {
        case PK_FORMULA_ATOMIC: {
            term_array_ctx ctx = { f->data.atomic.args, f->data.atomic.n_args };
            pk_field fields[] = {
                {"args",  emit_term_array_fn, &ctx},
                {"kind",  (void (*)(pk_buffer *, void *))emit_string, "atomic"},
                {"name",  (void (*)(pk_buffer *, void *))emit_string, f->data.atomic.name},
            };
            emit_object(buf, fields, 3);
            break;
        }
        case PK_FORMULA_CONNECTIVE: {
            formula_array_ctx ctx = { f->data.connective.operands, f->data.connective.n_operands };
            pk_field fields[] = {
                {"kind",      (void (*)(pk_buffer *, void *))emit_string, f->data.connective.kind},
                {"operands",  emit_formula_array_fn, &ctx},
            };
            emit_object(buf, fields, 2);
            break;
        }
        case PK_FORMULA_QUANTIFIER: {
            pk_field fields[] = {
                {"body", emit_formula_wrapper2, f->data.quantifier.body},
                {"kind", (void (*)(pk_buffer *, void *))emit_string, f->data.quantifier.kind},
                {"name", (void (*)(pk_buffer *, void *))emit_string, f->data.quantifier.name},
                {"sort", (void (*)(pk_buffer *, void *))emit_sort, f->data.quantifier.sort},
            };
            emit_object(buf, fields, 4);
            break;
        }
    }
}

void pk_emit_formula(pk_buffer *buf, pk_formula *f) {
    emit_formula(buf, f);
}

/* ----------------------------------------------------------------------- */
/* Declarations                                                            */
/* ----------------------------------------------------------------------- */

static void emit_formula_wrapper(pk_buffer *buf, void *ctx) {
    emit_formula(buf, (pk_formula *)ctx);
}

static void emit_string_wrapper(pk_buffer *buf, void *ctx) {
    emit_string(buf, (const char *)ctx);
}

static void emit_contract(pk_buffer *buf, pk_decl_contract *c) {
    pk_field fields[6];
    size_t n = 0;
    fields[n++] = (pk_field){"kind",       emit_string_wrapper, "contract"};
    fields[n++] = (pk_field){"name",       emit_string_wrapper, c->name};
    fields[n++] = (pk_field){"outBinding", emit_string_wrapper, c->out_binding};
    if (c->pre)  fields[n++] = (pk_field){"pre",  emit_formula_wrapper, c->pre};
    if (c->post) fields[n++] = (pk_field){"post", emit_formula_wrapper, c->post};
    if (c->inv)  fields[n++] = (pk_field){"inv",  emit_formula_wrapper, c->inv};
    emit_object(buf, fields, n);
}

static void emit_bridge(pk_buffer *buf, pk_decl_bridge *b) {
    pk_field fields[9];
    size_t n = 0;
    fields[n++] = (pk_field){"kind",              emit_string_wrapper, "bridge"};
    fields[n++] = (pk_field){"name",              emit_string_wrapper, b->name};
    fields[n++] = (pk_field){"sourceContractCid", emit_string_wrapper, b->source_contract_cid};
    fields[n++] = (pk_field){"sourceLayer",       emit_string_wrapper, b->source_layer};
    fields[n++] = (pk_field){"sourceSymbol",      emit_string_wrapper, b->source_symbol};
    fields[n++] = (pk_field){"targetContractCid", emit_string_wrapper, b->target_contract_cid};
    fields[n++] = (pk_field){"targetLayer",       emit_string_wrapper, b->target_layer};
    fields[n++] = (pk_field){"targetProofCid",    emit_string_wrapper, b->target_proof_cid};
    if (b->notes && b->notes[0]) {
        fields[n++] = (pk_field){"notes", emit_string_wrapper, b->notes};
    }
    emit_object(buf, fields, n);
}

void pk_emit_decl(pk_buffer *buf, pk_decl *d) {
    if (d->kind == PK_DECL_CONTRACT) {
        emit_contract(buf, &d->data.contract);
    } else {
        emit_bridge(buf, &d->data.bridge);
    }
}

void pk_emit_decls(pk_buffer *buf, pk_decl **decls, size_t n_decls) {
    pk_buffer_append_char(buf, '[');
    for (size_t i = 0; i < n_decls; i++) {
        if (i > 0) pk_buffer_append_char(buf, ',');
        pk_emit_decl(buf, decls[i]);
    }
    pk_buffer_append_char(buf, ']');
}
