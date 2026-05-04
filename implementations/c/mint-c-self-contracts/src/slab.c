/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Slab + formula DSL implementations. See slab.h.
 *
 * Memory model: every constructor returns a freshly-malloc'd value;
 * composing helpers take ownership of their argument values and place
 * them inside the returned tree. On allocation failure the helpers
 * return NULL after freeing partial state where possible — callers in
 * c_kit_invariants.c check the final post-formula for NULL and error
 * out the orchestrator if any author step OOMs.
 */

#include "slab.h"

#include <stdlib.h>
#include <string.h>

/* --- internal helpers --------------------------------------------------- */

static char *dup_cstr(const char *s) {
    if (!s) return NULL;
    size_t n = strlen(s);
    char *p = (char *)malloc(n + 1);
    if (!p) return NULL;
    memcpy(p, s, n + 1);
    return p;
}

/* Build {"kind":<kind>, ... user pairs ... }. The user pairs are passed as
 * (key, owned-value) parallel arrays; their values are placed into the
 * resulting object via pksc_v_obj_set (which takes ownership). On failure
 * everything is freed. Returns NULL on failure. */
static pksc_value *build_obj(const char *kind,
                             const char **keys,
                             pksc_value **values,
                             size_t n) {
    pksc_value *o = pksc_v_obj_new();
    if (!o) {
        for (size_t i = 0; i < n; i++) pksc_value_free(values[i]);
        return NULL;
    }
    pksc_value *kind_v = pksc_v_str(kind);
    if (!kind_v) goto fail;
    if (pksc_v_obj_set(o, "kind", kind_v) != 0) {
        pksc_value_free(kind_v);
        goto fail;
    }
    for (size_t i = 0; i < n; i++) {
        if (pksc_v_obj_set(o, keys[i], values[i]) != 0) {
            pksc_value_free(values[i]);
            /* remaining values still owned by us; free them */
            for (size_t j = i + 1; j < n; j++) pksc_value_free(values[j]);
            goto fail;
        }
    }
    return o;
fail:
    pksc_value_free(o);
    return NULL;
}

/* --- Sort / var / const ------------------------------------------------- */

pksc_value *mcsc_f_sort(const char *name) {
    pksc_value *name_v = pksc_v_str(name);
    if (!name_v) return NULL;
    const char *keys[] = { "name" };
    pksc_value *vals[] = { name_v };
    return build_obj("primitive", keys, vals, 1);
}

pksc_value *mcsc_f_var(const char *name) {
    pksc_value *name_v = pksc_v_str(name);
    if (!name_v) return NULL;
    const char *keys[] = { "name" };
    pksc_value *vals[] = { name_v };
    return build_obj("var", keys, vals, 1);
}

pksc_value *mcsc_f_str(const char *literal) {
    pksc_value *value_v = pksc_v_str(literal);
    pksc_value *sort_v = mcsc_f_sort("String");
    if (!value_v || !sort_v) {
        pksc_value_free(value_v);
        pksc_value_free(sort_v);
        return NULL;
    }
    const char *keys[] = { "value", "sort" };
    pksc_value *vals[] = { value_v, sort_v };
    return build_obj("const", keys, vals, 2);
}

pksc_value *mcsc_f_num(int64_t n) {
    pksc_value *value_v = pksc_v_int(n);
    pksc_value *sort_v = mcsc_f_sort("Int");
    if (!value_v || !sort_v) {
        pksc_value_free(value_v);
        pksc_value_free(sort_v);
        return NULL;
    }
    const char *keys[] = { "value", "sort" };
    pksc_value *vals[] = { value_v, sort_v };
    return build_obj("const", keys, vals, 2);
}

/* --- ctor / atomic ------------------------------------------------------ */

static pksc_value *build_named_with_args(const char *kind, const char *name,
                                         pksc_value **args, size_t n_args) {
    pksc_value *name_v = pksc_v_str(name);
    pksc_value *args_v = pksc_v_arr_new();
    if (!name_v || !args_v) {
        pksc_value_free(name_v);
        pksc_value_free(args_v);
        for (size_t i = 0; i < n_args; i++) pksc_value_free(args[i]);
        return NULL;
    }
    for (size_t i = 0; i < n_args; i++) {
        if (pksc_v_arr_push(args_v, args[i]) != 0) {
            pksc_value_free(args[i]);
            for (size_t j = i + 1; j < n_args; j++) pksc_value_free(args[j]);
            pksc_value_free(name_v);
            pksc_value_free(args_v);
            return NULL;
        }
    }
    const char *keys[] = { "name", "args" };
    pksc_value *vals[] = { name_v, args_v };
    return build_obj(kind, keys, vals, 2);
}

pksc_value *mcsc_f_ctor(const char *name, pksc_value **args, size_t n_args) {
    return build_named_with_args("ctor", name, args, n_args);
}

pksc_value *mcsc_f_app(const char *name, pksc_value **args, size_t n_args) {
    return build_named_with_args("atomic", name, args, n_args);
}

pksc_value *mcsc_f_eq(pksc_value *left, pksc_value *right) {
    pksc_value *args[2] = { left, right };
    return mcsc_f_app("eq", args, 2);
}

pksc_value *mcsc_f_gte(pksc_value *left, pksc_value *right) {
    pksc_value *args[2] = { left, right };
    return mcsc_f_app("gte", args, 2);
}

pksc_value *mcsc_f_starts_with(pksc_value *left, pksc_value *right) {
    pksc_value *args[2] = { left, right };
    return mcsc_f_app("starts_with", args, 2);
}

/* --- forall ------------------------------------------------------------- */

pksc_value *mcsc_f_forall(const char *var_name, pksc_value *sort,
                          mcsc_body_fn build_body, void *ctx) {
    pksc_value *name_v = pksc_v_str(var_name);
    pksc_value *bound = mcsc_f_var(var_name);
    if (!name_v || !sort || !bound) {
        pksc_value_free(name_v);
        pksc_value_free(sort);
        pksc_value_free(bound);
        return NULL;
    }
    pksc_value *body = build_body(bound, ctx);
    if (!body) {
        pksc_value_free(name_v);
        pksc_value_free(sort);
        return NULL;
    }
    const char *keys[] = { "name", "sort", "body" };
    pksc_value *vals[] = { name_v, sort, body };
    return build_obj("forall", keys, vals, 3);
}

/* --- Slab + Collector --------------------------------------------------- */

mcsc_slab *mcsc_slab_new(const char *label, const char *path) {
    mcsc_slab *s = (mcsc_slab *)calloc(1, sizeof(*s));
    if (!s) return NULL;
    s->label = dup_cstr(label);
    s->path = dup_cstr(path);
    if (!s->label || !s->path) {
        mcsc_slab_free(s);
        return NULL;
    }
    return s;
}

static void mcsc_contract_free(mcsc_contract *c) {
    if (!c) return;
    free(c->name);
    free(c->out_binding);
    pksc_value_free(c->pre);
    pksc_value_free(c->post);
    pksc_value_free(c->inv);
    free(c);
}

void mcsc_slab_free(mcsc_slab *s) {
    if (!s) return;
    free(s->label);
    free(s->path);
    for (size_t i = 0; i < s->n; i++) mcsc_contract_free(s->contracts[i]);
    free(s->contracts);
    free(s);
}

int mcsc_slab_must(mcsc_slab *s, const char *name, pksc_value *post_formula) {
    if (!s || !name || !post_formula) {
        pksc_value_free(post_formula);
        return -1;
    }
    if (s->n == s->cap) {
        size_t new_cap = s->cap ? s->cap * 2 : 4;
        mcsc_contract **resized = (mcsc_contract **)realloc(
            s->contracts, new_cap * sizeof(*resized));
        if (!resized) {
            pksc_value_free(post_formula);
            return -1;
        }
        s->contracts = resized;
        s->cap = new_cap;
    }
    mcsc_contract *c = (mcsc_contract *)calloc(1, sizeof(*c));
    if (!c) {
        pksc_value_free(post_formula);
        return -1;
    }
    c->name = dup_cstr(name);
    c->out_binding = dup_cstr("out");
    if (!c->name || !c->out_binding) {
        pksc_value_free(post_formula);
        mcsc_contract_free(c);
        return -1;
    }
    c->post = post_formula;
    s->contracts[s->n++] = c;
    return 0;
}

/* --- SlabList ----------------------------------------------------------- */

mcsc_slab_list *mcsc_slab_list_new(void) {
    return (mcsc_slab_list *)calloc(1, sizeof(mcsc_slab_list));
}

void mcsc_slab_list_free(mcsc_slab_list *l) {
    if (!l) return;
    for (size_t i = 0; i < l->n; i++) mcsc_slab_free(l->slabs[i]);
    free(l->slabs);
    free(l);
}

int mcsc_slab_list_push(mcsc_slab_list *l, mcsc_slab *s) {
    if (!l || !s) return -1;
    if (l->n == l->cap) {
        size_t new_cap = l->cap ? l->cap * 2 : 4;
        mcsc_slab **resized = (mcsc_slab **)realloc(l->slabs, new_cap * sizeof(*resized));
        if (!resized) return -1;
        l->slabs = resized;
        l->cap = new_cap;
    }
    l->slabs[l->n++] = s;
    return 0;
}
