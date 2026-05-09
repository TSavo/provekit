/* SPDX-License-Identifier: Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provekit/c_lift_core.h"

/*
 * Emit a trivial pure-identity function-contract memento for each function
 * found in source_facts. The contract has:
 *   pre:  true (no precondition)
 *   post: result = formals[0]  (arity <= 1)
 *         result = tuple(x0,...,xN-1)  (arity >= 2)
 *   effects: empty (MVP unblock; callers with real effects are conservatively
 *            accepted by compose and discharged trivially by prove)
 *
 * Shape mirrors the pure-identity memento emitted by composition.c so that
 * provekit compose --rpc can chain any pair of synthesized contracts.
 */

/* Minimal growable buffer for JSON construction. */
typedef struct {
    char *data;
    size_t len;
    size_t cap;
} WBuf;

static int wbuf_init(WBuf *b) {
    b->len = 0;
    b->cap = 256;
    b->data = malloc(b->cap);
    if (!b->data) {
        b->cap = 0;
        return -1;
    }
    b->data[0] = '\0';
    return 0;
}

static void wbuf_free(WBuf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int wbuf_grow(WBuf *b, size_t need) {
    size_t next = b->cap ? b->cap : 256;
    char *data;

    while (next < b->len + need + 1) {
        if (next > ((size_t)-1) / 2) {
            return -1;
        }
        next *= 2;
    }
    data = realloc(b->data, next);
    if (!data) {
        return -1;
    }
    b->data = data;
    b->cap = next;
    return 0;
}

static int wbuf_append(WBuf *b, const char *s) {
    size_t n = strlen(s);
    if (wbuf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int wbuf_append_char(WBuf *b, char c) {
    if (wbuf_grow(b, 1) != 0) {
        return -1;
    }
    b->data[b->len++] = c;
    b->data[b->len] = '\0';
    return 0;
}

/* JSON-escape src into b, without surrounding quotes. */
static int wbuf_json_escape(WBuf *b, const char *src) {
    for (const unsigned char *p = (const unsigned char *)src; p && *p; p++) {
        switch (*p) {
        case '"':
            if (wbuf_append(b, "\\\"") != 0) return -1;
            break;
        case '\\':
            if (wbuf_append(b, "\\\\") != 0) return -1;
            break;
        case '\n':
            if (wbuf_append(b, "\\n") != 0) return -1;
            break;
        case '\r':
            if (wbuf_append(b, "\\r") != 0) return -1;
            break;
        case '\t':
            if (wbuf_append(b, "\\t") != 0) return -1;
            break;
        default:
            if (*p < 0x20) {
                char esc[7];
                (void)snprintf(esc, sizeof(esc), "\\u00%02x", *p);
                if (wbuf_append(b, esc) != 0) return -1;
            } else {
                if (wbuf_append_char(b, (char)*p) != 0) return -1;
            }
            break;
        }
    }
    return 0;
}

/* Append a JSON-quoted, escaped string to b. */
static int wbuf_append_quoted(WBuf *b, const char *s) {
    if (wbuf_append_char(b, '"') != 0) return -1;
    if (wbuf_json_escape(b, s) != 0) return -1;
    return wbuf_append_char(b, '"');
}

/*
 * Build a trivial function-contract JSON object for fn_name with n_arity
 * formal parameters. Written into b. Returns 0 on success, -1 on OOM.
 *
 * The wire format matches WireFunctionContractMemento in cmd_compose.rs,
 * which uses serde snake_case field names. Key fields:
 *   fnName, formals, formalSorts, returnSort, pre, post, effects,
 *   bodyCid, locus, autoMintedMementos, schemaVersion, kind
 */
static int build_trivial_contract(WBuf *b, const char *fn_name, int n_arity) {
    char formal_name[32];
    int i;

    if (wbuf_append(b, "{") != 0) return -1;
    if (wbuf_append(b, "\"auto_minted_mementos\":[],") != 0) return -1;
    if (wbuf_append(b, "\"body_cid\":null,") != 0) return -1;

    if (wbuf_append(b, "\"fn_name\":") != 0) return -1;
    if (wbuf_append_quoted(b, fn_name) != 0) return -1;
    if (wbuf_append_char(b, ',') != 0) return -1;

    if (wbuf_append(b, "\"formal_sorts\":[") != 0) return -1;
    for (i = 0; i < n_arity; i++) {
        if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
        if (wbuf_append(b, "{\"kind\":\"primitive\",\"name\":\"i32\"}") != 0) return -1;
    }
    if (wbuf_append(b, "],") != 0) return -1;

    if (wbuf_append(b, "\"formals\":[") != 0) return -1;
    for (i = 0; i < n_arity; i++) {
        if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
        (void)snprintf(formal_name, sizeof(formal_name), "x%d", i);
        if (wbuf_append_quoted(b, formal_name) != 0) return -1;
    }
    if (wbuf_append(b, "],") != 0) return -1;

    if (wbuf_append(b, "\"kind\":\"function-contract\",") != 0) return -1;
    if (wbuf_append(b, "\"locus\":{\"col\":0,\"file\":null,\"line\":0},") != 0) return -1;

    /*
     * post formula:
     *   arity == 0: true (no formal to reference; emitting result = x0 here
     *               would name a nonexistent formal and trip compose's
     *               formal_idx >= formals.len() check on every chain)
     *   arity == 1: result = x0
     *   arity >= 2: result = Ctor("tuple", [Var(x0), ..., Var(xN-1)])
     */
    if (n_arity == 0) {
        if (wbuf_append(b, "\"post\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"true\"},") != 0) return -1;
    } else if (n_arity == 1) {
        if (wbuf_append(b, "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"result\"},{\"kind\":\"var\",\"name\":\"x0\"}],\"kind\":\"atomic\",\"name\":\"=\"},") != 0) return -1;
    } else {
        if (wbuf_append(b, "\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"result\"},{\"args\":[") != 0) return -1;
        for (i = 0; i < n_arity; i++) {
            if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
            (void)snprintf(formal_name, sizeof(formal_name), "x%d", i);
            if (wbuf_append(b, "{\"kind\":\"var\",\"name\":") != 0) return -1;
            if (wbuf_append_quoted(b, formal_name) != 0) return -1;
            if (wbuf_append(b, "}") != 0) return -1;
        }
        if (wbuf_append(b, "],\"kind\":\"ctor\",\"name\":\"tuple\"}],\"kind\":\"atomic\",\"name\":\"=\"},") != 0) return -1;
    }

    if (wbuf_append(b, "\"pre\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"true\"},") != 0) return -1;
    if (wbuf_append(b, "\"effects\":{\"effects\":[]},") != 0) return -1;
    if (wbuf_append(b, "\"return_sort\":{\"kind\":\"primitive\",\"name\":\"i32\"}}") != 0) return -1;

    return 0;
}

/*
 * Copy call-edge and effect entries from facts->extraction_result into result.
 * Mirrors kernel_doc.c's append_core_result pattern.
 */
static int append_core_result(pk_c_lift_result *result, const pk_c_source_facts *facts) {
    if (facts == NULL || facts->extraction_result == NULL) {
        return 0;
    }
    return pk_c_lift_result_extend(result, facts->extraction_result);
}

pk_c_lift_result *pk_c_walker_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *facts;
    size_t i;

    if (!result) {
        return NULL;
    }
    if (!source) {
        return result;
    }

    facts = pk_c_parse_source_with_options(path, source, options);
    if (facts == NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"message\":\"parse failed\",\"severity\":\"error\"}");
        return result;
    }

    /* Bring in call edges and function-effects entries from the parse pass. */
    if (append_core_result(result, facts) != 0) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }

    /* Emit one trivial function-contract per function with a body. */
    for (i = 0; i < facts->n_functions; i++) {
        const pk_c_function_fact *fn = &facts->functions[i];
        WBuf b;

        if (!fn->has_body || fn->name == NULL || fn->name[0] == '\0') {
            continue;
        }

        if (wbuf_init(&b) != 0) {
            pk_c_source_facts_free(facts);
            pk_c_lift_result_free(result);
            return NULL;
        }

        if (build_trivial_contract(&b, fn->name, fn->n_arity) != 0) {
            wbuf_free(&b);
            pk_c_source_facts_free(facts);
            pk_c_lift_result_free(result);
            return NULL;
        }

        if (pk_c_lift_result_add_declaration(result, b.data) != 0) {
            wbuf_free(&b);
            pk_c_source_facts_free(facts);
            pk_c_lift_result_free(result);
            return NULL;
        }

        wbuf_free(&b);
    }

    pk_c_source_facts_free(facts);
    return result;
}

pk_c_lift_result *pk_c_walker_lift_source(const char *path, const char *source) {
    return pk_c_walker_lift_source_with_options(path, source, NULL);
}
