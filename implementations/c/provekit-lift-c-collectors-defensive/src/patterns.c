/* SPDX-License-Identifier: Apache-2.0 */

#include "contract.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} PkBuf;

typedef enum {
    PK_FORMULA_ATOMIC = 0,
    PK_FORMULA_AND = 1,
    PK_FORMULA_OR = 2,
    PK_FORMULA_NOT = 3
} PkFormulaKind;

typedef struct PkFormula {
    PkFormulaKind kind;
    char *name;
    char **args;
    size_t n_args;
    struct PkFormula **ops;
    size_t n_ops;
} PkFormula;

typedef struct {
    const char *p;
    const char *end;
} PkParser;

static int buf_init(PkBuf *b) {
    b->len = 0;
    b->cap = 128;
    b->data = malloc(b->cap);
    if (b->data == NULL) {
        b->cap = 0;
        return -1;
    }
    b->data[0] = '\0';
    return 0;
}

static void buf_free(PkBuf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static char *buf_take(PkBuf *b) {
    char *data = b->data;
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
    return data;
}

static int buf_grow(PkBuf *b, size_t need) {
    size_t next = b->cap ? b->cap : 128;
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

static int buf_append_n(PkBuf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int buf_append(PkBuf *b, const char *s) {
    return buf_append_n(b, s, strlen(s));
}

static int buf_append_char(PkBuf *b, char c) {
    return buf_append_n(b, &c, 1);
}

static int buf_json_escape(PkBuf *b, const char *src) {
    const unsigned char *p = (const unsigned char *)(src == NULL ? "" : src);

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
    return 0;
}

static int buf_append_quoted(PkBuf *b, const char *s) {
    if (buf_append_char(b, '"') != 0) return -1;
    if (buf_json_escape(b, s) != 0) return -1;
    return buf_append_char(b, '"');
}

static int buf_append_predicate_name(PkBuf *b, const char *name) {
    if (strcmp(name, "<=") == 0) {
        return buf_append(b, "\"\\u2264\"");
    }
    if (strcmp(name, ">=") == 0) {
        return buf_append(b, "\"\\u2265\"");
    }
    if (strcmp(name, "!=") == 0) {
        return buf_append(b, "\"\\u2260\"");
    }
    return buf_append_quoted(b, name);
}

static char *copy_n(const char *src, size_t len) {
    char *out = malloc(len + 1);

    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

static char *copy_string(const char *src) {
    return copy_n(src == NULL ? "" : src, strlen(src == NULL ? "" : src));
}

static char *trim_copy_n(const char *start, size_t len) {
    const char *end = start + len;

    while (start < end && isspace((unsigned char)*start)) {
        start++;
    }
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    return copy_n(start, (size_t)(end - start));
}

static int json_list_append_take(pk_c_walker_json_list *list, char *json) {
    char **items;
    size_t cap;

    if (json == NULL) {
        return 0;
    }
    if (list->len == list->cap) {
        cap = list->cap == 0 ? 4 : list->cap * 2;
        if (cap < list->cap) {
            free(json);
            return -1;
        }
        items = realloc(list->items, cap * sizeof(*items));
        if (items == NULL) {
            free(json);
            return -1;
        }
        list->items = items;
        list->cap = cap;
    }
    list->items[list->len++] = json;
    return 0;
}

static int string_list_append(pk_c_walker_string_list *list, const char *s) {
    char **items;
    size_t cap;
    char *copy;

    copy = copy_string(s);
    if (copy == NULL) {
        return -1;
    }
    if (list->len == list->cap) {
        cap = list->cap == 0 ? 4 : list->cap * 2;
        if (cap < list->cap) {
            free(copy);
            return -1;
        }
        items = realloc(list->items, cap * sizeof(*items));
        if (items == NULL) {
            free(copy);
            return -1;
        }
        list->items = items;
        list->cap = cap;
    }
    list->items[list->len++] = copy;
    return 0;
}

void pk_c_walker_contract_init(pk_c_walker_contract *contract) {
    memset(contract, 0, sizeof(*contract));
}

void pk_c_walker_contract_free(pk_c_walker_contract *contract) {
    for (size_t i = 0; i < contract->preconditions.len; i++) {
        free(contract->preconditions.items[i]);
    }
    free(contract->preconditions.items);
    for (size_t i = 0; i < contract->effects.len; i++) {
        free(contract->effects.items[i]);
    }
    free(contract->effects.items);
    for (size_t i = 0; i < contract->formals.len; i++) {
        free(contract->formals.items[i]);
    }
    free(contract->formals.items);
    free(contract->post);
    memset(contract, 0, sizeof(*contract));
}

int pk_c_walker_contract_add_pre_take(pk_c_walker_contract *contract, char *json) {
    return json_list_append_take(&contract->preconditions, json);
}

int pk_c_walker_contract_add_effect_take(pk_c_walker_contract *contract, char *json) {
    return json_list_append_take(&contract->effects, json);
}

int pk_c_walker_contract_add_formal(pk_c_walker_contract *contract, const char *name) {
    return string_list_append(&contract->formals, name);
}

int pk_c_walker_contract_set_post_take(pk_c_walker_contract *contract, char *json) {
    if (json == NULL) {
        return 0;
    }
    free(contract->post);
    contract->post = json;
    return 0;
}

char *pk_c_walker_term_var(const char *name) {
    PkBuf b;
    char *out;

    if (buf_init(&b) != 0) {
        return NULL;
    }
    if (buf_append(&b, "{\"kind\":\"var\",\"name\":") != 0 ||
        buf_append_quoted(&b, name) != 0 ||
        buf_append_char(&b, '}') != 0) {
        buf_free(&b);
        return NULL;
    }
    out = buf_take(&b);
    return out;
}

char *pk_c_walker_term_const_int(long value) {
    char num[64];
    PkBuf b;
    char *out;

    (void)snprintf(num, sizeof(num), "%ld", value);
    if (buf_init(&b) != 0) {
        return NULL;
    }
    if (buf_append(&b, "{\"kind\":\"const\",\"value\":") != 0 ||
        buf_append(&b, num) != 0 ||
        buf_append(&b, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}") != 0) {
        buf_free(&b);
        return NULL;
    }
    out = buf_take(&b);
    return out;
}

static char *term_ctor_consume(const char *name, char **args, size_t n_args) {
    PkBuf b;
    char *out = NULL;

    if (buf_init(&b) != 0) {
        goto cleanup;
    }
    if (buf_append(&b, "{\"kind\":\"ctor\",\"name\":") != 0 ||
        buf_append_quoted(&b, name) != 0 ||
        buf_append(&b, ",\"args\":[") != 0) {
        goto cleanup;
    }
    for (size_t i = 0; i < n_args; i++) {
        if ((i > 0 && buf_append_char(&b, ',') != 0) ||
            buf_append(&b, args[i]) != 0) {
            goto cleanup;
        }
    }
    if (buf_append(&b, "]}") != 0) {
        goto cleanup;
    }
    out = buf_take(&b);

cleanup:
    buf_free(&b);
    for (size_t i = 0; i < n_args; i++) {
        free(args[i]);
    }
    free(args);
    return out;
}

static char *atomic_json_take(const char *name, char **args, size_t n_args) {
    PkBuf b;
    char *out = NULL;

    if (buf_init(&b) != 0) {
        goto cleanup;
    }
    if (buf_append(&b, "{\"args\":[") != 0) {
        goto cleanup;
    }
    for (size_t i = 0; i < n_args; i++) {
        if ((i > 0 && buf_append_char(&b, ',') != 0) ||
            buf_append(&b, args[i]) != 0) {
            goto cleanup;
        }
    }
    if (buf_append(&b, "],\"kind\":\"atomic\",\"name\":") != 0 ||
        buf_append_predicate_name(&b, name) != 0 ||
        buf_append_char(&b, '}') != 0) {
        goto cleanup;
    }
    out = buf_take(&b);

cleanup:
    buf_free(&b);
    for (size_t i = 0; i < n_args; i++) {
        free(args[i]);
    }
    free(args);
    return out;
}

char *pk_c_walker_formula_atomic1_var(const char *name, const char *arg) {
    char **args = calloc(1, sizeof(*args));

    if (args == NULL) {
        return NULL;
    }
    args[0] = pk_c_walker_term_var(arg);
    if (args[0] == NULL) {
        free(args);
        return NULL;
    }
    return atomic_json_take(name, args, 1);
}

char *pk_c_walker_formula_atomic2_var_const(const char *name, const char *lhs, long rhs) {
    char **args = calloc(2, sizeof(*args));

    if (args == NULL) {
        return NULL;
    }
    args[0] = pk_c_walker_term_var(lhs);
    args[1] = pk_c_walker_term_const_int(rhs);
    if (args[0] == NULL || args[1] == NULL) {
        for (size_t i = 0; i < 2; i++) {
            free(args[i]);
        }
        free(args);
        return NULL;
    }
    return atomic_json_take(name, args, 2);
}

char *pk_c_walker_formula_atomic2_var_var(const char *name, const char *lhs, const char *rhs) {
    char **args = calloc(2, sizeof(*args));

    if (args == NULL) {
        return NULL;
    }
    args[0] = pk_c_walker_term_var(lhs);
    args[1] = pk_c_walker_term_var(rhs);
    if (args[0] == NULL || args[1] == NULL) {
        for (size_t i = 0; i < 2; i++) {
            free(args[i]);
        }
        free(args);
        return NULL;
    }
    return atomic_json_take(name, args, 2);
}

static int is_ident_char(int ch) {
    return isalnum((unsigned char)ch) || ch == '_';
}

static int token_boundary_before(const char *source, const char *p) {
    return p == source || !is_ident_char((unsigned char)p[-1]);
}

static int token_boundary_after(const char *p) {
    return !is_ident_char((unsigned char)*p);
}

static const char *skip_line_comment(const char *p, const char *end) {
    p += 2;
    while (p < end && *p != '\n') {
        p++;
    }
    return p < end ? p + 1 : end;
}

static const char *skip_block_comment(const char *p, const char *end) {
    p += 2;
    while (p + 1 < end) {
        if (p[0] == '*' && p[1] == '/') {
            return p + 2;
        }
        p++;
    }
    return end;
}

static const char *skip_quoted_literal(const char *p, const char *end) {
    char quote = *p++;

    while (p < end) {
        if (*p == '\\') {
            p += p + 1 < end ? 2 : 1;
            continue;
        }
        if (*p == quote) {
            return p + 1;
        }
        p++;
    }
    return end;
}

static const char *line_start_for(const char *source, const char *p) {
    while (p > source && p[-1] != '\n') {
        p--;
    }
    return p;
}

static int line_is_preprocessor_directive(const char *source, const char *p) {
    const char *line = line_start_for(source, p);

    while (line < p && (*line == ' ' || *line == '\t' || *line == '\r')) {
        line++;
    }
    return *line == '#';
}

static const char *skip_preprocessor_directive(const char *p, const char *end) {
    const char *line = p;

    while (p < end) {
        if (*p == '\n') {
            const char *q = p;

            while (q > line && (q[-1] == ' ' || q[-1] == '\t' || q[-1] == '\r')) {
                q--;
            }
            if (q > line && q[-1] == '\\') {
                p++;
                line = p;
                continue;
            }
            return p + 1;
        }
        p++;
    }
    return end;
}

static const char *skip_non_code_at(const char *source, const char *p, const char *end) {
    if (line_is_preprocessor_directive(source, p)) {
        return skip_preprocessor_directive(p, end);
    }
    if (p + 1 < end && p[0] == '/' && p[1] == '/') {
        return skip_line_comment(p, end);
    }
    if (p + 1 < end && p[0] == '/' && p[1] == '*') {
        return skip_block_comment(p, end);
    }
    if (*p == '"' || *p == '\'') {
        return skip_quoted_literal(p, end);
    }
    return p;
}

static void parser_skip_ws(PkParser *parser) {
    while (parser->p < parser->end && isspace((unsigned char)*parser->p)) {
        parser->p++;
    }
}

static int parser_match(PkParser *parser, const char *text) {
    size_t len = strlen(text);

    parser_skip_ws(parser);
    if ((size_t)(parser->end - parser->p) < len ||
        strncmp(parser->p, text, len) != 0) {
        return 0;
    }
    parser->p += len;
    return 1;
}

static char *parser_identifier(PkParser *parser) {
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
    return copy_n(start, (size_t)(parser->p - start));
}

static char *parse_term_expr(PkParser *parser);
static PkFormula *parse_formula_or(PkParser *parser);

static char *parse_term_primary(PkParser *parser) {
    char *id;

    parser_skip_ws(parser);
    if (parser->p >= parser->end) {
        return NULL;
    }
    if (*parser->p == '(') {
        char *term;

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
        return pk_c_walker_term_const_int(value);
    }
    id = parser_identifier(parser);
    if (id != NULL) {
        parser_skip_ws(parser);
        if (parser->p < parser->end && *parser->p == '(') {
            pk_c_walker_json_list args = {0};
            char *ctor_name;
            char *result;
            size_t name_len = strlen("call:") + strlen(id);

            parser->p++;
            parser_skip_ws(parser);
            while (parser->p < parser->end && *parser->p != ')') {
                char *arg = parse_term_expr(parser);

                if (arg == NULL || json_list_append_take(&args, arg) != 0) {
                    free(id);
                    for (size_t i = 0; i < args.len; i++) free(args.items[i]);
                    free(args.items);
                    return NULL;
                }
                parser_skip_ws(parser);
                if (parser->p < parser->end && *parser->p == ',') {
                    parser->p++;
                    continue;
                }
                break;
            }
            (void)parser_match(parser, ")");
            ctor_name = malloc(name_len + 1);
            if (ctor_name == NULL) {
                free(id);
                for (size_t i = 0; i < args.len; i++) free(args.items[i]);
                free(args.items);
                return NULL;
            }
            (void)snprintf(ctor_name, name_len + 1, "call:%s", id);
            result = term_ctor_consume(ctor_name, args.items, args.len);
            free(ctor_name);
            free(id);
            return result;
        }
        {
            char *term = pk_c_walker_term_var(id);
            free(id);
            return term;
        }
    }
    return NULL;
}

static char *parse_term_unary(PkParser *parser) {
    parser_skip_ws(parser);
    if (parser->p < parser->end && (*parser->p == '&' || *parser->p == '*')) {
        parser->p++;
        return parse_term_unary(parser);
    }
    if (parser->p < parser->end && *parser->p == '-') {
        char **args;
        char *inner;

        parser->p++;
        parser_skip_ws(parser);
        if (parser->p < parser->end && isdigit((unsigned char)*parser->p)) {
            char *endptr;
            long value = strtol(parser->p, &endptr, 0);

            if (endptr == parser->p || endptr > parser->end) {
                return NULL;
            }
            parser->p = endptr;
            return pk_c_walker_term_const_int(-value);
        }
        inner = parse_term_unary(parser);
        if (inner == NULL) {
            return NULL;
        }
        args = calloc(1, sizeof(*args));
        if (args == NULL) {
            free(inner);
            return NULL;
        }
        args[0] = inner;
        return term_ctor_consume("neg", args, 1);
    }
    return parse_term_primary(parser);
}

static char *parse_term_mul(PkParser *parser) {
    char *lhs = parse_term_unary(parser);

    while (lhs != NULL) {
        const char *op = NULL;
        char **args;
        char *rhs;

        parser_skip_ws(parser);
        if (parser->p >= parser->end) {
            break;
        }
        if (*parser->p == '*' || *parser->p == '/' || *parser->p == '%') {
            char op_buf[2] = {*parser->p, '\0'};
            op = op_buf;
            parser->p++;
            rhs = parse_term_unary(parser);
            if (rhs == NULL) {
                free(lhs);
                return NULL;
            }
            args = calloc(2, sizeof(*args));
            if (args == NULL) {
                free(lhs);
                free(rhs);
                return NULL;
            }
            args[0] = lhs;
            args[1] = rhs;
            lhs = term_ctor_consume(op, args, 2);
            continue;
        }
        break;
    }
    return lhs;
}

static char *parse_term_expr(PkParser *parser) {
    char *lhs = parse_term_mul(parser);

    while (lhs != NULL) {
        const char *op = NULL;
        char **args;
        char *rhs;

        parser_skip_ws(parser);
        if (parser->p >= parser->end) {
            break;
        }
        if (*parser->p == '+' || *parser->p == '-') {
            char op_buf[2] = {*parser->p, '\0'};
            op = op_buf;
            parser->p++;
            rhs = parse_term_mul(parser);
            if (rhs == NULL) {
                free(lhs);
                return NULL;
            }
            args = calloc(2, sizeof(*args));
            if (args == NULL) {
                free(lhs);
                free(rhs);
                return NULL;
            }
            args[0] = lhs;
            args[1] = rhs;
            lhs = term_ctor_consume(op, args, 2);
            continue;
        }
        break;
    }
    return lhs;
}

static PkFormula *formula_new(PkFormulaKind kind) {
    PkFormula *formula = calloc(1, sizeof(*formula));

    if (formula != NULL) {
        formula->kind = kind;
    }
    return formula;
}

static void formula_free(PkFormula *formula) {
    if (formula == NULL) {
        return;
    }
    free(formula->name);
    for (size_t i = 0; i < formula->n_args; i++) {
        free(formula->args[i]);
    }
    free(formula->args);
    for (size_t i = 0; i < formula->n_ops; i++) {
        formula_free(formula->ops[i]);
    }
    free(formula->ops);
    free(formula);
}

static PkFormula *formula_atomic_take(const char *name, char **args, size_t n_args) {
    PkFormula *formula = formula_new(PK_FORMULA_ATOMIC);

    if (formula == NULL) {
        for (size_t i = 0; i < n_args; i++) free(args[i]);
        free(args);
        return NULL;
    }
    formula->name = copy_string(name);
    if (formula->name == NULL) {
        formula_free(formula);
        for (size_t i = 0; i < n_args; i++) free(args[i]);
        free(args);
        return NULL;
    }
    formula->args = args;
    formula->n_args = n_args;
    return formula;
}

static PkFormula *formula_binary(PkFormulaKind kind, PkFormula *lhs, PkFormula *rhs) {
    PkFormula *formula = formula_new(kind);

    if (formula == NULL) {
        formula_free(lhs);
        formula_free(rhs);
        return NULL;
    }
    formula->ops = calloc(2, sizeof(*formula->ops));
    if (formula->ops == NULL) {
        formula_free(formula);
        formula_free(lhs);
        formula_free(rhs);
        return NULL;
    }
    formula->ops[0] = lhs;
    formula->ops[1] = rhs;
    formula->n_ops = 2;
    return formula;
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

static PkFormula *formula_negate(PkFormula *formula) {
    const char *flipped;
    PkFormula *wrapper;

    if (formula == NULL) {
        return NULL;
    }
    if (formula->kind == PK_FORMULA_ATOMIC) {
        flipped = flip_predicate(formula->name);
        if (flipped != NULL) {
            char *name = copy_string(flipped);

            if (name == NULL) {
                formula_free(formula);
                return NULL;
            }
            free(formula->name);
            formula->name = name;
            return formula;
        }
    }
    if (formula->kind == PK_FORMULA_AND || formula->kind == PK_FORMULA_OR) {
        formula->kind = formula->kind == PK_FORMULA_AND ? PK_FORMULA_OR : PK_FORMULA_AND;
        for (size_t i = 0; i < formula->n_ops; i++) {
            formula->ops[i] = formula_negate(formula->ops[i]);
            if (formula->ops[i] == NULL) {
                formula_free(formula);
                return NULL;
            }
        }
        return formula;
    }
    if (formula->kind == PK_FORMULA_NOT && formula->n_ops == 1) {
        PkFormula *inner = formula->ops[0];

        formula->ops[0] = NULL;
        formula_free(formula);
        return inner;
    }
    wrapper = formula_new(PK_FORMULA_NOT);
    if (wrapper == NULL) {
        formula_free(formula);
        return NULL;
    }
    wrapper->ops = calloc(1, sizeof(*wrapper->ops));
    if (wrapper->ops == NULL) {
        formula_free(wrapper);
        formula_free(formula);
        return NULL;
    }
    wrapper->ops[0] = formula;
    wrapper->n_ops = 1;
    return wrapper;
}

static PkFormula *parse_formula_primary(PkParser *parser) {
    char *lhs;
    char **args;
    const char *op = NULL;

    parser_skip_ws(parser);
    if (parser->p < parser->end && *parser->p == '(') {
        PkFormula *formula;

        parser->p++;
        formula = parse_formula_or(parser);
        (void)parser_match(parser, ")");
        return formula;
    }
    lhs = parse_term_expr(parser);
    if (lhs == NULL) {
        return NULL;
    }
    parser_skip_ws(parser);
    if (parser_match(parser, "==")) {
        op = "=";
    } else if (parser_match(parser, "!=")) {
        op = "!=";
    } else if (parser_match(parser, "<=")) {
        op = "<=";
    } else if (parser_match(parser, ">=")) {
        op = ">=";
    } else if (parser_match(parser, "<")) {
        op = "<";
    } else if (parser_match(parser, ">")) {
        op = ">";
    }
    if (op != NULL) {
        char *rhs = parse_term_expr(parser);

        if (rhs == NULL) {
            free(lhs);
            return NULL;
        }
        args = calloc(2, sizeof(*args));
        if (args == NULL) {
            free(lhs);
            free(rhs);
            return NULL;
        }
        args[0] = lhs;
        args[1] = rhs;
        return formula_atomic_take(op, args, 2);
    }
    args = calloc(1, sizeof(*args));
    if (args == NULL) {
        free(lhs);
        return NULL;
    }
    args[0] = lhs;
    return formula_atomic_take("truthy", args, 1);
}

static PkFormula *parse_formula_not(PkParser *parser) {
    parser_skip_ws(parser);
    if (parser->p < parser->end && *parser->p == '!') {
        PkFormula *inner;

        parser->p++;
        inner = parse_formula_not(parser);
        return formula_negate(inner);
    }
    return parse_formula_primary(parser);
}

static PkFormula *parse_formula_and(PkParser *parser) {
    PkFormula *lhs = parse_formula_not(parser);

    while (lhs != NULL && parser_match(parser, "&&")) {
        PkFormula *rhs = parse_formula_not(parser);

        if (rhs == NULL) {
            formula_free(lhs);
            return NULL;
        }
        lhs = formula_binary(PK_FORMULA_AND, lhs, rhs);
    }
    return lhs;
}

static PkFormula *parse_formula_or(PkParser *parser) {
    PkFormula *lhs = parse_formula_and(parser);

    while (lhs != NULL && parser_match(parser, "||")) {
        PkFormula *rhs = parse_formula_and(parser);

        if (rhs == NULL) {
            formula_free(lhs);
            return NULL;
        }
        lhs = formula_binary(PK_FORMULA_OR, lhs, rhs);
    }
    return lhs;
}

static int formula_term_serialize_into(PkBuf *b, const PkFormula *formula) {
    const char *name;

    if (formula == NULL) {
        return -1;
    }
    if (formula->kind == PK_FORMULA_ATOMIC) {
        name = formula->name;
    } else if (formula->kind == PK_FORMULA_AND) {
        name = "and";
    } else if (formula->kind == PK_FORMULA_OR) {
        name = "or";
    } else {
        name = "not";
    }
    if (buf_append(b, "{\"kind\":\"ctor\",\"name\":") != 0 ||
        buf_append_quoted(b, name) != 0 ||
        buf_append(b, ",\"args\":[") != 0) {
        return -1;
    }
    if (formula->kind == PK_FORMULA_ATOMIC) {
        for (size_t i = 0; i < formula->n_args; i++) {
            if ((i > 0 && buf_append_char(b, ',') != 0) ||
                buf_append(b, formula->args[i]) != 0) {
                return -1;
            }
        }
    } else {
        for (size_t i = 0; i < formula->n_ops; i++) {
            if ((i > 0 && buf_append_char(b, ',') != 0) ||
                formula_term_serialize_into(b, formula->ops[i]) != 0) {
                return -1;
            }
        }
    }
    return buf_append(b, "]}");
}

static char *formula_to_term_json_take(PkFormula *formula) {
    PkBuf b;
    char *out = NULL;

    if (formula == NULL || buf_init(&b) != 0) {
        formula_free(formula);
        return NULL;
    }
    if (formula_term_serialize_into(&b, formula) == 0) {
        out = buf_take(&b);
    }
    buf_free(&b);
    formula_free(formula);
    return out;
}

static char *parse_condition_term_json(const char *start, size_t len) {
    PkParser parser = {start, start + len};

    return formula_to_term_json_take(parse_formula_or(&parser));
}

static char *parse_term_json(const char *start, size_t len) {
    PkParser parser = {start, start + len};

    return parse_term_expr(&parser);
}

static char *term_ite_take(char *condition, char *then_term, char *else_term) {
    char **args = calloc(3, sizeof(*args));

    if (args == NULL) {
        free(condition);
        free(then_term);
        free(else_term);
        return NULL;
    }
    args[0] = condition;
    args[1] = then_term;
    args[2] = else_term;
    return term_ctor_consume("ite", args, 3);
}

static char *post_eq_result_take(char *term) {
    char **args = calloc(2, sizeof(*args));

    if (args == NULL) {
        free(term);
        return NULL;
    }
    args[0] = pk_c_walker_term_var("result");
    args[1] = term;
    if (args[0] == NULL || args[1] == NULL) {
        for (size_t i = 0; i < 2; i++) {
            free(args[i]);
        }
        free(args);
        return NULL;
    }
    return atomic_json_take("=", args, 2);
}

static char *true_formula_json(void) {
    return atomic_json_take("true", NULL, 0);
}

static const char *match_balanced(const char *open, const char *end, char lhs, char rhs) {
    int depth = 0;

    for (const char *p = open; p < end && *p != '\0'; p++) {
        const char *next = skip_non_code_at(open, p, end);

        if (next != p) {
            p = next - 1;
            continue;
        }
        if (*p == lhs) {
            depth++;
        } else if (*p == rhs) {
            depth--;
            if (depth == 0) {
                return p;
            }
        }
    }
    return NULL;
}

int pk_c_walker_find_function_source(
    const char *source,
    const char *fn_name,
    pk_c_walker_function_span *span
) {
    size_t name_len;
    const char *p;
    const char *end;

    if (source == NULL || fn_name == NULL || span == NULL) {
        return 0;
    }
    memset(span, 0, sizeof(*span));
    name_len = strlen(fn_name);
    p = source;
    end = source + strlen(source);
    while (p < end) {
        const char *after = p + name_len;
        const char *params_end;
        const char *body_open;
        const char *body_close;
        const char *line_start;
        const char *next = skip_non_code_at(source, p, end);

        if (next != p) {
            p = next;
            continue;
        }
        if (after > end || strncmp(p, fn_name, name_len) != 0) {
            p++;
            continue;
        }
        if (!token_boundary_before(source, p) || !token_boundary_after(after)) {
            p++;
            continue;
        }
        while (isspace((unsigned char)*after)) {
            after++;
        }
        if (*after != '(') {
            p = after;
            continue;
        }
        params_end = match_balanced(after, end, '(', ')');
        if (params_end == NULL) {
            return 0;
        }
        body_open = params_end + 1;
        while (body_open < end && *body_open != '{' && *body_open != ';') {
            const char *next = skip_non_code_at(source, body_open, end);

            if (next != body_open) {
                body_open = next;
                continue;
            }
            body_open++;
        }
        if (body_open >= end || *body_open != '{') {
            p = params_end + 1;
            continue;
        }
        body_close = match_balanced(body_open, end, '{', '}');
        if (body_close == NULL) {
            return 0;
        }
        line_start = p;
        while (line_start > source && line_start[-1] != '\n') {
            line_start--;
        }
        span->source_start = source;
        span->signature_start = line_start;
        span->params_start = after + 1;
        span->params_end = params_end;
        span->body_start = body_open + 1;
        span->body_end = body_close;
        return 1;
    }
    return 0;
}

static int extract_paren_payload(
    const char *open,
    const char *end,
    const char **payload_start,
    const char **payload_end
) {
    const char *close;

    if (open == NULL || open >= end || *open != '(') {
        return 0;
    }
    close = match_balanced(open, end, '(', ')');
    if (close == NULL) {
        return 0;
    }
    *payload_start = open + 1;
    *payload_end = close;
    return 1;
}

static const char *statement_end(const char *source, const char *start, const char *end) {
    int depth = 0;

    for (const char *p = start; p < end; p++) {
        const char *next = skip_non_code_at(source, p, end);

        if (next != p) {
            p = next - 1;
            continue;
        }
        if (*p == '(' || *p == '[' || *p == '{') {
            depth++;
        } else if (*p == ')' || *p == ']' || *p == '}') {
            if (depth > 0) {
                depth--;
            }
        } else if (*p == ';' && depth == 0) {
            return p + 1;
        }
    }
    return end;
}

static char *return_expr_copy(const char *stmt_start, const char *stmt_end) {
    const char *p = stmt_start;
    const char *expr_start;
    const char *expr_end;

    while (p < stmt_end && isspace((unsigned char)*p)) {
        p++;
    }
    if (p < stmt_end && *p == '{') {
        p++;
        while (p < stmt_end && isspace((unsigned char)*p)) {
            p++;
        }
    }
    if ((size_t)(stmt_end - p) < strlen("return") ||
        strncmp(p, "return", strlen("return")) != 0 ||
        !token_boundary_after(p + strlen("return"))) {
        return NULL;
    }
    p += strlen("return");
    expr_start = p;
    expr_end = stmt_end;
    while (expr_end > expr_start && isspace((unsigned char)expr_end[-1])) {
        expr_end--;
    }
    if (expr_end > expr_start && expr_end[-1] == '}') {
        expr_end--;
        while (expr_end > expr_start && isspace((unsigned char)expr_end[-1])) {
            expr_end--;
        }
    }
    if (expr_end > expr_start && expr_end[-1] == ';') {
        expr_end--;
    }
    return trim_copy_n(expr_start, (size_t)(expr_end - expr_start));
}

static int return_has_local_label(
    const pk_c_walker_function_span *span,
    const char *return_pos
) {
    const char *start = return_pos;

    while (start > span->body_start &&
        start[-1] != ';' && start[-1] != '{' && start[-1] != '}') {
        start--;
    }
    while (start < return_pos) {
        if (*start == ':') {
            return 1;
        }
        start++;
    }
    return 0;
}

typedef struct {
    char *term;
    int has_return;
    int unsupported_loop;
} ReturnTerm;

static void return_term_free(ReturnTerm *ret) {
    if (ret == NULL) {
        return;
    }
    free(ret->term);
    memset(ret, 0, sizeof(*ret));
}

static const char *skip_ws_and_non_code(
    const char *source,
    const char *p,
    const char *end
) {
    for (;;) {
        const char *next;

        while (p < end && isspace((unsigned char)*p)) {
            p++;
        }
        if (p >= end) {
            return p;
        }
        next = skip_non_code_at(source, p, end);
        if (next == p) {
            return p;
        }
        p = next;
    }
}

static int starts_keyword(
    const char *source,
    const char *p,
    const char *end,
    const char *keyword
) {
    size_t len = strlen(keyword);

    return p + len <= end &&
        strncmp(p, keyword, len) == 0 &&
        token_boundary_before(source, p) &&
        token_boundary_after(p + len);
}

static int starts_loop_keyword(const char *source, const char *p, const char *end) {
    return starts_keyword(source, p, end, "while") ||
        starts_keyword(source, p, end, "for") ||
        starts_keyword(source, p, end, "do");
}

static const char *if_statement_end(const char *source, const char *if_start, const char *end);
static const char *statement_or_block_end(const char *source, const char *start, const char *end);
static int scan_sequence_return(
    const char *source,
    const char *start,
    const char *end,
    ReturnTerm *ret);

static const char *statement_or_block_end(
    const char *source,
    const char *start,
    const char *end
) {
    const char *p = skip_ws_and_non_code(source, start, end);

    if (p >= end) {
        return end;
    }
    if (*p == '{') {
        const char *close = match_balanced(p, end, '{', '}');

        return close == NULL ? NULL : close + 1;
    }
    if (starts_keyword(source, p, end, "if")) {
        return if_statement_end(source, p, end);
    }
    return statement_end(source, p, end);
}

static const char *if_statement_end(const char *source, const char *if_start, const char *end) {
    const char *open = if_start + strlen("if");
    const char *cond_start;
    const char *cond_end;
    const char *then_start;
    const char *then_finish;
    const char *after_then;

    while (open < end && isspace((unsigned char)*open)) {
        open++;
    }
    if (!extract_paren_payload(open, end, &cond_start, &cond_end)) {
        return NULL;
    }
    then_start = skip_ws_and_non_code(source, cond_end + 1, end);
    then_finish = statement_or_block_end(source, then_start, end);
    if (then_finish == NULL) {
        return NULL;
    }
    after_then = skip_ws_and_non_code(source, then_finish, end);
    if (starts_keyword(source, after_then, end, "else")) {
        const char *else_start = skip_ws_and_non_code(
            source,
            after_then + strlen("else"),
            end);

        return statement_or_block_end(source, else_start, end);
    }
    return then_finish;
}

static char *return_statement_term(const char *stmt_start, const char *stmt_end) {
    char *expr = return_expr_copy(stmt_start, stmt_end);
    char *term;

    if (expr == NULL) {
        return NULL;
    }
    term = parse_term_json(expr, strlen(expr));
    free(expr);
    return term;
}

static int scan_single_statement_return(
    const char *source,
    const char *stmt_start,
    const char *stmt_end,
    ReturnTerm *ret
);

static int scan_if_return(
    const char *source,
    const char *if_start,
    const char *end,
    const char **after_if,
    ReturnTerm *ret
) {
    const char *open = if_start + strlen("if");
    const char *cond_start;
    const char *cond_end;
    const char *then_start;
    const char *then_finish;
    const char *after_then;
    const char *else_start = NULL;
    const char *else_finish = NULL;
    ReturnTerm then_ret = {0};
    ReturnTerm else_ret = {0};
    ReturnTerm rest_ret = {0};
    char *cond_term = NULL;
    int has_else = 0;
    int rc = -1;

    while (open < end && isspace((unsigned char)*open)) {
        open++;
    }
    if (!extract_paren_payload(open, end, &cond_start, &cond_end)) {
        return 0;
    }
    then_start = skip_ws_and_non_code(source, cond_end + 1, end);
    then_finish = statement_or_block_end(source, then_start, end);
    if (then_finish == NULL) {
        return -1;
    }
    after_then = skip_ws_and_non_code(source, then_finish, end);
    if (starts_keyword(source, after_then, end, "else")) {
        has_else = 1;
        else_start = skip_ws_and_non_code(source, after_then + strlen("else"), end);
        else_finish = statement_or_block_end(source, else_start, end);
        if (else_finish == NULL) {
            return -1;
        }
        *after_if = else_finish;
    } else {
        *after_if = then_finish;
    }

    if (scan_single_statement_return(source, then_start, then_finish, &then_ret) != 0 ||
        (has_else &&
            scan_single_statement_return(source, else_start, else_finish, &else_ret) != 0)) {
        goto done;
    }
    ret->unsupported_loop = then_ret.unsupported_loop || else_ret.unsupported_loop;
    if (ret->unsupported_loop) {
        rc = 0;
        goto done;
    }

    if (then_ret.has_return && else_ret.has_return) {
        cond_term = parse_condition_term_json(cond_start, (size_t)(cond_end - cond_start));
        if (cond_term == NULL) {
            rc = 0;
            goto done;
        }
        ret->term = term_ite_take(cond_term, then_ret.term, else_ret.term);
        cond_term = NULL;
        then_ret.term = NULL;
        else_ret.term = NULL;
        ret->has_return = ret->term != NULL;
        rc = ret->term == NULL ? -1 : 0;
        goto done;
    }

    if (then_ret.has_return || else_ret.has_return) {
        const char *rest_start = *after_if;

        if (scan_sequence_return(source, rest_start, end, &rest_ret) != 0) {
            goto done;
        }
        ret->unsupported_loop = rest_ret.unsupported_loop;
        if (ret->unsupported_loop || !rest_ret.has_return) {
            rc = 0;
            goto done;
        }
        cond_term = parse_condition_term_json(cond_start, (size_t)(cond_end - cond_start));
        if (cond_term == NULL) {
            rc = 0;
            goto done;
        }
        if (then_ret.has_return) {
            ret->term = term_ite_take(cond_term, then_ret.term, rest_ret.term);
            then_ret.term = NULL;
        } else {
            ret->term = term_ite_take(cond_term, rest_ret.term, else_ret.term);
            else_ret.term = NULL;
        }
        cond_term = NULL;
        rest_ret.term = NULL;
        ret->has_return = ret->term != NULL;
        rc = ret->term == NULL ? -1 : 0;
        goto done;
    }

    rc = 0;

done:
    free(cond_term);
    return_term_free(&then_ret);
    return_term_free(&else_ret);
    return_term_free(&rest_ret);
    return rc;
}

static int scan_single_statement_return(
    const char *source,
    const char *stmt_start,
    const char *stmt_end,
    ReturnTerm *ret
) {
    const char *p = skip_ws_and_non_code(source, stmt_start, stmt_end);

    if (p >= stmt_end) {
        return 0;
    }
    if (*p == '{') {
        const char *close = match_balanced(p, stmt_end, '{', '}');

        if (close == NULL) {
            return -1;
        }
        return scan_sequence_return(source, p + 1, close, ret);
    }
    if (starts_loop_keyword(source, p, stmt_end)) {
        ret->unsupported_loop = 1;
        return 0;
    }
    if (starts_keyword(source, p, stmt_end, "return")) {
        ret->term = return_statement_term(p, stmt_end);
        ret->has_return = ret->term != NULL;
        return 0;
    }
    if (starts_keyword(source, p, stmt_end, "if")) {
        const char *after_if = p;

        return scan_if_return(source, p, stmt_end, &after_if, ret);
    }
    return 0;
}

static int scan_sequence_return(
    const char *source,
    const char *start,
    const char *end,
    ReturnTerm *ret
) {
    const char *p = start;

    while (p < end) {
        const char *stmt_finish;

        p = skip_ws_and_non_code(source, p, end);
        if (p >= end) {
            break;
        }
        if (starts_loop_keyword(source, p, end)) {
            ret->unsupported_loop = 1;
            return 0;
        }
        if (starts_keyword(source, p, end, "if")) {
            const char *after_if = p;
            ReturnTerm if_ret = {0};

            if (scan_if_return(source, p, end, &after_if, &if_ret) != 0) {
                return -1;
            }
            if (if_ret.unsupported_loop) {
                ret->unsupported_loop = 1;
                return_term_free(&if_ret);
                return 0;
            }
            if (if_ret.has_return) {
                *ret = if_ret;
                return 0;
            }
            p = after_if;
            continue;
        }
        stmt_finish = statement_or_block_end(source, p, end);
        if (stmt_finish == NULL) {
            return -1;
        }
        if (starts_keyword(source, p, stmt_finish, "return")) {
            if (return_has_local_label(&(pk_c_walker_function_span){.body_start = start}, p)) {
                return 0;
            }
            ret->term = return_statement_term(p, stmt_finish);
            ret->has_return = ret->term != NULL;
            return 0;
        }
        p = stmt_finish;
    }
    return 0;
}

static int add_branch_sensitive_return_post(
    const pk_c_walker_function_span *span,
    pk_c_walker_contract *contract
) {
    ReturnTerm ret = {0};
    int rc;

    if (scan_sequence_return(span->source_start, span->body_start, span->body_end, &ret) != 0) {
        return -1;
    }
    if (ret.unsupported_loop) {
        rc = pk_c_walker_contract_set_post_take(contract, true_formula_json());
        return_term_free(&ret);
        return rc;
    }
    if (!ret.has_return) {
        return 0;
    }
    rc = pk_c_walker_contract_set_post_take(contract, post_eq_result_take(ret.term));
    ret.term = NULL;
    return_term_free(&ret);
    return rc;
}

int pk_c_walker_extract_defensive_patterns(
    const char *source,
    const char *fn_name,
    pk_c_walker_contract *contract
) {
    pk_c_walker_function_span span;

    if (!pk_c_walker_find_function_source(source, fn_name, &span)) {
        return 0;
    }
    return add_branch_sensitive_return_post(&span, contract);
}

int pk_c_walker_function_has_loop(const char *source, const char *fn_name) {
    pk_c_walker_function_span span;
    const char *p;

    if (!pk_c_walker_find_function_source(source, fn_name, &span)) {
        return 0;
    }
    p = span.body_start;
    while (p < span.body_end) {
        const char *next = skip_non_code_at(span.source_start, p, span.body_end);

        if (next != p) {
            p = next;
            continue;
        }
        if (starts_loop_keyword(span.source_start, p, span.body_end)) {
            return 1;
        }
        p++;
    }
    return 0;
}
