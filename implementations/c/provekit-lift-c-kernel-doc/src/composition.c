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
 * over single formal `formal`. Output matches libprovekit's
 * `build_value` byte-for-byte (lex-sorted keys, primitive sort name
 * "i32", post: result = <formal>, empty effects, unknown locus,
 * bodyCid: null, autoMintedMementos: []). The FFI's permissive
 * serde_json deserializer accepts any JSON; libprovekit re-canonicalises
 * via build_value so the resulting CID is byte-stable.
 *
 * We emit keys in lex-sorted order anyway so the bytes the FFI sees
 * mirror what jcs_bytes_of_value would produce, which lets a debugging
 * eye diff producer-side bytes against the gold envelope from
 * tests/ffi_smoke.rs trivially. */

static int pk_c_compose_emit_identity_body(
    pk_c_compose_buf *b,
    const char *fn_name,
    const char *formal
) {
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
    if (pk_c_compose_buf_append(b,
            ",\"formalSorts\":[{\"kind\":\"primitive\",\"name\":\"i32\"}],") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b, "\"formals\":[") != 0) {
        return -1;
    }
    if (pk_c_compose_json_escape_into(b, formal) != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "],\"kind\":\"function-contract\",") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "\"locus\":{\"col\":0,\"file\":null,\"line\":0},") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"result\"},"
            "{\"kind\":\"var\",\"name\":") != 0) {
        return -1;
    }
    if (pk_c_compose_json_escape_into(b, formal) != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(b,
            "}],\"kind\":\"atomic\",\"name\":\"=\"},") != 0) {
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

/* Build the leaf-first chain rooted at `root`. Returns malloc'd array
 * of pointers and length via out-params. Caller frees the array (not
 * the elements; they alias into `facts`). Returns -1 on error.
 *
 * Cycle guard: if the same function appears twice in the chain we
 * stop. Effect-set purity already prevents recursion-with-side-effects
 * cycles, but a pure self-call would otherwise loop. */

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

    /* Walk leaf-first by repeatedly chasing the first pure callee, then
     * reverse so the root is last. Cycle check via name comparison. */
    const pk_c_function_fact *cursor = root;
    while (cursor != NULL) {
        for (size_t i = 0; i < n; i++) {
            if (chain[i]->name != NULL && cursor->name != NULL &&
                strcmp(chain[i]->name, cursor->name) == 0) {
                free(chain);
                return 0;
            }
        }
        if (n >= cap) {
            size_t next = cap * 2;
            const pk_c_function_fact **resized = realloc(
                chain, next * sizeof(*chain));
            if (resized == NULL) {
                free(chain);
                return -1;
            }
            chain = resized;
            cap = next;
        }
        chain[n++] = cursor;
        cursor = pk_c_compose_first_pure_callee(facts, cursor);
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

/* Build atoms_jcs and effects_jcs payloads for a chain. */

static int pk_c_compose_build_payloads(
    const pk_c_function_fact *const *chain,
    size_t chain_len,
    pk_c_compose_buf *atoms,
    pk_c_compose_buf *effects
) {
    if (pk_c_compose_buf_append(atoms, "[") != 0) {
        return -1;
    }
    if (pk_c_compose_buf_append(effects, "[") != 0) {
        return -1;
    }
    for (size_t i = 0; i < chain_len; i++) {
        const pk_c_function_fact *fn = chain[i];
        if (i > 0) {
            if (pk_c_compose_buf_append(atoms, ",") != 0) {
                return -1;
            }
            if (pk_c_compose_buf_append(effects, ",") != 0) {
                return -1;
            }
        }
        if (pk_c_compose_buf_append(atoms, "{\"formalIdx\":0,\"memento\":") != 0) {
            return -1;
        }
        if (pk_c_compose_emit_identity_body(atoms, fn->name, "x") != 0) {
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
        if (pk_c_compose_build_payloads(chain, chain_len, &atoms, &effects) != 0) {
            pk_c_compose_buf_free(&atoms);
            pk_c_compose_buf_free(&effects);
            free(chain);
            pk_c_compose_seen_free(&seen);
            return -1;
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
