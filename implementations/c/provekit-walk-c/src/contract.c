/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"

#ifdef PK_C_ENABLE_CLANG_AST

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} ContractBuf;

static int buf_init(ContractBuf *b) {
    b->len = 0;
    b->cap = 512;
    b->data = malloc(b->cap);
    if (b->data == NULL) {
        b->cap = 0;
        return -1;
    }
    b->data[0] = '\0';
    return 0;
}

static void buf_free(ContractBuf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int buf_grow(ContractBuf *b, size_t need) {
    size_t next = b->cap ? b->cap : 512;
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

static int buf_append_n(ContractBuf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int buf_append(ContractBuf *b, const char *s) {
    return buf_append_n(b, s, strlen(s));
}

static int buf_append_char(ContractBuf *b, char c) {
    return buf_append_n(b, &c, 1);
}

static int buf_append_quoted(ContractBuf *b, const char *src) {
    const unsigned char *p = (const unsigned char *)(src == NULL ? "" : src);

    if (buf_append_char(b, '"') != 0) return -1;
    for (; *p != '\0'; p++) {
        char esc[7];

        switch (*p) {
        case '"':
            if (buf_append(b, "\\\"") != 0) return -1;
            break;
        case '\\':
            if (buf_append(b, "\\\\") != 0) return -1;
            break;
        case '\n':
            if (buf_append(b, "\\n") != 0) return -1;
            break;
        case '\r':
            if (buf_append(b, "\\r") != 0) return -1;
            break;
        case '\t':
            if (buf_append(b, "\\t") != 0) return -1;
            break;
        default:
            if (*p < 0x20) {
                (void)snprintf(esc, sizeof(esc), "\\u00%02x", *p);
                if (buf_append(b, esc) != 0) return -1;
            } else if (buf_append_char(b, (char)*p) != 0) {
                return -1;
            }
            break;
        }
    }
    return buf_append_char(b, '"');
}

static int append_sort(ContractBuf *b) {
    return buf_append(b, "{\"kind\":\"primitive\",\"name\":\"i32\"}");
}

static int append_formula(ContractBuf *b, const pk_c_walk_formula *formula) {
    char *json = pk_c_walk_formula_json(formula);
    int rc;

    if (json == NULL) {
        return -1;
    }
    rc = buf_append(b, json);
    free(json);
    return rc;
}

static int append_arrival(ContractBuf *b, const pk_c_walk_arrival *arrival) {
    char idx[32];
    char join_idx[32];
    char line[32];
    char col[32];

    (void)snprintf(idx, sizeof(idx), "%zu", arrival->stmt_index);
    (void)snprintf(join_idx, sizeof(join_idx), "%zu", arrival->join_stmt_index);
    (void)snprintf(line, sizeof(line), "%d", arrival->line);
    (void)snprintf(col, sizeof(col), "%d", arrival->column);
    if (buf_append(b, "{\"column\":") != 0 ||
        buf_append(b, col) != 0 ||
        buf_append(b, ",\"kind\":") != 0 ||
        buf_append_quoted(b, arrival->kind) != 0 ||
        buf_append(b, ",\"line\":") != 0 ||
        buf_append(b, line) != 0 ||
        buf_append(b, ",\"name\":") != 0 ||
        buf_append_quoted(b, arrival->name) != 0 ||
        buf_append(b, ",\"stmt_index\":") != 0 ||
        buf_append(b, idx) != 0) {
        return -1;
    }
    if (strcmp(arrival->kind, "ConditionalArm") == 0) {
        if (arrival->branch == NULL || arrival->cond == NULL ||
            buf_append(b, ",\"branch\":") != 0 ||
            buf_append_quoted(b, arrival->branch) != 0 ||
            buf_append(b, ",\"cond\":") != 0 ||
            append_formula(b, arrival->cond) != 0 ||
            buf_append(b, ",\"join_stmt_index\":") != 0 ||
            buf_append(b, join_idx) != 0) {
            return -1;
        }
    }
    if (buf_append(b, ",\"wp\":") != 0 ||
        append_formula(b, arrival->wp) != 0 ||
        buf_append_char(b, '}') != 0) {
        return -1;
    }
    return 0;
}

int pk_c_walk_emit_chain_contract(pk_c_lift_result *result, const pk_c_walk_chain *chain) {
    ContractBuf b;
    char name[512];
    char line[32];
    char col[32];
    int rc = -1;

    if (chain == NULL || chain->n_arrivals == 0) {
        return 0;
    }
    (void)snprintf(name, sizeof(name), "%s__wp__%s__%d_%d",
        chain->caller_name == NULL ? "caller" : chain->caller_name,
        chain->callee_name == NULL ? "callee" : chain->callee_name,
        chain->line,
        chain->column);
    (void)snprintf(line, sizeof(line), "%d", chain->line);
    (void)snprintf(col, sizeof(col), "%d", chain->column);
    if (buf_init(&b) != 0) {
        return -1;
    }
    if (buf_append(&b, "{\"auto_minted_mementos\":[],\"body_cid\":null,") != 0) goto done;
    if (buf_append(&b, "\"evidence\":{\"arrivals\":[") != 0) goto done;
    for (size_t i = 0; i < chain->n_arrivals; i++) {
        if ((i > 0 && buf_append_char(&b, ',') != 0) ||
            append_arrival(&b, &chain->arrivals[i]) != 0) {
            goto done;
        }
    }
    if (buf_append(&b, "],\"callee\":") != 0 ||
        buf_append_quoted(&b, chain->callee_name) != 0 ||
        buf_append(&b, ",\"caller\":") != 0 ||
        buf_append_quoted(&b, chain->caller_name) != 0 ||
        buf_append(&b, ",\"kind\":\"wp-walk-chain\"},") != 0) {
        goto done;
    }
    if (buf_append(&b, "\"effects\":{\"effects\":[]},\"fn_name\":") != 0 ||
        buf_append_quoted(&b, name) != 0 ||
        buf_append(&b, ",\"formal_sorts\":[") != 0) {
        goto done;
    }
    for (size_t i = 0; i < chain->n_formals; i++) {
        if ((i > 0 && buf_append_char(&b, ',') != 0) || append_sort(&b) != 0) {
            goto done;
        }
    }
    if (buf_append(&b, "],\"formals\":[") != 0) goto done;
    for (size_t i = 0; i < chain->n_formals; i++) {
        if ((i > 0 && buf_append_char(&b, ',') != 0) ||
            buf_append_quoted(&b, chain->formals[i]) != 0) {
            goto done;
        }
    }
    if (buf_append(&b, "],\"kind\":\"function-contract\",\"locus\":{\"col\":") != 0 ||
        buf_append(&b, col) != 0 ||
        buf_append(&b, ",\"file\":") != 0 ||
        (chain->path == NULL ? buf_append(&b, "null") : buf_append_quoted(&b, chain->path)) != 0 ||
        buf_append(&b, ",\"line\":") != 0 ||
        buf_append(&b, line) != 0 ||
        buf_append(&b, "},\"post\":") != 0 ||
        append_formula(&b, chain->arrivals[0].wp) != 0 ||
        buf_append(&b, ",\"pre\":") != 0 ||
        append_formula(&b, chain->arrivals[chain->n_arrivals - 1].wp) != 0 ||
        buf_append(&b, ",\"return_sort\":") != 0 ||
        append_sort(&b) != 0 ||
        buf_append_char(&b, '}') != 0) {
        goto done;
    }
    rc = pk_c_lift_result_add_declaration(result, b.data);

done:
    buf_free(&b);
    return rc;
}

#endif
