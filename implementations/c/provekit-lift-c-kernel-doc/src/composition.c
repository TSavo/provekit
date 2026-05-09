/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Composition pass for the C lifter, per Contract Composition Protocol
 * (CCP) v1.0.0 sections 4 (eager materialization) and 6.2 (C ABI FFI).
 *
 * Spec: protocol/specs/2026-05-09-contract-composition-protocol.md.
 *
 * After the core C parser has populated `pk_c_source_facts` with
 * functions, per-function effects (libclang AST backend only, see
 * effects.c), and call_sites, this pass walks the call-site graph,
 * identifies pure subtrees (every member of the chain has empty effect
 * set), and invokes `pk_compose_chain_contracts` from libprovekit's C
 * ABI to produce a ComposedFunctionContract memento.
 *
 * Eager / lazy equivalence (CCP section 4): both materialization paths
 * produce byte-identical CIDs because libprovekit owns the canonical
 * compose primitive. We send JSON envelopes per CCP section 6.2's
 * documented deviation: each atom is `{"memento": <body>, "formalIdx":
 * N}` and the parallel `effects_jcs` array equals each atom's embedded
 * `effects` field by-value.
 *
 * Important: the C lifter's existing per-function "contract"
 * declarations (kind="contract", emitted by add_contract in
 * kernel_doc.c) are kernel-doc preconditions, not full
 * FunctionContractMementos. The wire format the FFI accepts requires
 * `fnName`, `formals`, `formalSorts`, `returnSort`, `pre`, `post`,
 * `effects`, `locus`, `bodyCid`, `autoMintedMementos`, and
 * `schemaVersion` per `build_value` in libprovekit/src/compose.rs.
 *
 * For chains discovered via the call-site graph this pass synthesises
 * pure-identity-shaped mementos (post: result = formal_0) per pure
 * function so that the wire format round-trips. The composed CID
 * therefore reflects the structural shape of the chain (number of
 * atoms, formal-index linkage), not kernel-doc preconditions. This is
 * sufficient for BZ-COMPOSITION-001 cross-language equivalence: the
 * Rust side must lift the equivalent shape and produce the same CID.
 *
 * Conservative classification: a function is composable iff
 *   1. it has a body (parser saw `{`), AND
 *   2. its effect set is empty.
 *
 * A chain is composable iff every member is composable AND the chain
 * has at least 2 atoms (FFI returns ChainTooShort otherwise).
 *
 * Determinism for byte-stable composed CID:
 *   - Functions are walked in source order (libclang's order).
 *   - For each caller F we pick the first composable callee G (in
 *     callsite source order) and prepend G to the chain rooted at F;
 *     we then DFS from G in the same way. Tie-broken by source order.
 *   - A formal index of 0 is used uniformly. Real cross-language
 *     federation requires matching argument-position resolution but
 *     the structural property (same number of atoms, same chain shape)
 *     is what BZ-COMPOSITION-001 exercises in v0.
 *
 * The pass is a no-op when the FFI symbol is absent at link time
 * (regex-only stub builds). The Makefile gates compilation of this
 * file behind PK_C_ENABLE_FFI_COMPOSE which is set iff libclang AST
 * is enabled.
 */

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"
#include "provekit-compose.h"

/* -------------------------------------------------------------------- */
/* Small dynamic buffer for JSON construction. */

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} pk_c_compose_buf;

static int pk_c_compose_buf_init(pk_c_compose_buf *b) {
    b->cap = 256;
    b->len = 0;
    b->data = malloc(b->cap);
    if (b->data == NULL) {
        b->cap = 0;
        return -1;
    }
    b->data[0] = '\0';
    return 0;
}

static void pk_c_compose_buf_free(pk_c_compose_buf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int pk_c_compose_buf_grow(pk_c_compose_buf *b, size_t need) {
    size_t next = b->cap ? b->cap : 256;
    char *data;

    while (next < b->len + need + 1) {
        if (next > ((size_t)-1) / 2) {
            return -1;
        }
        next *= 2;
    }
    data = realloc(b->data, next);
    if (data == NULL) {
        return -1;
    }
    b->data = data;
    b->cap = next;
    return 0;
}

static int pk_c_compose_buf_append(pk_c_compose_buf *b, const char *s) {
    size_t n = strlen(s);
    if (pk_c_compose_buf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

/* -------------------------------------------------------------------- */
/* JSON-string escape (sufficient for ASCII identifiers). The lifter
 * only feeds C identifiers through here so the tight subset is fine. */

static int pk_c_compose_json_escape_into(pk_c_compose_buf *b, const char *s) {
    if (pk_c_compose_buf_append(b, "\"") != 0) {
        return -1;
    }
    for (const unsigned char *p = (const unsigned char *)(s == NULL ? "" : s);
         *p != '\0'; p++) {
        char ch[8];
        if (*p == '"') {
            if (pk_c_compose_buf_append(b, "\\\"") != 0) {
                return -1;
            }
        } else if (*p == '\\') {
            if (pk_c_compose_buf_append(b, "\\\\") != 0) {
                return -1;
            }
        } else if (*p == '\n') {
            if (pk_c_compose_buf_append(b, "\\n") != 0) {
                return -1;
            }
        } else if (*p == '\r') {
            if (pk_c_compose_buf_append(b, "\\r") != 0) {
                return -1;
            }
        } else if (*p == '\t') {
            if (pk_c_compose_buf_append(b, "\\t") != 0) {
                return -1;
            }
        } else if (*p < 0x20) {
            (void)snprintf(ch, sizeof(ch), "\\u00%02x", *p);
            if (pk_c_compose_buf_append(b, ch) != 0) {
                return -1;
            }
        } else {
            ch[0] = (char)*p;
            ch[1] = '\0';
            if (pk_c_compose_buf_append(b, ch) != 0) {
                return -1;
            }
        }
    }
    return pk_c_compose_buf_append(b, "\"");
}

/* -------------------------------------------------------------------- */
/* Build a pure-identity FunctionContractMemento body for `fn_name`
 * over `arity` formal parameters. Output matches libprovekit's
 * `build_value` byte-for-byte (lex-sorted keys, primitive sort name
 * "i32", post: result = formals[0], empty effects, unknown locus,
 * bodyCid: null, autoMintedMementos: []). The FFI's permissive
 * serde_json deserializer accepts any JSON; libprovekit re-canonicalises
 * via build_value so the resulting CID is byte-stable.
 *
 * Arity convention:
 *   - arity <= 1: a single formal named "x". Preserves byte-equivalence
 *     with the v1 single-formal synthetic memento so previously pinned
 *     composed CIDs (compose_three, compose_two in the v1 fixture)
 *     remain stable across this change.
 *   - arity >= 2: formals are "x0", "x1", ..., "x<N-1>" with
 *     formalSorts of length N, all primitive i32. Required so the
 *     composition pass can substitute an inner result at any
 *     formalIdx in [0, N) without libprovekit returning None on the
 *     `formal_idx >= outer.formals.len()` bounds check (compose.rs).
 *
 * We emit keys in lex-sorted order anyway so the bytes the FFI sees
 * mirror what jcs_bytes_of_value would produce, which lets a debugging
 * eye diff producer-side bytes against the gold envelope from
 * tests/ffi_smoke.rs trivially. */

static int pk_c_compose_emit_identity_body(
    pk_c_compose_buf *b,
    const char *fn_name,
    int arity
) {
    char formal_name[16];
    int n = arity < 1 ? 1 : arity;
    int legacy = (arity <= 1) ? 1 : 0;

    if (pk_c_compose_buf_append(b, "{") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"autoMintedMementos\":[],") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"bodyCid\":null,") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"effects\":[],") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"fnName\":") != 0) {
        return -1;
    }
    if (pk_c_compose_json_escape_into(b, fn_name) != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, ",\"formalSorts\":[") != 0) {
        return -1;
    }
    for (int i = 0; i < n; i++) {
        if (i > 0 && pk_c_compose_buf_append(b, ",") != 0) {
            return -1;
        }
        if (pk_c_compose_buf_append(b,
                "{\"kind\":\"primitive\",\"name\":\"i32\"}") != 0) {
            return -1;
        }
    }
    if (pk_c_compose_buf_append(b, "],") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"formals\":[") != 0) {
        return -1;
    }
    for (int i = 0; i < n; i++) {
        if (i > 0 && pk_c_compose_buf_append(b, ",") != 0) {
            return -1;
        }
        if (legacy) {
            (void)snprintf(formal_name, sizeof(formal_name), "x");
        } else {
            (void)snprintf(formal_name, sizeof(formal_name), "x%d", i);
        }
        if (pk_c_compose_json_escape_into(b, formal_name) != 0) {
            return -1;
        }
    }
    if (pk_c_compose_buf_append(b,
            "],\"kind\":\"function-contract\",") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "\"locus\":{\"col\":0,\"file\":null,\"line\":0},") != 0) {
        return -1;
    }
    /* post for arity <= 1: `result = formals[0]` (identity).
     * post for arity >= 2: `result = Ctor("tuple", [Var(x0), ...,
     * Var(xN-1)])`. The Ctor wrapper makes all formals appear in the
     * post so that compose_function_contracts' substitution
     * (`substitute_in_formula(outer.post, outer.formals[idx],
     * inner_value)`) produces measurably different output for
     * different formalIdx values. Without this, substitution at
     * idx=1 for an outer post that mentions only formals[0] is a
     * no-op and the composed CID would not depend on formalIdx; the
     * formalIdx differential would only surface via the wire-format
     * `formalIdx` field downstream, but Rule 5 / Rule 4 of CCP §9
     * require the substituted formula itself to carry the position
     * difference. find_result_equation in libprovekit/src/compose.rs
     * still matches because it looks for `result = <expr>` where
     * <expr> is whatever IrTerm we put on the RHS (Var or Ctor; both
     * are extracted as inner_value verbatim). */
    if (pk_c_compose_buf_append(b,
            "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"result\"},") != 0) {
        return -1;
    }
    if (legacy) {
        if (pk_c_compose_buf_append(b,
                "{\"kind\":\"var\",\"name\":\"x\"}") != 0) {
            return -1;
        }
    } else {
        /* Ctor("tuple", [Var(x0), ..., Var(xN-1)]). */
        if (pk_c_compose_buf_append(b,
                "{\"args\":[") != 0) {
            return -1;
        }
        for (int i = 0; i < n; i++) {
            if (i > 0 && pk_c_compose_buf_append(b, ",") != 0) {
                return -1;
            }
            (void)snprintf(formal_name, sizeof(formal_name), "x%d", i);
            if (pk_c_compose_buf_append(b,
                    "{\"kind\":\"var\",\"name\":") != 0) {
                return -1;
            }
            if (pk_c_compose_json_escape_into(b, formal_name) != 0) {
                return -1;
            }
            if (pk_c_compose_buf_append(b, "}") != 0) {
                return -1;
            }
        }
        if (pk_c_compose_buf_append(b,
                "],\"kind\":\"ctor\",\"name\":\"tuple\"}") != 0) {
            return -1;
        }
    }
    if (pk_c_compose_buf_append(b,
            "],\"kind\":\"atomic\",\"name\":\"=\"},") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "\"pre\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"true\"},") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "\"returnSort\":{\"kind\":\"primitive\",\"name\":\"i32\"},") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"schemaVersion\":\"1\"}") != 0) {
        return -1;
    }
    return 0;
}

/* -------------------------------------------------------------------- */
/* Function-fact lookup helpers. */

static const pk_c_function_fact *pk_c_compose_find_function(
    const pk_c_source_facts *facts,
    const char *name
) {
    if (name == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < facts->n_functions; i++) {
        const pk_c_function_fact *fn = &facts->functions[i];
        if (fn->name != NULL && strcmp(fn->name, name) == 0) {
            return fn;
        }
    }
    return NULL;
}

static int pk_c_compose_function_is_pure(const pk_c_function_fact *fn) {
    return fn != NULL && fn->has_body && fn->n_effects == 0;
}

/* Find the first composable callee of `fn` that is itself defined in
 * this translation unit and is pure. Source order is preserved by the
 * order call_sites were appended. Returns NULL if no such callee
 * exists, or the function's name (a pointer into `facts`). */

/* Find the first call_site whose caller is `caller_name` and callee is
 * `callee_name`. Returns NULL if no such site. Used by the chain-build
 * extension to inspect the args_json of a specific direct call edge. */

static const pk_c_call_site_fact *pk_c_compose_find_call_site(
    const pk_c_source_facts *facts,
    const char *caller_name,
    const char *callee_name
) {
    if (caller_name == NULL || callee_name == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < facts->n_call_sites; i++) {
        const pk_c_call_site_fact *cs = &facts->call_sites[i];
        if (cs->caller == NULL || cs->callee == NULL) {
            continue;
        }
        if (strcmp(cs->caller, caller_name) == 0 &&
            strcmp(cs->callee, callee_name) == 0) {
            return cs;
        }
    }
    return NULL;
}

/* Scan args_json for the first arg whose `text` field starts with
 * `<name>(`. Returns the arg's `position` field on hit (>= 0) or -1
 * if no match. The args_json shape is fixed by clang_ast.c's
 * pk_c_clang_extract_args:
 *   [{"position":N,"kind":"...","text":"..."}, ...]
 * We do a lightweight pass that looks for `"text":"<name>(`. The text
 * field is JSON-escaped but C identifiers and `(` survive escape
 * unchanged so substring search is sound here. The position is read
 * from the same arg object.
 *
 * The function does NOT use a real JSON parser because:
 *   1. The producer is in-tree (clang_ast.c) and the schema is stable.
 *   2. Pulling in a JSON dependency for a 3-field needle is overkill.
 *   3. The defensive default to -1 means false negatives degrade to
 *      formalIdx=0 (legacy behaviour) rather than miscomposing.
 *
 * Returns -1 if args_json is NULL, malformed, or no match found.
 *
 * Per CCP §9 Rule 1 (singular formal substitution): each arg position
 * referencing the same inner is its own composition opportunity. The
 * caller decides what to do with multiple matches; this resolver
 * surfaces only the first to keep determinism (source order). */

static int pk_c_compose_resolve_formal_idx_in_args(
    const char *args_json,
    const char *inner_name
) {
    if (args_json == NULL || inner_name == NULL || inner_name[0] == '\0') {
        return -1;
    }
    size_t name_len = strlen(inner_name);
    /* Build the literal needle: "text":"<name>( */
    /* Max identifier length here is bounded by C, but we malloc to be safe. */
    size_t needle_cap = name_len + 16;
    char *needle = malloc(needle_cap);
    if (needle == NULL) {
        return -1;
    }
    (void)snprintf(needle, needle_cap, "\"text\":\"%s(", inner_name);

    const char *hit = strstr(args_json, needle);
    free(needle);
    if (hit == NULL) {
        return -1;
    }

    /* Walk backwards from `hit` to find the enclosing arg object's
     * "position":<int> field. The args_json shape is
     *   {"position":N,"kind":"...","text":"<name>("...")"}
     * so "position": appears earlier in the same object. Find the
     * preceding "position": and parse the integer. */
    const char *pos_key = "\"position\":";
    size_t pos_key_len = strlen(pos_key);
    const char *p = hit;
    /* Search backwards for the most recent "position": occurrence. */
    while (p > args_json) {
        if ((size_t)(p - args_json) >= pos_key_len &&
            strncmp(p, pos_key, pos_key_len) == 0) {
            const char *digits = p + pos_key_len;
            int value = 0;
            int seen_digit = 0;
            while (*digits >= '0' && *digits <= '9') {
                value = value * 10 + (*digits - '0');
                digits++;
                seen_digit = 1;
            }
            if (!seen_digit) {
                return -1;
            }
            return value;
        }
        p--;
    }
    return -1;
}

/* High-level resolver: given an outer call_site (which exposes
 * args_json) and the name of an inner function, return the formal
 * index in the outer's argument list where `inner_name(...)` appears
 * as a nested call expression. Per CCP §9 Rule 1 this index IS the
 * formalIdx for composing inner into outer.
 *
 * Defensive fallback: returns 0 when:
 *   - the call_site or args_json is missing (regex backend, or args
 *     extraction skipped),
 *   - the inner name is not present as a nested-call arg (the chain
 *     was built via direct callee chasing without nested-call
 *     structure, e.g. compose_three -> add_one -> double_it).
 * The caller MAY emit a one-shot diagnostic in the fallback path; we
 * defer that decision so this function stays pure. */

static int pk_c_compose_resolve_formal_idx(
    const pk_c_call_site_fact *outer_call,
    const char *inner_name
) {
    int idx;

    if (outer_call == NULL || outer_call->args_json == NULL) {
        return 0;
    }
    idx = pk_c_compose_resolve_formal_idx_in_args(
        outer_call->args_json, inner_name);
    if (idx < 0) {
        return 0;
    }
    return idx;
}

static const pk_c_function_fact *pk_c_compose_first_pure_callee(
    const pk_c_source_facts *facts,
    const pk_c_function_fact *fn
) {
    if (fn == NULL || fn->name == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < facts->n_call_sites; i++) {
        const pk_c_call_site_fact *cs = &facts->call_sites[i];
        if (cs->caller == NULL || cs->callee == NULL) {
            continue;
        }
        if (strcmp(cs->caller, fn->name) != 0) {
            continue;
        }
        const pk_c_function_fact *callee = pk_c_compose_find_function(
            facts, cs->callee);
        if (pk_c_compose_function_is_pure(callee)) {
            return callee;
        }
    }
    return NULL;
}

/* Inspect the args_json of (caller -> callee) and return the first
 * defined-pure function whose name appears as a nested call expression
 * in the args. NULL if none. Used to extend the chain past F when F is
 * called with G(...) as one of its args (the [G, F] composition
 * opportunity per CCP §9 Rule 1). */

static const pk_c_function_fact *pk_c_compose_first_nested_pure_call(
    const pk_c_source_facts *facts,
    const pk_c_call_site_fact *cs
) {
    if (cs == NULL || cs->args_json == NULL) {
        return NULL;
    }
    /* Iterate the args_json once, looking for any "text":"<name>("
     * pattern where <name> is a defined pure function. Source order
     * (= libclang arg order) ensures determinism. */
    const char *p = cs->args_json;
    while (*p != '\0') {
        const char *needle = "\"text\":\"";
        const char *hit = strstr(p, needle);
        if (hit == NULL) {
            return NULL;
        }
        const char *text_start = hit + strlen(needle);
        /* Read identifier characters (a-z, A-Z, 0-9, _) as candidate
         * function name; require a `(` immediately after. */
        const char *q = text_start;
        while ((*q >= 'a' && *q <= 'z') || (*q >= 'A' && *q <= 'Z') ||
               (*q >= '0' && *q <= '9') || *q == '_') {
            q++;
        }
        if (q > text_start && *q == '(') {
            size_t name_len = (size_t)(q - text_start);
            char *name = malloc(name_len + 1);
            if (name == NULL) {
                return NULL;
            }
            memcpy(name, text_start, name_len);
            name[name_len] = '\0';
            const pk_c_function_fact *fn = pk_c_compose_find_function(
                facts, name);
            free(name);
            if (pk_c_compose_function_is_pure(fn)) {
                return fn;
            }
        }
        p = text_start;
        /* Advance past this text field's opening so we don't re-match. */
        while (*p != '\0' && *p != '"') {
            p++;
        }
        if (*p == '"') {
            p++;
        }
    }
    return NULL;
}

/* Append `fn` to the chain, growing as needed. Returns 0 on success,
 * 1 if `fn` is already in the chain (cycle / dedup), -1 on alloc
 * failure. */

static int pk_c_compose_chain_push(
    const pk_c_function_fact ***chain,
    size_t *n,
    size_t *cap,
    const pk_c_function_fact *fn
) {
    for (size_t i = 0; i < *n; i++) {
        if ((*chain)[i]->name != NULL && fn->name != NULL &&
            strcmp((*chain)[i]->name, fn->name) == 0) {
            return 1;
        }
    }
    if (*n >= *cap) {
        size_t next = (*cap) * 2;
        const pk_c_function_fact **resized = realloc(
            *chain, next * sizeof(**chain));
        if (resized == NULL) {
            return -1;
        }
        *chain = resized;
        *cap = next;
    }
    (*chain)[(*n)++] = fn;
    return 0;
}

/* Build the leaf-first chain rooted at `root`. Returns malloc'd array
 * of pointers and length via out-params. Caller frees the array (not
 * the elements; they alias into `facts`). Returns -1 on error.
 *
 * Strategy (caller-first then reverse):
 *   1. Append root.
 *   2. Find root's first pure direct callee F. Append F.
 *   3. Inspect the (root -> F) call_site's args_json. If any arg is a
 *      nested call expression `G(...)` for some defined pure G,
 *      append G as the deeper leaf and continue chasing from G.
 *      Otherwise continue chasing from F.
 *   4. Repeat step 2-3 until no further pure callee is found.
 *
 * Cycle guard: if the same function appears twice in the chain we
 * stop. Effect-set purity already prevents recursion-with-side-effects
 * cycles, but a pure self-call would otherwise loop.
 *
 * The nested-call extension realises CCP §9 Rule 1 structurally: a
 * call site where one arg is itself a function call IS a composition
 * opportunity for that inner. Without it, expressions like
 * `outer(inner(a), b)` would yield a chain of [outer, root] only and
 * the inner / outer composition (the load-bearing per-position
 * substitution) would never be exercised. */

static int pk_c_compose_build_chain(
    const pk_c_source_facts *facts,
    const pk_c_function_fact *root,
    const pk_c_function_fact ***out_chain,
    size_t *out_len
) {
    *out_chain = NULL;
    *out_len = 0;
    if (root == NULL || !pk_c_compose_function_is_pure(root)) {
        return 0;
    }

    size_t cap = 4;
    const pk_c_function_fact **chain = malloc(cap * sizeof(*chain));
    if (chain == NULL) {
        return -1;
    }
    size_t n = 0;

    int rc = pk_c_compose_chain_push(&chain, &n, &cap, root);
    if (rc != 0) {
        free(chain);
        return rc < 0 ? -1 : 0;
    }

    const pk_c_function_fact *caller = root;
    while (caller != NULL) {
        const pk_c_function_fact *callee = pk_c_compose_first_pure_callee(
            facts, caller);
        if (callee == NULL) {
            break;
        }
        rc = pk_c_compose_chain_push(&chain, &n, &cap, callee);
        if (rc < 0) {
            free(chain);
            return -1;
        }
        if (rc == 1) {
            /* Already in chain (cycle); stop without erroring. */
            break;
        }

        /* Look at (caller -> callee) for a nested-call leaf. The
         * args_json of that call_site IS where CCP §9 Rule 1 lives. */
        const pk_c_call_site_fact *cs = pk_c_compose_find_call_site(
            facts, caller->name, callee->name);
        const pk_c_function_fact *nested =
            pk_c_compose_first_nested_pure_call(facts, cs);
        if (nested != NULL) {
            int prc = pk_c_compose_chain_push(&chain, &n, &cap, nested);
            if (prc < 0) {
                free(chain);
                return -1;
            }
            if (prc == 1) {
                /* Already present; stop walking deeper. */
                break;
            }
            /* Continue chasing from the nested-call leaf. */
            caller = nested;
        } else {
            caller = callee;
        }
    }

    if (n < 2) {
        free(chain);
        return 0;
    }

    /* Reverse so chain[0] is the leaf and chain[n-1] is `root`, per
     * CCP §2 ordering: "atoms ordered by call-graph depth, leaf
     * first". */
    for (size_t i = 0, j = n - 1; i < j; i++, j--) {
        const pk_c_function_fact *tmp = chain[i];
        chain[i] = chain[j];
        chain[j] = tmp;
    }

    *out_chain = chain;
    *out_len = n;
    return 0;
}

/* Build atoms_jcs and effects_jcs payloads for a chain.
 *
 * Per CCP §9 Rule 1 (singular formal substitution) and the wire
 * format documented in §6.2, each atom carries `formalIdx`: where in
 * THIS atom's formals the previous atom's result substitutes. Step 0
 * is the leaf; its formalIdx is unused by libprovekit
 * (compose_chain_contracts uses steps[1].formal_idx onward). For
 * subsequent steps the resolver looks at the call_site whose callee
 * is this step and whose args list contains the prior step as a
 * nested call expression; the matching arg's position IS the
 * formalIdx. Defensive default 0 with one-shot diagnostic when no
 * match is found (e.g. the prior step is a direct callee like
 * `add_one(x)` with `x` as the arg, no nested call).
 *
 * `diag_facts` is consulted only to surface the diagnostic by name in
 * case of a fallback; pass NULL to suppress. */

static int pk_c_compose_build_payloads(
    const pk_c_source_facts *facts,
    const pk_c_function_fact *const *chain,
    size_t chain_len,
    pk_c_compose_buf *atoms,
    pk_c_compose_buf *effects,
    int *out_n_fallback
) {
    if (pk_c_compose_buf_append(atoms, "[") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(effects, "[") != 0) {
        return -1;
    }
    if (out_n_fallback != NULL) {
        *out_n_fallback = 0;
    }
    for (size_t i = 0; i < chain_len; i++) {
        const pk_c_function_fact *fn = chain[i];
        char header[64];
        int formal_idx = 0;

        if (i > 0) {
            /* Resolve formalIdx for chain[i] composing chain[i-1] as
             * inner. The relevant call_site is the one where chain[i]
             * is called WITH chain[i-1] as a nested arg expression.
             * In the chain (leaf-first ordering after reverse), that
             * call_site lives in chain[i+1]'s body when i < len-1
             * (chain[i+1] is the function that contains the
             * `chain[i](..., chain[i-1](...), ...)` expression).
             *
             * For the topmost step i = chain_len - 1 there is no
             * chain[i+1]; default to 0. The composition primitive
             * only uses formal_idx for steps[1..]; the topmost step's
             * formal_idx contributes to where the previous-composed
             * result substitutes into the root. We default to 0
             * because the root typically wraps the chain's final
             * call as its return expression rather than as a nested
             * arg of yet another call. */
            int resolved = -1;
            if (i + 1 < chain_len) {
                const pk_c_call_site_fact *cs = pk_c_compose_find_call_site(
                    facts, chain[i + 1]->name, fn->name);
                /* High-level resolver: returns 0 on miss (defensive
                 * fallback per task contract) and the matching arg
                 * position on hit. We re-run the strict variant
                 * below to distinguish "matched at position 0" from
                 * "no match". */
                if (cs != NULL) {
                    int idx = pk_c_compose_resolve_formal_idx(
                        cs, chain[i - 1]->name);
                    int strict = pk_c_compose_resolve_formal_idx_in_args(
                        cs->args_json, chain[i - 1]->name);
                    /* The strict variant returns -1 on miss; the
                     * high-level resolver collapses miss to 0. Use
                     * strict to detect genuine misses; otherwise
                     * trust the high-level value. */
                    if (strict >= 0) {
                        resolved = idx;
                    }
                }
            }
            if (resolved < 0) {
                /* No matching call_site (i.e., chain[i-1] does not
                 * appear as a nested-call arg in chain[i+1]'s call to
                 * chain[i]) OR we are at the topmost step. Default to
                 * 0 and surface a one-line diagnostic when
                 * PK_C_COMPOSE_DEBUG is set so debuggers can spot
                 * when a chain falls back. */
                if (out_n_fallback != NULL) {
                    (*out_n_fallback)++;
                }
                formal_idx = 0;
            } else {
                formal_idx = resolved;
            }
        }

        if (i > 0) {
            if (pk_c_compose_buf_append(atoms, ",") != 0) {
                return -1;
            }
            if (pk_c_compose_buf_append(effects, ",") != 0) {
                return -1;
            }
        }
        (void)snprintf(header, sizeof(header),
            "{\"formalIdx\":%d,\"memento\":", formal_idx);
        if (pk_c_compose_buf_append(atoms, header) != 0) {
            return -1;
        }
        if (pk_c_compose_emit_identity_body(atoms, fn->name, fn->n_arity) != 0) {
            return -1;
        }
        if (pk_c_compose_buf_append(atoms, "}") != 0) {
            return -1;
        }
        /* Pure atoms: empty effect array. The FFI's effects-mismatch
         * check requires this to equal the embedded
         * memento.effects (also `[]`). */
        if (pk_c_compose_buf_append(effects, "[]") != 0) {
            return -1;
        }
    }
    if (pk_c_compose_buf_append(atoms, "]") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(effects, "]") != 0) {
        return -1;
    }
    return 0;
}

/* Emit a composed-contract declaration into `result`. Conservative
 * minimal shape: kind, function (root name), composedCid, atoms list
 * (function names, leaf first), bodyJcs. */

static int pk_c_compose_emit_decl(
    pk_c_lift_result *result,
    const pk_c_function_fact *const *chain,
    size_t chain_len,
    const char *cid,
    const char *body_jcs
) {
    pk_c_compose_buf out;

    if (pk_c_compose_buf_init(&out) != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(&out, "{\"kind\":\"composed-contract\",") != 0) {
        goto fail;
    }
    if (pk_c_compose_buf_append(&out, "\"function\":") != 0) {
        goto fail;
    }
    if (pk_c_compose_json_escape_into(&out, chain[chain_len - 1]->name) != 0) {
        goto fail;
    }
    if (pk_c_compose_buf_append(&out, ",\"atoms\":[") != 0) {
        goto fail;
    }
    for (size_t i = 0; i < chain_len; i++) {
        if (i > 0) {
            if (pk_c_compose_buf_append(&out, ",") != 0) {
                goto fail;
            }
        }
        if (pk_c_compose_json_escape_into(&out, chain[i]->name) != 0) {
            goto fail;
        }
    }
    if (pk_c_compose_buf_append(&out, "],\"composedCid\":") != 0) {
        goto fail;
    }
    if (pk_c_compose_json_escape_into(&out, cid) != 0) {
        goto fail;
    }
    if (pk_c_compose_buf_append(&out, ",\"bodyJcs\":") != 0) {
        goto fail;
    }
    if (pk_c_compose_json_escape_into(&out, body_jcs) != 0) {
        goto fail;
    }
    if (pk_c_compose_buf_append(&out, ",\"ccpVersion\":\"1.0.0\"}") != 0) {
        goto fail;
    }
    if (pk_c_lift_result_add_declaration(result, out.data) != 0) {
        goto fail;
    }
    pk_c_compose_buf_free(&out);
    return 0;

fail:
    pk_c_compose_buf_free(&out);
    return -1;
}

/* Track which function names have already been emitted as the root of
 * a composed chain so we don't double-emit when the same chain is
 * reachable from multiple ancestors. */

typedef struct {
    char **names;
    size_t len;
    size_t cap;
} pk_c_compose_seen;

static int pk_c_compose_seen_add(pk_c_compose_seen *s, const char *name) {
    for (size_t i = 0; i < s->len; i++) {
        if (strcmp(s->names[i], name) == 0) {
            return 1; /* already seen */
        }
    }
    if (s->len >= s->cap) {
        size_t next = s->cap == 0 ? 4 : s->cap * 2;
        char **resized = realloc(s->names, next * sizeof(*resized));
        if (resized == NULL) {
            return -1;
        }
        s->names = resized;
        s->cap = next;
    }
    s->names[s->len] = malloc(strlen(name) + 1);
    if (s->names[s->len] == NULL) {
        return -1;
    }
    memcpy(s->names[s->len], name, strlen(name) + 1);
    s->len++;
    return 0;
}

static void pk_c_compose_seen_free(pk_c_compose_seen *s) {
    for (size_t i = 0; i < s->len; i++) {
        free(s->names[i]);
    }
    free(s->names);
    s->names = NULL;
    s->len = 0;
    s->cap = 0;
}

/* -------------------------------------------------------------------- */
/* Public entry point: walk pure chains, call the FFI, emit composed
 * contracts. Returns 0 on success (including the no-chain case);
 * non-zero only on internal errors. Composition refusal from the FFI
 * is treated as data ("we couldn't compose this chain") and silently
 * skipped: the rest of the lifter output is unaffected. */

int pk_c_composition_emit(pk_c_lift_result *result, const pk_c_source_facts *facts) {
    if (result == NULL || facts == NULL) {
        return 0;
    }

    pk_c_compose_seen seen = {0};

    for (size_t i = 0; i < facts->n_functions; i++) {
        const pk_c_function_fact *root = &facts->functions[i];
        if (!pk_c_compose_function_is_pure(root)) {
            continue;
        }

        const pk_c_function_fact **chain = NULL;
        size_t chain_len = 0;
        if (pk_c_compose_build_chain(facts, root, &chain, &chain_len) != 0) {
            pk_c_compose_seen_free(&seen);
            return -1;
        }
        if (chain == NULL || chain_len < 2) {
            free(chain);
            continue;
        }

        /* De-dup: only emit one composed-contract per root function. */
        int dup = pk_c_compose_seen_add(&seen, root->name);
        if (dup < 0) {
            free(chain);
            pk_c_compose_seen_free(&seen);
            return -1;
        }
        if (dup == 1) {
            free(chain);
            continue;
        }

        pk_c_compose_buf atoms;
        pk_c_compose_buf effects;
        if (pk_c_compose_buf_init(&atoms) != 0) {
            free(chain);
            pk_c_compose_seen_free(&seen);
            return -1;
        }
        if (pk_c_compose_buf_init(&effects) != 0) {
            pk_c_compose_buf_free(&atoms);
            free(chain);
            pk_c_compose_seen_free(&seen);
            return -1;
        }
        int n_fallback = 0;
        if (pk_c_compose_build_payloads(
                facts, chain, chain_len, &atoms, &effects, &n_fallback) != 0) {
            pk_c_compose_buf_free(&atoms);
            pk_c_compose_buf_free(&effects);
            free(chain);
            pk_c_compose_seen_free(&seen);
            return -1;
        }
        /* Surface a one-line debug note on stderr when one or more
         * steps fell back to formalIdx=0 because no nested-call
         * resolution was found. This is informational only; it does
         * not change the composed CID. Helpful when comparing
         * composed CIDs across changes to the resolver. */
        if (n_fallback > 0 && getenv("PK_C_COMPOSE_DEBUG") != NULL) {
            (void)fprintf(stderr,
                "provekit-lift-c-kernel-doc: composition for %s used "
                "formalIdx=0 fallback for %d step(s) (no nested-call "
                "resolution found; chain length %zu)\n",
                root->name, n_fallback, chain_len);
        }

        pk_composition_result *r = pk_compose_chain_contracts(
            atoms.data, effects.data, atoms.len, effects.len);
        if (r == NULL) {
            /* Defensive: the FFI contract guarantees non-NULL but if a
             * future libprovekit revision regresses, fail soft. */
            pk_c_compose_buf_free(&atoms);
            pk_c_compose_buf_free(&effects);
            free(chain);
            continue;
        }
        const char *err = pk_composition_result_error(r);
        if (err != NULL) {
            /* Skip this chain; composition refused (impure / shape
             * issue / schema). Other chains may still succeed. */
            pk_composition_result_free(r);
            pk_c_compose_buf_free(&atoms);
            pk_c_compose_buf_free(&effects);
            free(chain);
            continue;
        }
        const char *cid = pk_composition_result_cid(r);
        const char *body = pk_composition_result_body_jcs(r);
        if (cid == NULL || body == NULL) {
            pk_composition_result_free(r);
            pk_c_compose_buf_free(&atoms);
            pk_c_compose_buf_free(&effects);
            free(chain);
            continue;
        }
        int rc = pk_c_compose_emit_decl(result, chain, chain_len, cid, body);
        pk_composition_result_free(r);
        pk_c_compose_buf_free(&atoms);
        pk_c_compose_buf_free(&effects);
        free(chain);
        if (rc != 0) {
            pk_c_compose_seen_free(&seen);
            return -1;
        }
    }

    pk_c_compose_seen_free(&seen);
    return 0;
}
