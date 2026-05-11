/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} WpBuf;

typedef struct {
    const char *p;
    const char *end;
} WpParser;

char *pk_c_walk_copy_n(const char *src, size_t len) {
    char *out = malloc(len + 1);

    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

char *pk_c_walk_copy(const char *src) {
    return pk_c_walk_copy_n(src == NULL ? "" : src, strlen(src == NULL ? "" : src));
}

static int buf_init(WpBuf *b) {
    b->len = 0;
    b->cap = 256;
    b->data = malloc(b->cap);
    if (b->data == NULL) {
        b->cap = 0;
        return -1;
    }
    b->data[0] = '\0';
    return 0;
}

static void buf_free(WpBuf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static char *buf_take(WpBuf *b) {
    char *data = b->data;

    b->data = NULL;
    b->len = 0;
    b->cap = 0;
    return data;
}

static int buf_grow(WpBuf *b, size_t need) {
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

static int buf_append_n(WpBuf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int buf_append(WpBuf *b, const char *s) {
    return buf_append_n(b, s, strlen(s));
}

static int buf_append_char(WpBuf *b, char c) {
    return buf_append_n(b, &c, 1);
}

static int buf_append_quoted(WpBuf *b, const char *src) {
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

static int is_ident_char(int ch) {
    return isalnum((unsigned char)ch) || ch == '_';
}

static pk_c_walk_term *term_new(pk_c_walk_term_kind kind) {
    pk_c_walk_term *term = calloc(1, sizeof(*term));

    if (term != NULL) {
        term->kind = kind;
    }
    return term;
}

pk_c_walk_term *pk_c_walk_term_var(const char *name) {
    pk_c_walk_term *term = term_new(PK_C_WALK_TERM_VAR);

    if (term == NULL) {
        return NULL;
    }
    term->name = pk_c_walk_copy(name);
    if (term->name == NULL) {
        pk_c_walk_term_free(term);
        return NULL;
    }
    return term;
}

pk_c_walk_term *pk_c_walk_term_const_int(long value) {
    pk_c_walk_term *term = term_new(PK_C_WALK_TERM_CONST_INT);

    if (term != NULL) {
        term->value = value;
    }
    return term;
}

pk_c_walk_term *pk_c_walk_term_ctor_take(const char *name, pk_c_walk_term **args, size_t n_args) {
    pk_c_walk_term *term = term_new(PK_C_WALK_TERM_CTOR);

    if (term == NULL) {
        for (size_t i = 0; i < n_args; i++) pk_c_walk_term_free(args[i]);
        free(args);
        return NULL;
    }
    term->name = pk_c_walk_copy(name);
    if (term->name == NULL) {
        pk_c_walk_term_free(term);
        for (size_t i = 0; i < n_args; i++) pk_c_walk_term_free(args[i]);
        free(args);
        return NULL;
    }
    term->args = args;
    term->n_args = n_args;
    return term;
}

pk_c_walk_term *pk_c_walk_term_clone(const pk_c_walk_term *term) {
    pk_c_walk_term **args;

    if (term == NULL) {
        return NULL;
    }
    if (term->kind == PK_C_WALK_TERM_VAR) {
        return pk_c_walk_term_var(term->name);
    }
    if (term->kind == PK_C_WALK_TERM_CONST_INT) {
        return pk_c_walk_term_const_int(term->value);
    }
    args = calloc(term->n_args, sizeof(*args));
    if (args == NULL && term->n_args > 0) {
        return NULL;
    }
    for (size_t i = 0; i < term->n_args; i++) {
        args[i] = pk_c_walk_term_clone(term->args[i]);
        if (args[i] == NULL) {
            for (size_t j = 0; j < i; j++) pk_c_walk_term_free(args[j]);
            free(args);
            return NULL;
        }
    }
    return pk_c_walk_term_ctor_take(term->name, args, term->n_args);
}

void pk_c_walk_term_free(pk_c_walk_term *term) {
    if (term == NULL) {
        return;
    }
    free(term->name);
    for (size_t i = 0; i < term->n_args; i++) {
        pk_c_walk_term_free(term->args[i]);
    }
    free(term->args);
    free(term);
}

static pk_c_walk_formula *formula_new(pk_c_walk_formula_kind kind) {
    pk_c_walk_formula *formula = calloc(1, sizeof(*formula));

    if (formula != NULL) {
        formula->kind = kind;
    }
    return formula;
}

pk_c_walk_formula *pk_c_walk_formula_true(void) {
    return pk_c_walk_formula_atomic_take("true", NULL, 0);
}

pk_c_walk_formula *pk_c_walk_formula_atomic_take(const char *name, pk_c_walk_term **args, size_t n_args) {
    pk_c_walk_formula *formula = formula_new(PK_C_WALK_FORMULA_ATOMIC);

    if (formula == NULL) {
        for (size_t i = 0; i < n_args; i++) pk_c_walk_term_free(args[i]);
        free(args);
        return NULL;
    }
    formula->name = pk_c_walk_copy(name);
    if (formula->name == NULL) {
        for (size_t i = 0; i < n_args; i++) pk_c_walk_term_free(args[i]);
        free(args);
        free(formula);
        return NULL;
    }
    formula->args = args;
    formula->n_args = n_args;
    return formula;
}

static pk_c_walk_formula *formula_binary_take(
    pk_c_walk_formula_kind kind,
    pk_c_walk_formula *lhs,
    pk_c_walk_formula *rhs
) {
    pk_c_walk_formula *formula;

    if (lhs == NULL) return rhs;
    if (rhs == NULL) return lhs;
    formula = formula_new(kind);
    if (formula == NULL) {
        pk_c_walk_formula_free(lhs);
        pk_c_walk_formula_free(rhs);
        return NULL;
    }
    formula->ops = calloc(2, sizeof(*formula->ops));
    if (formula->ops == NULL) {
        pk_c_walk_formula_free(formula);
        pk_c_walk_formula_free(lhs);
        pk_c_walk_formula_free(rhs);
        return NULL;
    }
    formula->ops[0] = lhs;
    formula->ops[1] = rhs;
    formula->n_ops = 2;
    return formula;
}

pk_c_walk_formula *pk_c_walk_formula_and_take(pk_c_walk_formula *lhs, pk_c_walk_formula *rhs) {
    if (pk_c_walk_formula_is_true(lhs)) {
        pk_c_walk_formula_free(lhs);
        return rhs;
    }
    if (pk_c_walk_formula_is_true(rhs)) {
        pk_c_walk_formula_free(rhs);
        return lhs;
    }
    return formula_binary_take(PK_C_WALK_FORMULA_AND, lhs, rhs);
}

pk_c_walk_formula *pk_c_walk_formula_or_take(pk_c_walk_formula *lhs, pk_c_walk_formula *rhs) {
    return formula_binary_take(PK_C_WALK_FORMULA_OR, lhs, rhs);
}

pk_c_walk_formula *pk_c_walk_formula_implies_take(pk_c_walk_formula *lhs, pk_c_walk_formula *rhs) {
    return formula_binary_take(PK_C_WALK_FORMULA_IMPLIES, lhs, rhs);
}

pk_c_walk_formula *pk_c_walk_formula_not_take(pk_c_walk_formula *inner) {
    pk_c_walk_formula *formula = formula_new(PK_C_WALK_FORMULA_NOT);

    if (formula == NULL) {
        pk_c_walk_formula_free(inner);
        return NULL;
    }
    formula->ops = calloc(1, sizeof(*formula->ops));
    if (formula->ops == NULL) {
        pk_c_walk_formula_free(formula);
        pk_c_walk_formula_free(inner);
        return NULL;
    }
    formula->ops[0] = inner;
    formula->n_ops = 1;
    return formula;
}

void pk_c_walk_formula_free(pk_c_walk_formula *formula) {
    if (formula == NULL) {
        return;
    }
    free(formula->name);
    for (size_t i = 0; i < formula->n_args; i++) {
        pk_c_walk_term_free(formula->args[i]);
    }
    free(formula->args);
    for (size_t i = 0; i < formula->n_ops; i++) {
        pk_c_walk_formula_free(formula->ops[i]);
    }
    free(formula->ops);
    free(formula);
}

pk_c_walk_formula *pk_c_walk_formula_clone(const pk_c_walk_formula *formula) {
    pk_c_walk_formula *out;

    if (formula == NULL) {
        return NULL;
    }
    out = formula_new(formula->kind);
    if (out == NULL) {
        return NULL;
    }
    out->name = formula->name == NULL ? NULL : pk_c_walk_copy(formula->name);
    if (formula->name != NULL && out->name == NULL) {
        pk_c_walk_formula_free(out);
        return NULL;
    }
    out->n_args = formula->n_args;
    out->args = calloc(formula->n_args, sizeof(*out->args));
    if (out->args == NULL && formula->n_args > 0) {
        pk_c_walk_formula_free(out);
        return NULL;
    }
    for (size_t i = 0; i < formula->n_args; i++) {
        out->args[i] = pk_c_walk_term_clone(formula->args[i]);
        if (out->args[i] == NULL) {
            pk_c_walk_formula_free(out);
            return NULL;
        }
    }
    out->n_ops = formula->n_ops;
    out->ops = calloc(formula->n_ops, sizeof(*out->ops));
    if (out->ops == NULL && formula->n_ops > 0) {
        pk_c_walk_formula_free(out);
        return NULL;
    }
    for (size_t i = 0; i < formula->n_ops; i++) {
        out->ops[i] = pk_c_walk_formula_clone(formula->ops[i]);
        if (out->ops[i] == NULL) {
            pk_c_walk_formula_free(out);
            return NULL;
        }
    }
    return out;
}

static const char *flip_predicate(const char *name) {
    if (strcmp(name, "<") == 0) return ">=";
    if (strcmp(name, "<=") == 0) return ">";
    if (strcmp(name, ">") == 0) return "<=";
    if (strcmp(name, ">=") == 0) return "<";
    if (strcmp(name, "=") == 0) return "!=";
    if (strcmp(name, "!=") == 0) return "=";
    return NULL;
}

pk_c_walk_formula *pk_c_walk_formula_negate_take(pk_c_walk_formula *formula) {
    const char *flipped;

    if (formula == NULL) {
        return NULL;
    }
    if (formula->kind == PK_C_WALK_FORMULA_ATOMIC) {
        flipped = flip_predicate(formula->name);
        if (flipped != NULL) {
            char *name = pk_c_walk_copy(flipped);

            if (name == NULL) {
                pk_c_walk_formula_free(formula);
                return NULL;
            }
            free(formula->name);
            formula->name = name;
            return formula;
        }
    }
    if (formula->kind == PK_C_WALK_FORMULA_AND || formula->kind == PK_C_WALK_FORMULA_OR) {
        formula->kind = formula->kind == PK_C_WALK_FORMULA_AND ?
            PK_C_WALK_FORMULA_OR : PK_C_WALK_FORMULA_AND;
        for (size_t i = 0; i < formula->n_ops; i++) {
            formula->ops[i] = pk_c_walk_formula_negate_take(formula->ops[i]);
            if (formula->ops[i] == NULL) {
                pk_c_walk_formula_free(formula);
                return NULL;
            }
        }
        return formula;
    }
    if (formula->kind == PK_C_WALK_FORMULA_NOT && formula->n_ops == 1) {
        pk_c_walk_formula *inner = formula->ops[0];

        formula->ops[0] = NULL;
        pk_c_walk_formula_free(formula);
        return inner;
    }
    return pk_c_walk_formula_not_take(formula);
}

static pk_c_walk_term *substitute_term(
    const pk_c_walk_term *term,
    const char *var_name,
    const pk_c_walk_term *replacement
) {
    pk_c_walk_term **args;

    if (term == NULL) {
        return NULL;
    }
    if (term->kind == PK_C_WALK_TERM_VAR && strcmp(term->name, var_name) == 0) {
        return pk_c_walk_term_clone(replacement);
    }
    if (term->kind != PK_C_WALK_TERM_CTOR) {
        return pk_c_walk_term_clone(term);
    }
    args = calloc(term->n_args, sizeof(*args));
    if (args == NULL && term->n_args > 0) {
        return NULL;
    }
    for (size_t i = 0; i < term->n_args; i++) {
        args[i] = substitute_term(term->args[i], var_name, replacement);
        if (args[i] == NULL) {
            for (size_t j = 0; j < i; j++) pk_c_walk_term_free(args[j]);
            free(args);
            return NULL;
        }
    }
    return pk_c_walk_term_ctor_take(term->name, args, term->n_args);
}

pk_c_walk_formula *pk_c_walk_substitute_formula(
    const pk_c_walk_formula *formula,
    const char *var_name,
    const pk_c_walk_term *replacement
) {
    pk_c_walk_formula *out;

    if (formula == NULL) {
        return NULL;
    }
    out = formula_new(formula->kind);
    if (out == NULL) {
        return NULL;
    }
    out->name = formula->name == NULL ? NULL : pk_c_walk_copy(formula->name);
    if (formula->name != NULL && out->name == NULL) {
        pk_c_walk_formula_free(out);
        return NULL;
    }
    out->n_args = formula->n_args;
    out->args = calloc(formula->n_args, sizeof(*out->args));
    if (out->args == NULL && formula->n_args > 0) {
        pk_c_walk_formula_free(out);
        return NULL;
    }
    for (size_t i = 0; i < formula->n_args; i++) {
        out->args[i] = substitute_term(formula->args[i], var_name, replacement);
        if (out->args[i] == NULL) {
            pk_c_walk_formula_free(out);
            return NULL;
        }
    }
    out->n_ops = formula->n_ops;
    out->ops = calloc(formula->n_ops, sizeof(*out->ops));
    if (out->ops == NULL && formula->n_ops > 0) {
        pk_c_walk_formula_free(out);
        return NULL;
    }
    for (size_t i = 0; i < formula->n_ops; i++) {
        out->ops[i] = pk_c_walk_substitute_formula(formula->ops[i], var_name, replacement);
        if (out->ops[i] == NULL) {
            pk_c_walk_formula_free(out);
            return NULL;
        }
    }
    return out;
}

int pk_c_walk_formula_is_true(const pk_c_walk_formula *formula) {
    return formula != NULL && formula->kind == PK_C_WALK_FORMULA_ATOMIC &&
        formula->n_args == 0 && formula->name != NULL && strcmp(formula->name, "true") == 0;
}

static int append_predicate_name(WpBuf *b, const char *name) {
    if (strcmp(name, ">=") == 0) return buf_append(b, "\"\\u2265\"");
    if (strcmp(name, "<=") == 0) return buf_append(b, "\"\\u2264\"");
    if (strcmp(name, "!=") == 0) return buf_append(b, "\"\\u2260\"");
    return buf_append_quoted(b, name);
}

static int term_json_into(WpBuf *b, const pk_c_walk_term *term) {
    char num[64];

    if (term == NULL) {
        return -1;
    }
    if (term->kind == PK_C_WALK_TERM_VAR) {
        return buf_append(b, "{\"kind\":\"var\",\"name\":") == 0 &&
            buf_append_quoted(b, term->name) == 0 &&
            buf_append_char(b, '}') == 0 ? 0 : -1;
    }
    if (term->kind == PK_C_WALK_TERM_CONST_INT) {
        (void)snprintf(num, sizeof(num), "%ld", term->value);
        return buf_append(b, "{\"kind\":\"const\",\"value\":") == 0 &&
            buf_append(b, num) == 0 &&
            buf_append(b, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}") == 0 ? 0 : -1;
    }
    if (buf_append(b, "{\"kind\":\"ctor\",\"name\":") != 0 ||
        buf_append_quoted(b, term->name) != 0 ||
        buf_append(b, ",\"args\":[") != 0) {
        return -1;
    }
    for (size_t i = 0; i < term->n_args; i++) {
        if ((i > 0 && buf_append_char(b, ',') != 0) ||
            term_json_into(b, term->args[i]) != 0) {
            return -1;
        }
    }
    return buf_append(b, "]}");
}

char *pk_c_walk_term_json(const pk_c_walk_term *term) {
    WpBuf b;
    char *out = NULL;

    if (buf_init(&b) != 0) {
        return NULL;
    }
    if (term_json_into(&b, term) == 0) {
        out = buf_take(&b);
    }
    buf_free(&b);
    return out;
}

static int formula_json_into(WpBuf *b, const pk_c_walk_formula *formula) {
    const char *kind;

    if (formula == NULL) {
        return -1;
    }
    if (formula->kind == PK_C_WALK_FORMULA_ATOMIC) {
        if (buf_append(b, "{\"args\":[") != 0) return -1;
        for (size_t i = 0; i < formula->n_args; i++) {
            if ((i > 0 && buf_append_char(b, ',') != 0) ||
                term_json_into(b, formula->args[i]) != 0) {
                return -1;
            }
        }
        return buf_append(b, "],\"kind\":\"atomic\",\"name\":") == 0 &&
            append_predicate_name(b, formula->name) == 0 &&
            buf_append_char(b, '}') == 0 ? 0 : -1;
    }
    kind = formula->kind == PK_C_WALK_FORMULA_AND ? "and" :
        formula->kind == PK_C_WALK_FORMULA_OR ? "or" :
        formula->kind == PK_C_WALK_FORMULA_IMPLIES ? "implies" : "not";
    if (buf_append(b, "{\"kind\":") != 0 ||
        buf_append_quoted(b, kind) != 0 ||
        buf_append(b, ",\"operands\":[") != 0) {
        return -1;
    }
    for (size_t i = 0; i < formula->n_ops; i++) {
        if ((i > 0 && buf_append_char(b, ',') != 0) ||
            formula_json_into(b, formula->ops[i]) != 0) {
            return -1;
        }
    }
    return buf_append(b, "]}");
}

char *pk_c_walk_formula_json(const pk_c_walk_formula *formula) {
    WpBuf b;
    char *out = NULL;

    if (buf_init(&b) != 0) {
        return NULL;
    }
    if (formula_json_into(&b, formula) == 0) {
        out = buf_take(&b);
    }
    buf_free(&b);
    return out;
}

static void parser_skip_ws(WpParser *parser) {
    while (parser->p < parser->end && isspace((unsigned char)*parser->p)) {
        parser->p++;
    }
}

static int parser_match(WpParser *parser, const char *text) {
    size_t len = strlen(text);

    parser_skip_ws(parser);
    if ((size_t)(parser->end - parser->p) < len || strncmp(parser->p, text, len) != 0) {
        return 0;
    }
    parser->p += len;
    return 1;
}

static char *parser_identifier(WpParser *parser) {
    const char *start;

    parser_skip_ws(parser);
    if (parser->p >= parser->end ||
        (!isalpha((unsigned char)*parser->p) && *parser->p != '_')) {
        return NULL;
    }
    start = parser->p++;
    while (parser->p < parser->end && is_ident_char((unsigned char)*parser->p)) {
        parser->p++;
    }
    return pk_c_walk_copy_n(start, (size_t)(parser->p - start));
}

static pk_c_walk_term *parse_term_expr(WpParser *parser);
static pk_c_walk_formula *parse_formula_or(WpParser *parser);

static pk_c_walk_term *parse_term_primary(WpParser *parser) {
    char *id;

    parser_skip_ws(parser);
    if (parser->p >= parser->end) {
        return NULL;
    }
    if (*parser->p == '(') {
        pk_c_walk_term *term;

        parser->p++;
        term = parse_term_expr(parser);
        (void)parser_match(parser, ")");
        return term;
    }
    if (isdigit((unsigned char)*parser->p)) {
        char *endptr;
        long value = strtol(parser->p, &endptr, 0);

        if (endptr == parser->p || endptr > parser->end) {
            return NULL;
        }
        parser->p = endptr;
        return pk_c_walk_term_const_int(value);
    }
    id = parser_identifier(parser);
    if (id != NULL) {
        parser_skip_ws(parser);
        if (parser->p < parser->end && *parser->p == '(') {
            pk_c_walk_term **args = NULL;
            size_t len = 0;
            size_t cap = 0;
            char *ctor_name;
            pk_c_walk_term *result;
            size_t name_len = strlen("call:") + strlen(id);

            parser->p++;
            parser_skip_ws(parser);
            while (parser->p < parser->end && *parser->p != ')') {
                pk_c_walk_term *arg = parse_term_expr(parser);

                if (arg == NULL) goto call_fail;
                if (len == cap) {
                    size_t next = cap == 0 ? 4 : cap * 2;
                    pk_c_walk_term **items = realloc(args, next * sizeof(*args));

                    if (items == NULL) {
                        pk_c_walk_term_free(arg);
                        goto call_fail;
                    }
                    args = items;
                    cap = next;
                }
                args[len++] = arg;
                parser_skip_ws(parser);
                if (parser->p < parser->end && *parser->p == ',') {
                    parser->p++;
                    continue;
                }
                break;
            }
            (void)parser_match(parser, ")");
            ctor_name = malloc(name_len + 1);
            if (ctor_name == NULL) goto call_fail;
            (void)snprintf(ctor_name, name_len + 1, "call:%s", id);
            result = pk_c_walk_term_ctor_take(ctor_name, args, len);
            free(ctor_name);
            free(id);
            return result;

call_fail:
            for (size_t i = 0; i < len; i++) pk_c_walk_term_free(args[i]);
            free(args);
            free(id);
            return NULL;
        }
        {
            pk_c_walk_term *term = pk_c_walk_term_var(id);

            free(id);
            return term;
        }
    }
    return NULL;
}

static pk_c_walk_term *parse_term_unary(WpParser *parser) {
    parser_skip_ws(parser);
    if (parser->p < parser->end && (*parser->p == '&' || *parser->p == '*')) {
        parser->p++;
        return parse_term_unary(parser);
    }
    if (parser->p < parser->end && *parser->p == '-') {
        pk_c_walk_term *inner;
        pk_c_walk_term **args;

        parser->p++;
        parser_skip_ws(parser);
        if (parser->p < parser->end && isdigit((unsigned char)*parser->p)) {
            char *endptr;
            long value = strtol(parser->p, &endptr, 0);

            if (endptr == parser->p || endptr > parser->end) {
                return NULL;
            }
            parser->p = endptr;
            return pk_c_walk_term_const_int(-value);
        }
        inner = parse_term_unary(parser);
        if (inner == NULL) {
            return NULL;
        }
        args = calloc(1, sizeof(*args));
        if (args == NULL) {
            pk_c_walk_term_free(inner);
            return NULL;
        }
        args[0] = inner;
        return pk_c_walk_term_ctor_take("neg", args, 1);
    }
    return parse_term_primary(parser);
}

static pk_c_walk_term *parse_term_mul(WpParser *parser) {
    pk_c_walk_term *lhs = parse_term_unary(parser);

    while (lhs != NULL) {
        char op_buf[2];
        pk_c_walk_term **args;
        pk_c_walk_term *rhs;

        parser_skip_ws(parser);
        if (parser->p >= parser->end || (*parser->p != '*' && *parser->p != '/' && *parser->p != '%')) {
            break;
        }
        op_buf[0] = *parser->p++;
        op_buf[1] = '\0';
        rhs = parse_term_unary(parser);
        if (rhs == NULL) {
            pk_c_walk_term_free(lhs);
            return NULL;
        }
        args = calloc(2, sizeof(*args));
        if (args == NULL) {
            pk_c_walk_term_free(lhs);
            pk_c_walk_term_free(rhs);
            return NULL;
        }
        args[0] = lhs;
        args[1] = rhs;
        lhs = pk_c_walk_term_ctor_take(op_buf, args, 2);
    }
    return lhs;
}

static pk_c_walk_term *parse_term_expr(WpParser *parser) {
    pk_c_walk_term *lhs = parse_term_mul(parser);

    while (lhs != NULL) {
        char op_buf[2];
        pk_c_walk_term **args;
        pk_c_walk_term *rhs;

        parser_skip_ws(parser);
        if (parser->p >= parser->end || (*parser->p != '+' && *parser->p != '-')) {
            break;
        }
        op_buf[0] = *parser->p++;
        op_buf[1] = '\0';
        rhs = parse_term_mul(parser);
        if (rhs == NULL) {
            pk_c_walk_term_free(lhs);
            return NULL;
        }
        args = calloc(2, sizeof(*args));
        if (args == NULL) {
            pk_c_walk_term_free(lhs);
            pk_c_walk_term_free(rhs);
            return NULL;
        }
        args[0] = lhs;
        args[1] = rhs;
        lhs = pk_c_walk_term_ctor_take(op_buf, args, 2);
    }
    return lhs;
}

static pk_c_walk_formula *parse_formula_primary(WpParser *parser) {
    pk_c_walk_term *lhs;
    pk_c_walk_term **args;
    const char *op = NULL;

    parser_skip_ws(parser);
    if (parser->p < parser->end && *parser->p == '(') {
        pk_c_walk_formula *formula;

        parser->p++;
        formula = parse_formula_or(parser);
        (void)parser_match(parser, ")");
        return formula;
    }
    lhs = parse_term_expr(parser);
    if (lhs == NULL) {
        return NULL;
    }
    if (parser_match(parser, "==")) op = "=";
    else if (parser_match(parser, "!=")) op = "!=";
    else if (parser_match(parser, "<=")) op = "<=";
    else if (parser_match(parser, ">=")) op = ">=";
    else if (parser_match(parser, "<")) op = "<";
    else if (parser_match(parser, ">")) op = ">";
    if (op != NULL) {
        pk_c_walk_term *rhs = parse_term_expr(parser);

        if (rhs == NULL) {
            pk_c_walk_term_free(lhs);
            return NULL;
        }
        args = calloc(2, sizeof(*args));
        if (args == NULL) {
            pk_c_walk_term_free(lhs);
            pk_c_walk_term_free(rhs);
            return NULL;
        }
        args[0] = lhs;
        args[1] = rhs;
        return pk_c_walk_formula_atomic_take(op, args, 2);
    }
    args = calloc(1, sizeof(*args));
    if (args == NULL) {
        pk_c_walk_term_free(lhs);
        return NULL;
    }
    args[0] = lhs;
    return pk_c_walk_formula_atomic_take("truthy", args, 1);
}

static pk_c_walk_formula *parse_formula_not(WpParser *parser) {
    parser_skip_ws(parser);
    if (parser->p < parser->end && *parser->p == '!') {
        parser->p++;
        return pk_c_walk_formula_negate_take(parse_formula_not(parser));
    }
    return parse_formula_primary(parser);
}

static pk_c_walk_formula *parse_formula_and(WpParser *parser) {
    pk_c_walk_formula *lhs = parse_formula_not(parser);

    while (lhs != NULL && parser_match(parser, "&&")) {
        pk_c_walk_formula *rhs = parse_formula_not(parser);

        if (rhs == NULL) {
            pk_c_walk_formula_free(lhs);
            return NULL;
        }
        lhs = pk_c_walk_formula_and_take(lhs, rhs);
    }
    return lhs;
}

static pk_c_walk_formula *parse_formula_or(WpParser *parser) {
    pk_c_walk_formula *lhs = parse_formula_and(parser);

    while (lhs != NULL && parser_match(parser, "||")) {
        pk_c_walk_formula *rhs = parse_formula_and(parser);

        if (rhs == NULL) {
            pk_c_walk_formula_free(lhs);
            return NULL;
        }
        lhs = pk_c_walk_formula_or_take(lhs, rhs);
    }
    return lhs;
}

pk_c_walk_term *pk_c_walk_term_from_text(const char *start, size_t len) {
    WpParser parser = {start, start + len};

    return parse_term_expr(&parser);
}

pk_c_walk_term *pk_c_walk_term_from_text_full(const char *start, size_t len) {
    WpParser parser = {start, start + len};
    pk_c_walk_term *term = parse_term_expr(&parser);

    if (term == NULL) {
        return NULL;
    }
    parser_skip_ws(&parser);
    if (parser.p != parser.end) {
        pk_c_walk_term_free(term);
        return NULL;
    }
    return term;
}

pk_c_walk_formula *pk_c_walk_formula_from_text(const char *start, size_t len) {
    WpParser parser = {start, start + len};

    return parse_formula_or(&parser);
}

void pk_c_walk_chain_init(pk_c_walk_chain *chain) {
    memset(chain, 0, sizeof(*chain));
}

void pk_c_walk_chain_free(pk_c_walk_chain *chain) {
    if (chain == NULL) {
        return;
    }
    free(chain->caller_name);
    free(chain->callee_name);
    for (size_t i = 0; i < chain->n_formals; i++) free(chain->formals[i]);
    free(chain->formals);
    free(chain->path);
    for (size_t i = 0; i < chain->n_arrivals; i++) {
        free(chain->arrivals[i].kind);
        free(chain->arrivals[i].name);
        free(chain->arrivals[i].branch);
        free(chain->arrivals[i].actuals_json);
        pk_c_walk_formula_free(chain->arrivals[i].cond);
        pk_c_walk_formula_free(chain->arrivals[i].wp);
    }
    free(chain->arrivals);
    memset(chain, 0, sizeof(*chain));
}

static pk_c_walk_arrival *chain_append_slot(pk_c_walk_chain *chain) {
    pk_c_walk_arrival *items;
    size_t cap;

    if (chain->n_arrivals == chain->cap_arrivals) {
        cap = chain->cap_arrivals == 0 ? 4 : chain->cap_arrivals * 2;
        if (cap < chain->cap_arrivals) {
            return NULL;
        }
        items = realloc(chain->arrivals, cap * sizeof(*items));
        if (items == NULL) {
            return NULL;
        }
        chain->arrivals = items;
        chain->cap_arrivals = cap;
    }
    memset(&chain->arrivals[chain->n_arrivals], 0, sizeof(chain->arrivals[chain->n_arrivals]));
    return &chain->arrivals[chain->n_arrivals];
}

static void arrival_clear(pk_c_walk_arrival *arrival) {
    free(arrival->kind);
    free(arrival->name);
    free(arrival->branch);
    free(arrival->actuals_json);
    pk_c_walk_formula_free(arrival->cond);
    pk_c_walk_formula_free(arrival->wp);
    memset(arrival, 0, sizeof(*arrival));
}

static int chain_add_arrival_with_actuals(
    pk_c_walk_chain *chain,
    const char *kind,
    const char *name,
    size_t stmt_index,
    int line,
    int column,
    const char *actuals_json,
    const pk_c_walk_formula *wp
) {
    pk_c_walk_arrival *arrival = chain_append_slot(chain);

    if (arrival == NULL) {
        return -1;
    }
    arrival->kind = pk_c_walk_copy(kind);
    arrival->name = pk_c_walk_copy(name == NULL ? "" : name);
    arrival->stmt_index = stmt_index;
    arrival->line = line;
    arrival->column = column;
    if (actuals_json != NULL) {
        arrival->actuals_json = pk_c_walk_copy(actuals_json);
    }
    arrival->wp = pk_c_walk_formula_clone(wp);
    if (arrival->kind == NULL || arrival->name == NULL ||
        (actuals_json != NULL && arrival->actuals_json == NULL) ||
        arrival->wp == NULL) {
        arrival_clear(arrival);
        return -1;
    }
    chain->n_arrivals++;
    return 0;
}

int pk_c_walk_chain_add_arrival(
    pk_c_walk_chain *chain,
    const char *kind,
    const char *name,
    size_t stmt_index,
    int line,
    int column,
    const pk_c_walk_formula *wp
) {
    if (kind != NULL && strcmp(kind, "Callsite") == 0) {
        return chain_add_arrival_with_actuals(chain, kind, name, stmt_index, line, column, "[]", wp);
    }
    return chain_add_arrival_with_actuals(chain, kind, name, stmt_index, line, column, NULL, wp);
}

int pk_c_walk_chain_add_callsite_arrival(
    pk_c_walk_chain *chain,
    const char *name,
    size_t stmt_index,
    int line,
    int column,
    const char *actuals_json,
    const pk_c_walk_formula *wp
) {
    return chain_add_arrival_with_actuals(
        chain,
        "Callsite",
        name,
        stmt_index,
        line,
        column,
        actuals_json == NULL ? "[]" : actuals_json,
        wp);
}

int pk_c_walk_chain_add_conditional_arm_arrival(
    pk_c_walk_chain *chain,
    const char *branch,
    size_t stmt_index,
    size_t join_stmt_index,
    int line,
    int column,
    const pk_c_walk_formula *cond,
    const pk_c_walk_formula *wp
) {
    pk_c_walk_arrival *arrival = chain_append_slot(chain);

    if (arrival == NULL) {
        return -1;
    }
    arrival->kind = pk_c_walk_copy("ConditionalArm");
    arrival->name = pk_c_walk_copy(branch == NULL ? "" : branch);
    arrival->branch = pk_c_walk_copy(branch == NULL ? "" : branch);
    arrival->stmt_index = stmt_index;
    arrival->join_stmt_index = join_stmt_index;
    arrival->line = line;
    arrival->column = column;
    arrival->cond = pk_c_walk_formula_clone(cond);
    arrival->wp = pk_c_walk_formula_clone(wp);
    if (arrival->kind == NULL || arrival->name == NULL || arrival->branch == NULL ||
        arrival->cond == NULL || arrival->wp == NULL) {
        arrival_clear(arrival);
        return -1;
    }
    chain->n_arrivals++;
    return 0;
}
