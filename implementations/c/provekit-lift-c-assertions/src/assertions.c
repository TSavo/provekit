#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef PK_C_ENABLE_CLANG_AST
#include <clang-c/Index.h>
#endif

#include "provekit/c_lift_core.h"

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} WBuf;

typedef enum {
    FORMULA_ATOMIC,
    FORMULA_AND,
    FORMULA_OR,
    FORMULA_NOT
} FormulaKind;

typedef struct Formula Formula;

struct Formula {
    FormulaKind kind;
    char *name;
    char **args;
    size_t n_args;
    Formula **operands;
    size_t n_operands;
};

typedef struct {
    const char *s;
    size_t n;
} Slice;

typedef struct {
    char *name;
} LabelName;

typedef struct {
    pk_c_lift_result *result;
    char *name;
    char **formals;
    int n_formals;
    const char *path;
    int line;
    int column;
    Formula **preconditions;
    size_t n_preconditions;
    size_t cap_preconditions;
    int failed;
} FunctionContracts;

static int wbuf_init(WBuf *b) {
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
    if (data == NULL) {
        return -1;
    }
    b->data = data;
    b->cap = next;
    return 0;
}

static int wbuf_append_n(WBuf *b, const char *s, size_t n) {
    if (wbuf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int wbuf_append(WBuf *b, const char *s) {
    return wbuf_append_n(b, s, strlen(s));
}

static int wbuf_append_char(WBuf *b, char c) {
    return wbuf_append_n(b, &c, 1);
}

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
            } else if (wbuf_append_char(b, (char)*p) != 0) {
                return -1;
            }
            break;
        }
    }
    return 0;
}

static int wbuf_append_quoted(WBuf *b, const char *s) {
    if (wbuf_append_char(b, '"') != 0) return -1;
    if (wbuf_json_escape(b, s == NULL ? "" : s) != 0) return -1;
    return wbuf_append_char(b, '"');
}

static char *copy_n(const char *src, size_t len) {
    char *copy = malloc(len + 1);

    if (copy == NULL) {
        return NULL;
    }
    memcpy(copy, src, len);
    copy[len] = '\0';
    return copy;
}

static char *copy_string(const char *src) {
    if (src == NULL) {
        return NULL;
    }
    return copy_n(src, strlen(src));
}

static int is_ident_char(int c) {
    return isalnum((unsigned char)c) || c == '_';
}

static Slice slice_trim(Slice s) {
    while (s.n > 0 && isspace((unsigned char)s.s[0])) {
        s.s++;
        s.n--;
    }
    while (s.n > 0 && isspace((unsigned char)s.s[s.n - 1])) {
        s.n--;
    }
    return s;
}

static int matching_paren_in_slice(Slice s, size_t open, size_t *close_out) {
    int depth = 0;

    for (size_t i = open; i < s.n; i++) {
        if (s.s[i] == '(') {
            depth++;
        } else if (s.s[i] == ')') {
            depth--;
            if (depth == 0) {
                *close_out = i;
                return 1;
            }
            if (depth < 0) {
                return 0;
            }
        }
    }
    return 0;
}

static Slice slice_strip_parens(Slice s) {
    s = slice_trim(s);
    while (s.n >= 2 && s.s[0] == '(' && s.s[s.n - 1] == ')') {
        size_t close = 0;

        if (!matching_paren_in_slice(s, 0, &close) || close != s.n - 1) {
            break;
        }
        s.s++;
        s.n -= 2;
        s = slice_trim(s);
    }
    return s;
}

static char *slice_copy_trimmed(Slice s) {
    s = slice_strip_parens(s);
    return copy_n(s.s, s.n);
}

static int slice_is_integer(Slice s) {
    size_t i = 0;

    s = slice_strip_parens(s);
    if (s.n == 0) {
        return 0;
    }
    if (s.s[0] == '-') {
        i = 1;
    }
    if (i == s.n) {
        return 0;
    }
    for (; i < s.n; i++) {
        if (!isdigit((unsigned char)s.s[i])) {
            return 0;
        }
    }
    return 1;
}

static int slice_is_identifier(Slice s) {
    s = slice_strip_parens(s);
    if (s.n == 0 || (!isalpha((unsigned char)s.s[0]) && s.s[0] != '_')) {
        return 0;
    }
    for (size_t i = 1; i < s.n; i++) {
        if (!is_ident_char((unsigned char)s.s[i])) {
            return 0;
        }
    }
    return 1;
}

static char *term_json_from_slice(Slice s) {
    WBuf b;
    char *text;
    char *out;

    s = slice_strip_parens(s);
    if (wbuf_init(&b) != 0) {
        return NULL;
    }
    if (slice_is_integer(s)) {
        text = copy_n(s.s, s.n);
        if (text == NULL ||
            wbuf_append(&b, "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":") != 0 ||
            wbuf_append(&b, text) != 0 ||
            wbuf_append_char(&b, '}') != 0) {
            free(text);
            wbuf_free(&b);
            return NULL;
        }
        free(text);
    } else {
        text = slice_copy_trimmed(s);
        if (text == NULL ||
            wbuf_append(&b, "{\"kind\":\"var\",\"name\":") != 0 ||
            wbuf_append_quoted(&b, text) != 0 ||
            wbuf_append_char(&b, '}') != 0) {
            free(text);
            wbuf_free(&b);
            return NULL;
        }
        free(text);
    }
    out = b.data;
    b.data = NULL;
    wbuf_free(&b);
    return out;
}

static Formula *formula_alloc(FormulaKind kind) {
    Formula *f = calloc(1, sizeof(*f));

    if (f != NULL) {
        f->kind = kind;
    }
    return f;
}

static void formula_free(Formula *f) {
    if (f == NULL) {
        return;
    }
    free(f->name);
    for (size_t i = 0; i < f->n_args; i++) {
        free(f->args[i]);
    }
    free(f->args);
    for (size_t i = 0; i < f->n_operands; i++) {
        formula_free(f->operands[i]);
    }
    free(f->operands);
    free(f);
}

static Formula *formula_atomic2(const char *name, char *arg0, char *arg1) {
    Formula *f = formula_alloc(FORMULA_ATOMIC);

    if (f == NULL) {
        free(arg0);
        free(arg1);
        return NULL;
    }
    f->name = copy_string(name);
    f->args = calloc(2, sizeof(*f->args));
    if (f->name == NULL || f->args == NULL) {
        formula_free(f);
        free(arg0);
        free(arg1);
        return NULL;
    }
    f->args[0] = arg0;
    f->args[1] = arg1;
    f->n_args = 2;
    return f;
}

static Formula *formula_binary(FormulaKind kind, Formula *lhs, Formula *rhs) {
    Formula *f = formula_alloc(kind);

    if (f == NULL) {
        formula_free(lhs);
        formula_free(rhs);
        return NULL;
    }
    f->operands = calloc(2, sizeof(*f->operands));
    if (f->operands == NULL) {
        formula_free(f);
        formula_free(lhs);
        formula_free(rhs);
        return NULL;
    }
    f->operands[0] = lhs;
    f->operands[1] = rhs;
    f->n_operands = 2;
    return f;
}

static int formula_set_name(Formula *f, const char *name) {
    char *copy = copy_string(name);

    if (copy == NULL) {
        return -1;
    }
    free(f->name);
    f->name = copy;
    return 0;
}

static Formula *formula_wrap_not(Formula *inner) {
    Formula *f = formula_alloc(FORMULA_NOT);

    if (f == NULL) {
        formula_free(inner);
        return NULL;
    }
    f->operands = calloc(1, sizeof(*f->operands));
    if (f->operands == NULL) {
        formula_free(f);
        formula_free(inner);
        return NULL;
    }
    f->operands[0] = inner;
    f->n_operands = 1;
    return f;
}

static Formula *formula_negate(Formula *f) {
    if (f == NULL) {
        return NULL;
    }
    if (f->kind == FORMULA_ATOMIC && f->name != NULL) {
        const char *flipped = NULL;

        if (strcmp(f->name, "<") == 0) flipped = "≥";
        else if (strcmp(f->name, "≤") == 0) flipped = ">";
        else if (strcmp(f->name, ">") == 0) flipped = "≤";
        else if (strcmp(f->name, "≥") == 0) flipped = "<";
        else if (strcmp(f->name, "=") == 0) flipped = "≠";
        else if (strcmp(f->name, "≠") == 0) flipped = "=";
        if (flipped != NULL) {
            if (formula_set_name(f, flipped) != 0) {
                formula_free(f);
                return NULL;
            }
            return f;
        }
    }
    if (f->kind == FORMULA_AND || f->kind == FORMULA_OR) {
        f->kind = f->kind == FORMULA_AND ? FORMULA_OR : FORMULA_AND;
        for (size_t i = 0; i < f->n_operands; i++) {
            f->operands[i] = formula_negate(f->operands[i]);
            if (f->operands[i] == NULL) {
                formula_free(f);
                return NULL;
            }
        }
        return f;
    }
    if (f->kind == FORMULA_NOT && f->n_operands == 1) {
        Formula *inner = f->operands[0];

        f->operands[0] = NULL;
        free(f->operands);
        f->operands = NULL;
        f->n_operands = 0;
        formula_free(f);
        return inner;
    }
    return formula_wrap_not(f);
}

static int formula_write_json(WBuf *b, const Formula *f) {
    if (f == NULL) {
        return -1;
    }
    if (f->kind == FORMULA_ATOMIC) {
        if (wbuf_append(b, "{\"args\":[") != 0) return -1;
        for (size_t i = 0; i < f->n_args; i++) {
            if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
            if (wbuf_append(b, f->args[i]) != 0) return -1;
        }
        if (wbuf_append(b, "],\"kind\":\"atomic\",\"name\":") != 0) return -1;
        if (wbuf_append_quoted(b, f->name) != 0) return -1;
        return wbuf_append_char(b, '}');
    }
    if (f->kind == FORMULA_AND || f->kind == FORMULA_OR || f->kind == FORMULA_NOT) {
        const char *kind = f->kind == FORMULA_AND ? "and" : f->kind == FORMULA_OR ? "or" : "not";

        if (wbuf_append(b, "{\"kind\":") != 0) return -1;
        if (wbuf_append_quoted(b, kind) != 0) return -1;
        if (wbuf_append(b, ",\"operands\":[") != 0) return -1;
        for (size_t i = 0; i < f->n_operands; i++) {
            if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
            if (formula_write_json(b, f->operands[i]) != 0) return -1;
        }
        return wbuf_append(b, "]}");
    }
    return -1;
}

static Formula *lift_predicate_slice(Slice s);

static int find_top_level_operator(Slice s, const char *const *ops, size_t n_ops, size_t *pos, const char **op_out) {
    int depth = 0;

    s = slice_strip_parens(s);
    for (size_t i = 0; i < s.n; i++) {
        if (s.s[i] == '(') {
            depth++;
            continue;
        }
        if (s.s[i] == ')') {
            depth--;
            continue;
        }
        if (depth != 0) {
            continue;
        }
        for (size_t j = 0; j < n_ops; j++) {
            size_t op_len = strlen(ops[j]);

            if (i + op_len <= s.n && strncmp(s.s + i, ops[j], op_len) == 0) {
                *pos = i;
                *op_out = ops[j];
                return 1;
            }
        }
    }
    return 0;
}

static const char *predicate_name_for_op(const char *op) {
    if (strcmp(op, "==") == 0) return "=";
    if (strcmp(op, "!=") == 0) return "≠";
    if (strcmp(op, "<=") == 0) return "≤";
    if (strcmp(op, ">=") == 0) return "≥";
    if (strcmp(op, "<") == 0) return "<";
    if (strcmp(op, ">") == 0) return ">";
    return NULL;
}

static Formula *lift_comparison(Slice s, size_t pos, const char *op) {
    Slice lhs = {s.s, pos};
    Slice rhs = {s.s + pos + strlen(op), s.n - pos - strlen(op)};
    char *lhs_term = term_json_from_slice(lhs);
    char *rhs_term = term_json_from_slice(rhs);
    const char *name = predicate_name_for_op(op);

    if (lhs_term == NULL || rhs_term == NULL || name == NULL) {
        free(lhs_term);
        free(rhs_term);
        return NULL;
    }
    return formula_atomic2(name, lhs_term, rhs_term);
}

static Formula *lift_truthy(Slice s) {
    char *term = term_json_from_slice(s);
    char *zero = term_json_from_slice((Slice){"0", 1});

    if (term == NULL || zero == NULL) {
        free(term);
        free(zero);
        return NULL;
    }
    return formula_atomic2("≠", term, zero);
}

static Formula *lift_predicate_slice(Slice s) {
    static const char *const or_ops[] = {"||"};
    static const char *const and_ops[] = {"&&"};
    static const char *const cmp_ops[] = {"==", "!=", "<=", ">=", "<", ">"};
    size_t pos = 0;
    const char *op = NULL;

    s = slice_strip_parens(s);
    if (s.n == 0) {
        return NULL;
    }
    if (find_top_level_operator(s, or_ops, 1, &pos, &op)) {
        Formula *lhs = lift_predicate_slice((Slice){s.s, pos});
        Formula *rhs = lift_predicate_slice((Slice){s.s + pos + 2, s.n - pos - 2});

        return lhs == NULL || rhs == NULL ? NULL : formula_binary(FORMULA_OR, lhs, rhs);
    }
    if (find_top_level_operator(s, and_ops, 1, &pos, &op)) {
        Formula *lhs = lift_predicate_slice((Slice){s.s, pos});
        Formula *rhs = lift_predicate_slice((Slice){s.s + pos + 2, s.n - pos - 2});

        return lhs == NULL || rhs == NULL ? NULL : formula_binary(FORMULA_AND, lhs, rhs);
    }
    if (s.s[0] == '!') {
        Formula *inner = lift_predicate_slice((Slice){s.s + 1, s.n - 1});

        return formula_negate(inner);
    }
    if (find_top_level_operator(s, cmp_ops, sizeof(cmp_ops) / sizeof(cmp_ops[0]), &pos, &op)) {
        return lift_comparison(s, pos, op);
    }
    return lift_truthy(s);
}

static Formula *nonnull_formula_for_name(const char *name) {
    char *lhs = term_json_from_slice((Slice){name, strlen(name)});
    char *rhs = term_json_from_slice((Slice){"NULL", 4});

    if (lhs == NULL || rhs == NULL) {
        free(lhs);
        free(rhs);
        return NULL;
    }
    return formula_atomic2("≠", lhs, rhs);
}

static char *name_from_not_condition(Slice condition) {
    condition = slice_strip_parens(condition);
    if (condition.n < 2 || condition.s[0] != '!') {
        return NULL;
    }
    condition.s++;
    condition.n--;
    condition = slice_strip_parens(condition);
    if (!slice_is_identifier(condition)) {
        return NULL;
    }
    return copy_n(condition.s, condition.n);
}

static int function_add_precondition(FunctionContracts *fn, Formula *formula) {
    Formula **next;
    size_t next_cap;

    if (formula == NULL) {
        return 0;
    }
    if (fn->n_preconditions == fn->cap_preconditions) {
        next_cap = fn->cap_preconditions == 0 ? 4 : fn->cap_preconditions * 2;
        if (next_cap < fn->cap_preconditions) {
            formula_free(formula);
            return -1;
        }
        next = realloc(fn->preconditions, next_cap * sizeof(*fn->preconditions));
        if (next == NULL) {
            formula_free(formula);
            return -1;
        }
        fn->preconditions = next;
        fn->cap_preconditions = next_cap;
    }
    fn->preconditions[fn->n_preconditions++] = formula;
    return 0;
}

static int emit_warn_opacity(FunctionContracts *fn) {
    return pk_c_lift_result_add_opacity_entry(
        fn->result,
        "c-assertions.warn-on",
        fn->path == NULL ? "" : fn->path,
        fn->line,
        fn->column,
        "WARN_ON is non-fatal, so its predicate is not emitted as a hard contract",
        "c-assertions");
}

static int build_function_contract(WBuf *b, const FunctionContracts *fn) {
    if (wbuf_append(b, "{") != 0) return -1;
    if (wbuf_append(b, "\"auto_minted_mementos\":[],") != 0) return -1;
    if (wbuf_append(b, "\"body_cid\":null,") != 0) return -1;
    if (wbuf_append(b, "\"fn_name\":") != 0) return -1;
    if (wbuf_append_quoted(b, fn->name) != 0) return -1;
    if (wbuf_append_char(b, ',') != 0) return -1;
    if (wbuf_append(b, "\"formal_sorts\":[") != 0) return -1;
    for (int i = 0; i < fn->n_formals; i++) {
        if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
        if (wbuf_append(b, "{\"kind\":\"primitive\",\"name\":\"i32\"}") != 0) return -1;
    }
    if (wbuf_append(b, "],\"formals\":[") != 0) return -1;
    for (int i = 0; i < fn->n_formals; i++) {
        if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
        if (wbuf_append_quoted(b, fn->formals[i]) != 0) return -1;
    }
    if (wbuf_append(b, "],\"kind\":\"function-contract\",") != 0) return -1;
    if (wbuf_append(b, "\"locus\":{\"col\":") != 0) return -1;
    {
        char nbuf[32];
        (void)snprintf(nbuf, sizeof(nbuf), "%d", fn->column);
        if (wbuf_append(b, nbuf) != 0) return -1;
        if (wbuf_append(b, ",\"file\":") != 0) return -1;
        if (wbuf_append_quoted(b, fn->path == NULL ? "" : fn->path) != 0) return -1;
        (void)snprintf(nbuf, sizeof(nbuf), "%d", fn->line);
        if (wbuf_append(b, ",\"line\":") != 0) return -1;
        if (wbuf_append(b, nbuf) != 0) return -1;
        if (wbuf_append(b, "},") != 0) return -1;
    }
    if (wbuf_append(b, "\"post\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"true\"},") != 0) return -1;
    if (wbuf_append(b, "\"pre\":") != 0) return -1;
    if (fn->n_preconditions == 1) {
        if (formula_write_json(b, fn->preconditions[0]) != 0) return -1;
    } else {
        if (wbuf_append(b, "{\"kind\":\"and\",\"operands\":[") != 0) return -1;
        for (size_t i = 0; i < fn->n_preconditions; i++) {
            if (i > 0 && wbuf_append_char(b, ',') != 0) return -1;
            if (formula_write_json(b, fn->preconditions[i]) != 0) return -1;
        }
        if (wbuf_append(b, "]}") != 0) return -1;
    }
    if (wbuf_append(b, ",\"effects\":{\"effects\":[]},") != 0) return -1;
    return wbuf_append(b, "\"return_sort\":{\"kind\":\"primitive\",\"name\":\"i32\"}}");
}

static int emit_function_contract(FunctionContracts *fn) {
    WBuf b;
    int rc;

    if (fn->n_preconditions == 0) {
        return 0;
    }
    if (wbuf_init(&b) != 0) {
        return -1;
    }
    rc = build_function_contract(&b, fn);
    if (rc == 0) {
        rc = pk_c_lift_result_add_declaration(fn->result, b.data);
    }
    wbuf_free(&b);
    return rc;
}

static int append_core_result(pk_c_lift_result *result, const pk_c_source_facts *facts) {
    if (facts == NULL || facts->extraction_result == NULL) {
        return 0;
    }
    return pk_c_lift_result_extend(result, facts->extraction_result);
}

static char *sanitize_literals(const char *source) {
    char *copy = copy_string(source == NULL ? "" : source);
    char quote = '\0';
    int escaped = 0;

    if (copy == NULL) {
        return NULL;
    }
    for (size_t i = 0; copy[i] != '\0'; i++) {
        if (quote != '\0') {
            if (escaped) {
                copy[i] = '_';
                escaped = 0;
            } else if (copy[i] == '\\') {
                copy[i] = '_';
                escaped = 1;
            } else if (copy[i] == quote) {
                quote = '\0';
            } else {
                copy[i] = '_';
            }
        } else if (copy[i] == '"' || copy[i] == '\'') {
            quote = copy[i];
        }
    }
    return copy;
}

static int match_identifier_call(const char *safe, size_t pos, const char *name, size_t *open_out) {
    size_t n = strlen(name);
    size_t p;

    if (pos > 0 && is_ident_char((unsigned char)safe[pos - 1])) {
        return 0;
    }
    if (strncmp(safe + pos, name, n) != 0) {
        return 0;
    }
    if (is_ident_char((unsigned char)safe[pos + n])) {
        return 0;
    }
    p = pos + n;
    while (isspace((unsigned char)safe[p])) {
        p++;
    }
    if (safe[p] != '(') {
        return 0;
    }
    *open_out = p;
    return 1;
}

static int match_parenthesized_assert_call(const char *safe, size_t pos, size_t *open_out) {
    size_t p = pos;

    if (safe[p] != '(') {
        return 0;
    }
    p++;
    while (isspace((unsigned char)safe[p])) p++;
    if (strncmp(safe + p, "assert", strlen("assert")) != 0 ||
        is_ident_char((unsigned char)safe[p + strlen("assert")])) {
        return 0;
    }
    p += strlen("assert");
    while (isspace((unsigned char)safe[p])) p++;
    if (safe[p] != ')') {
        return 0;
    }
    p++;
    while (isspace((unsigned char)safe[p])) p++;
    if (safe[p] != '(') {
        return 0;
    }
    *open_out = p;
    return 1;
}

static int find_matching_paren_text(const char *s, size_t open, size_t *close_out) {
    int depth = 0;

    for (size_t i = open; s[i] != '\0'; i++) {
        if (s[i] == '(') {
            depth++;
        } else if (s[i] == ')') {
            depth--;
            if (depth == 0) {
                *close_out = i;
                return 1;
            }
            if (depth < 0) {
                return 0;
            }
        }
    }
    return 0;
}

static Slice first_call_arg(const char *raw, size_t open, size_t close) {
    int depth = 0;
    size_t start = open + 1;

    for (size_t i = start; i < close; i++) {
        if (raw[i] == '(') {
            depth++;
        } else if (raw[i] == ')') {
            depth--;
        } else if (raw[i] == ',' && depth == 0) {
            return (Slice){raw + start, i - start};
        }
    }
    return (Slice){raw + start, close - start};
}

static int add_negated_condition(FunctionContracts *fn, Slice condition) {
    return function_add_precondition(fn, formula_negate(lift_predicate_slice(condition)));
}

static int add_errno_return_condition(FunctionContracts *fn, Slice condition) {
    char *name = name_from_not_condition(condition);
    Formula *formula;
    int rc;

    if (name != NULL) {
        formula = nonnull_formula_for_name(name);
        free(name);
    } else {
        formula = formula_negate(lift_predicate_slice(condition));
    }
    rc = function_add_precondition(fn, formula);
    return rc;
}

static int starts_with_word(const char *s, const char *word) {
    size_t n = strlen(word);

    return strncmp(s, word, n) == 0 && !is_ident_char((unsigned char)s[n]);
}

static const char *skip_statement_prefix(const char *s) {
    while (isspace((unsigned char)*s)) {
        s++;
    }
    if (*s == '{') {
        s++;
        while (isspace((unsigned char)*s)) {
            s++;
        }
    }
    return s;
}

static int expression_is_errno_literal(Slice expr) {
    size_t i = 1;

    expr = slice_strip_parens(expr);
    if (expr.n < 2 || expr.s[0] != '-') {
        return 0;
    }
    while (i < expr.n && isspace((unsigned char)expr.s[i])) {
        i++;
    }
    if (i >= expr.n) {
        return 0;
    }
    if (expr.s[i] == 'E' && i + 1 < expr.n &&
        (isalpha((unsigned char)expr.s[i + 1]) || expr.s[i + 1] == '_')) {
        return 1;
    }
    return isdigit((unsigned char)expr.s[i]) != 0;
}

static int expression_is_same_identifier(Slice expr, Slice condition) {
    static const char *const lt_op[] = {"<"};
    size_t pos = 0;
    const char *op = NULL;
    Slice lhs;

    expr = slice_strip_parens(expr);
    condition = slice_strip_parens(condition);
    if (!slice_is_identifier(expr)) {
        return 0;
    }
    if (!find_top_level_operator(condition, lt_op, 1, &pos, &op)) {
        return 0;
    }
    lhs = slice_strip_parens((Slice){condition.s, pos});
    return lhs.n == expr.n && strncmp(lhs.s, expr.s, lhs.n) == 0;
}

static int label_is_early_return(const LabelName *labels, size_t n_labels, Slice label) {
    label = slice_strip_parens(label);
    for (size_t i = 0; i < n_labels; i++) {
        if (strlen(labels[i].name) == label.n && strncmp(labels[i].name, label.s, label.n) == 0) {
            return 1;
        }
    }
    return 0;
}

static void free_labels(LabelName *labels, size_t n_labels) {
    for (size_t i = 0; i < n_labels; i++) {
        free(labels[i].name);
    }
    free(labels);
}

static int add_label(LabelName **labels, size_t *n_labels, const char *name, size_t len) {
    LabelName *next = realloc(*labels, (*n_labels + 1) * sizeof(**labels));

    if (next == NULL) {
        return -1;
    }
    *labels = next;
    (*labels)[*n_labels].name = copy_n(name, len);
    if ((*labels)[*n_labels].name == NULL) {
        return -1;
    }
    (*n_labels)++;
    return 0;
}

static int collect_early_return_labels(const char *safe, LabelName **labels, size_t *n_labels) {
    for (size_t i = 0; safe[i] != '\0'; i++) {
        size_t start;
        size_t len;
        const char *after;

        if (!isalpha((unsigned char)safe[i]) && safe[i] != '_') {
            continue;
        }
        start = i;
        while (is_ident_char((unsigned char)safe[i])) {
            i++;
        }
        len = i - start;
        if (safe[i] != ':' || (len == 4 && strncmp(safe + start, "case", 4) == 0) ||
            (len == 7 && strncmp(safe + start, "default", 7) == 0)) {
            continue;
        }
        after = skip_statement_prefix(safe + i + 1);
        if (starts_with_word(after, "return") && add_label(labels, n_labels, safe + start, len) != 0) {
            return -1;
        }
    }
    return 0;
}

static int scan_macro_calls(FunctionContracts *fn, const char *raw, const char *safe) {
    for (size_t i = 0; safe[i] != '\0'; i++) {
        size_t open = 0;
        size_t close = 0;
        Slice arg;

        if (match_identifier_call(safe, i, "BUILD_BUG_ON", &open)) {
            if (find_matching_paren_text(safe, open, &close)) {
                arg = first_call_arg(raw, open, close);
                if (add_negated_condition(fn, arg) != 0) return -1;
            }
        } else if (match_identifier_call(safe, i, "BUG_ON", &open)) {
            if (find_matching_paren_text(safe, open, &close)) {
                arg = first_call_arg(raw, open, close);
                if (add_negated_condition(fn, arg) != 0) return -1;
            }
        } else if (match_identifier_call(safe, i, "WARN_ON_ONCE", &open) ||
            match_identifier_call(safe, i, "WARN_ON", &open)) {
            if (emit_warn_opacity(fn) != 0) return -1;
        } else if (match_identifier_call(safe, i, "assert", &open) ||
            match_parenthesized_assert_call(safe, i, &open)) {
            if (find_matching_paren_text(safe, open, &close)) {
                arg = first_call_arg(raw, open, close);
                if (function_add_precondition(fn, lift_predicate_slice(arg)) != 0) return -1;
            }
        }
    }
    return 0;
}

static int scan_if_statements(
    FunctionContracts *fn,
    const char *raw,
    const char *safe,
    const LabelName *labels,
    size_t n_labels
) {
    for (size_t i = 0; safe[i] != '\0'; i++) {
        size_t open;
        size_t close;
        const char *stmt;
        Slice condition;

        if ((i > 0 && is_ident_char((unsigned char)safe[i - 1])) ||
            strncmp(safe + i, "if", 2) != 0 ||
            is_ident_char((unsigned char)safe[i + 2])) {
            continue;
        }
        open = i + 2;
        while (isspace((unsigned char)safe[open])) {
            open++;
        }
        if (safe[open] != '(') {
            continue;
        }
        if (!find_matching_paren_text(safe, open, &close)) {
            continue;
        }
        condition = (Slice){raw + open + 1, close - open - 1};
        stmt = skip_statement_prefix(safe + close + 1);
        if (starts_with_word(stmt, "BUG")) {
            if (add_negated_condition(fn, condition) != 0) return -1;
        } else if (starts_with_word(stmt, "return")) {
            const char *expr_start = stmt + strlen("return");
            const char *semi = strchr(expr_start, ';');
            Slice expr;

            if (semi == NULL) {
                continue;
            }
            expr = (Slice){raw + (expr_start - safe), (size_t)(semi - expr_start)};
            if (expression_is_errno_literal(expr)) {
                if (add_errno_return_condition(fn, condition) != 0) return -1;
            } else if (expression_is_same_identifier(expr, condition)) {
                if (add_negated_condition(fn, condition) != 0) return -1;
            }
        } else if (starts_with_word(stmt, "goto")) {
            const char *label_start = stmt + strlen("goto");
            const char *semi = strchr(label_start, ';');
            Slice label;

            if (semi == NULL) {
                continue;
            }
            label = slice_trim((Slice){label_start, (size_t)(semi - label_start)});
            if (label_is_early_return(labels, n_labels, label) &&
                add_negated_condition(fn, condition) != 0) {
                return -1;
            }
        }
    }
    return 0;
}

static int scan_function_source(FunctionContracts *fn, const char *source) {
    char *safe = sanitize_literals(source);
    LabelName *labels = NULL;
    size_t n_labels = 0;
    int rc = -1;

    if (safe == NULL) {
        return -1;
    }
    if (collect_early_return_labels(safe, &labels, &n_labels) != 0) {
        goto done;
    }
    if (scan_macro_calls(fn, source, safe) != 0) {
        goto done;
    }
    if (scan_if_statements(fn, source, safe, labels, n_labels) != 0) {
        goto done;
    }
    rc = 0;
done:
    free_labels(labels, n_labels);
    free(safe);
    return rc;
}

static void function_contracts_free(FunctionContracts *fn) {
    if (fn == NULL) {
        return;
    }
    free(fn->name);
    for (int i = 0; i < fn->n_formals; i++) {
        free(fn->formals[i]);
    }
    free(fn->formals);
    for (size_t i = 0; i < fn->n_preconditions; i++) {
        formula_free(fn->preconditions[i]);
    }
    free(fn->preconditions);
    memset(fn, 0, sizeof(*fn));
}

#ifdef PK_C_ENABLE_CLANG_AST
typedef struct {
    pk_c_lift_result *result;
    const char *path;
    int failed;
    int saw_function;
} AstContractCtx;

static char *clang_string(CXString string) {
    const char *cstr = clang_getCString(string);
    char *copy = copy_string(cstr == NULL ? "" : cstr);

    clang_disposeString(string);
    return copy;
}

static void set_locus(FunctionContracts *fn, CXCursor cursor, const char *fallback_path) {
    CXSourceLocation location = clang_getCursorLocation(cursor);
    unsigned line = 0;
    unsigned column = 0;

    clang_getExpansionLocation(location, NULL, &line, &column, NULL);
    fn->path = fallback_path == NULL ? "" : fallback_path;
    fn->line = (int)line;
    fn->column = (int)column;
}

static char *cursor_source(CXCursor cursor) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    WBuf b;
    char *out;

    if (wbuf_init(&b) != 0) {
        return NULL;
    }
    clang_tokenize(tu, range, &tokens, &ntoks);
    for (unsigned i = 0; i < ntoks; i++) {
        CXString spelling = clang_getTokenSpelling(tu, tokens[i]);
        const char *text = clang_getCString(spelling);

        if (i > 0 && wbuf_append_char(&b, ' ') != 0) {
            clang_disposeString(spelling);
            clang_disposeTokens(tu, tokens, ntoks);
            wbuf_free(&b);
            return NULL;
        }
        if (text != NULL && wbuf_append(&b, text) != 0) {
            clang_disposeString(spelling);
            clang_disposeTokens(tu, tokens, ntoks);
            wbuf_free(&b);
            return NULL;
        }
        clang_disposeString(spelling);
    }
    clang_disposeTokens(tu, tokens, ntoks);
    out = b.data;
    b.data = NULL;
    wbuf_free(&b);
    return out;
}

static int collect_formals(FunctionContracts *fn, CXCursor cursor) {
    int n = clang_Cursor_getNumArguments(cursor);

    if (n < 0) {
        n = 0;
    }
    fn->n_formals = n;
    if (n == 0) {
        return 0;
    }
    fn->formals = calloc((size_t)n, sizeof(*fn->formals));
    if (fn->formals == NULL) {
        return -1;
    }
    for (int i = 0; i < n; i++) {
        CXCursor arg = clang_Cursor_getArgument(cursor, (unsigned)i);
        char fallback[32];
        char *name = clang_string(clang_getCursorSpelling(arg));

        if (name == NULL) {
            return -1;
        }
        if (name[0] == '\0') {
            free(name);
            (void)snprintf(fallback, sizeof(fallback), "x%d", i);
            name = copy_string(fallback);
            if (name == NULL) {
                return -1;
            }
        }
        fn->formals[i] = name;
    }
    return 0;
}

static enum CXChildVisitResult visit_ast_function(CXCursor cursor, CXCursor parent, CXClientData client_data) {
    AstContractCtx *ctx = (AstContractCtx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    (void)parent;

    if (ctx->failed || !clang_Location_isFromMainFile(clang_getCursorLocation(cursor))) {
        return CXChildVisit_Continue;
    }
    if (kind == CXCursor_FunctionDecl && clang_isCursorDefinition(cursor) != 0) {
        FunctionContracts fn;
        char *source;
        int rc;

        memset(&fn, 0, sizeof(fn));
        fn.result = ctx->result;
        fn.name = clang_string(clang_getCursorSpelling(cursor));
        set_locus(&fn, cursor, ctx->path);
        source = cursor_source(cursor);
        if (fn.name == NULL || source == NULL || collect_formals(&fn, cursor) != 0) {
            free(source);
            function_contracts_free(&fn);
            ctx->failed = 1;
            return CXChildVisit_Break;
        }
        rc = scan_function_source(&fn, source);
        free(source);
        if (rc != 0 || emit_function_contract(&fn) != 0) {
            function_contracts_free(&fn);
            ctx->failed = 1;
            return CXChildVisit_Break;
        }
        ctx->saw_function = 1;
        function_contracts_free(&fn);
        return CXChildVisit_Continue;
    }
    return CXChildVisit_Recurse;
}

static int lift_ast_contracts(
    pk_c_lift_result *result,
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    static const char *const default_args[] = {"-x", "c", "-std=c11"};
    const char *filename = path == NULL || path[0] == '\0' ? "source.c" : path;
    const char *contents = source == NULL ? "" : source;
    const char *const *args = default_args;
    int n_args = (int)(sizeof(default_args) / sizeof(default_args[0]));
    struct CXUnsavedFile unsaved;
    CXIndex index;
    CXTranslationUnit unit = NULL;
    enum CXErrorCode error;
    AstContractCtx ctx;

    if (options != NULL && options->clang_args != NULL && options->n_clang_args > 0) {
        args = options->clang_args;
        n_args = (int)options->n_clang_args;
    }
    unsaved.Filename = filename;
    unsaved.Contents = contents;
    unsaved.Length = (unsigned long)strlen(contents);
    index = clang_createIndex(0, 0);
    if (index == NULL) {
        return 0;
    }
    error = clang_parseTranslationUnit2(
        index,
        filename,
        args,
        n_args,
        &unsaved,
        1,
        CXTranslationUnit_DetailedPreprocessingRecord |
            CXTranslationUnit_Incomplete |
            CXTranslationUnit_KeepGoing,
        &unit);
    if (error != CXError_Success || unit == NULL) {
        clang_disposeIndex(index);
        return 0;
    }
    memset(&ctx, 0, sizeof(ctx));
    ctx.result = result;
    ctx.path = filename;
    (void)clang_visitChildren(clang_getTranslationUnitCursor(unit), visit_ast_function, &ctx);
    clang_disposeTranslationUnit(unit);
    clang_disposeIndex(index);
    return ctx.failed ? -1 : 0;
}
#endif

#ifndef PK_C_ENABLE_CLANG_AST
static int emit_regex_fallback_opacity(pk_c_lift_result *result, const pk_c_source_facts *facts) {
    for (size_t i = 0; i < facts->n_macro_calls; i++) {
        const pk_c_macro_call_fact *call = &facts->macro_calls[i];

        if (strcmp(call->name, "WARN_ON") == 0 || strcmp(call->name, "WARN_ON_ONCE") == 0) {
            if (pk_c_lift_result_add_opacity_entry(
                result,
                "c-assertions.warn-on",
                call->locus.path,
                call->locus.line,
                call->locus.column,
                "WARN_ON is non-fatal, so its predicate is not emitted as a hard contract",
                "c-assertions") != 0) {
                return -1;
            }
        }
    }
    return 0;
}
#endif

pk_c_lift_result *pk_c_assertions_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    pk_c_source_facts *facts;

    if (result == NULL) {
        return NULL;
    }
    if (source == NULL) {
        return result;
    }
    facts = pk_c_parse_source_with_options(path, source, options);
    if (facts == NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"severity\":\"error\",\"message\":\"parse failed\"}");
        return result;
    }
    if (append_core_result(result, facts) != 0) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }
#ifdef PK_C_ENABLE_CLANG_AST
    if (lift_ast_contracts(result, path, source, options) != 0) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }
#else
    if (emit_regex_fallback_opacity(result, facts) != 0) {
        pk_c_source_facts_free(facts);
        pk_c_lift_result_free(result);
        return NULL;
    }
#endif
    pk_c_source_facts_free(facts);
    return result;
}

pk_c_lift_result *pk_c_assertions_lift_source(const char *path, const char *source) {
    return pk_c_assertions_lift_source_with_options(path, source, NULL);
}
