/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"
#include "c11_cursor_dispatch.generated.h"

#ifdef PK_C_ENABLE_CLANG_AST

#include <clang-c/Index.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define FOO_WP_NOTE "The branch-sensitive WP value is recorded in foo.expected-wp-contract.json. The current c-collectors-defensive lifter emits the same branch-sensitive contract in foo.contract.json."
#define GENERIC_WP_NOTE "The branch-sensitive WP value is projected from this materialized C11 algebra term by provekit-c11-term-project."

typedef enum {
    C11_TERM_OP = 0,
    C11_TERM_VAR = 1,
    C11_TERM_CONST_INT = 2,
    C11_TERM_UNIT = 3
} C11TermKind;

typedef struct C11Term C11Term;

struct C11Term {
    C11TermKind kind;
    char *name;
    long value;
    C11Term **args;
    size_t n_args;
};

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buf;

typedef struct {
    CXCursor *items;
    size_t len;
    size_t cap;
} CursorList;

typedef struct {
    const char *name;
    CXCursor cursor;
    int found;
} FindFunctionCtx;

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} NameList;

static void term_free(C11Term *term);
static C11Term *lift_stmt(CXCursor cursor);
static C11Term *lift_expr(CXCursor cursor);

static void buf_free(Buf *b) {
    if (b == NULL) return;
    free(b->data);
    memset(b, 0, sizeof(*b));
}

static int buf_grow(Buf *b, size_t add) {
    size_t cap = b->cap == 0 ? 256 : b->cap;
    char *data;

    while (cap < b->len + add + 1) {
        if (cap > ((size_t)-1) / 2) return -1;
        cap *= 2;
    }
    if (cap == b->cap) return 0;
    data = realloc(b->data, cap);
    if (data == NULL) return -1;
    b->data = data;
    b->cap = cap;
    if (b->len == 0) b->data[0] = '\0';
    return 0;
}

static int buf_append_n(Buf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) return -1;
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int buf_append(Buf *b, const char *s) {
    return buf_append_n(b, s, strlen(s));
}

static int buf_append_char(Buf *b, char c) {
    return buf_append_n(b, &c, 1);
}

static int buf_append_long(Buf *b, long value) {
    char tmp[64];

    (void)snprintf(tmp, sizeof(tmp), "%ld", value);
    return buf_append(b, tmp);
}

static int buf_append_indent(Buf *b, int indent) {
    for (int i = 0; i < indent; i++) {
        if (buf_append(b, "    ") != 0) return -1;
    }
    return 0;
}

static int buf_append_json_string(Buf *b, const char *s) {
    if (buf_append_char(b, '"') != 0) return -1;
    for (const unsigned char *p = (const unsigned char *)(s == NULL ? "" : s); *p; p++) {
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
                char esc[7];

                (void)snprintf(esc, sizeof(esc), "\\u%04x", *p);
                if (buf_append(b, esc) != 0) return -1;
            } else if (buf_append_char(b, (char)*p) != 0) {
                return -1;
            }
            break;
        }
    }
    return buf_append_char(b, '"');
}

static const char *path_basename(const char *path) {
    const char *slash;

    if (path == NULL || path[0] == '\0') return "source.c";
    slash = strrchr(path, '/');
    return slash == NULL ? path : slash + 1;
}

static char *read_file(const char *path) {
    FILE *fp = fopen(path, "rb");
    long size;
    char *data;

    if (fp == NULL) return NULL;
    if (fseek(fp, 0, SEEK_END) != 0) {
        fclose(fp);
        return NULL;
    }
    size = ftell(fp);
    if (size < 0 || fseek(fp, 0, SEEK_SET) != 0) {
        fclose(fp);
        return NULL;
    }
    data = malloc((size_t)size + 1);
    if (data == NULL) {
        fclose(fp);
        return NULL;
    }
    if (fread(data, 1, (size_t)size, fp) != (size_t)size) {
        free(data);
        fclose(fp);
        return NULL;
    }
    data[size] = '\0';
    fclose(fp);
    return data;
}

static char *cx_string_copy(CXString string) {
    const char *cstr = clang_getCString(string);
    char *copy = pk_c_walk_copy(cstr == NULL ? "" : cstr);

    clang_disposeString(string);
    return copy;
}

static int cursor_is_main_file(CXCursor cursor) {
    return clang_Location_isFromMainFile(clang_getCursorLocation(cursor)) != 0;
}

static void cursor_list_free(CursorList *list) {
    free(list->items);
    memset(list, 0, sizeof(*list));
}

static int cursor_list_append(CursorList *list, CXCursor cursor) {
    CXCursor *items;
    size_t cap;

    if (list->len == list->cap) {
        cap = list->cap == 0 ? 4 : list->cap * 2;
        if (cap < list->cap) return -1;
        items = realloc(list->items, cap * sizeof(*items));
        if (items == NULL) return -1;
        list->items = items;
        list->cap = cap;
    }
    list->items[list->len++] = cursor;
    return 0;
}

static enum CXChildVisitResult collect_child(CXCursor cursor, CXCursor parent, CXClientData data) {
    CursorList *list = (CursorList *)data;

    (void)parent;
    return cursor_list_append(list, cursor) == 0 ? CXChildVisit_Continue : CXChildVisit_Break;
}

static int cursor_children(CXCursor cursor, CursorList *out) {
    return clang_visitChildren(cursor, collect_child, out) == 0 ? 0 : -1;
}

static C11Term *term_new(C11TermKind kind) {
    C11Term *term = calloc(1, sizeof(*term));

    if (term != NULL) term->kind = kind;
    return term;
}

static C11Term *term_var_take(char *name) {
    C11Term *term = term_new(C11_TERM_VAR);

    if (term == NULL) {
        free(name);
        return NULL;
    }
    term->name = name;
    return term;
}

static C11Term *term_var(const char *name) {
    return term_var_take(pk_c_walk_copy(name));
}

static C11Term *term_const_int(long value) {
    C11Term *term = term_new(C11_TERM_CONST_INT);

    if (term != NULL) term->value = value;
    return term;
}

static C11Term *term_unit(void) {
    return term_new(C11_TERM_UNIT);
}

static C11Term *term_op_take(const char *name, C11Term **args, size_t n_args) {
    C11Term *term = term_new(C11_TERM_OP);

    if (term == NULL) {
        for (size_t i = 0; i < n_args; i++) term_free(args[i]);
        free(args);
        return NULL;
    }
    term->name = pk_c_walk_copy(name);
    term->args = args;
    term->n_args = n_args;
    if (term->name == NULL) {
        term_free(term);
        return NULL;
    }
    return term;
}

static C11Term *term_op1_take(const char *name, C11Term *a) {
    C11Term **args = calloc(1, sizeof(*args));

    if (args == NULL) {
        term_free(a);
        return NULL;
    }
    args[0] = a;
    return term_op_take(name, args, 1);
}

static C11Term *term_op2_take(const char *name, C11Term *a, C11Term *b) {
    C11Term **args = calloc(2, sizeof(*args));

    if (args == NULL) {
        term_free(a);
        term_free(b);
        return NULL;
    }
    args[0] = a;
    args[1] = b;
    return term_op_take(name, args, 2);
}

static C11Term *term_op3_take(const char *name, C11Term *a, C11Term *b, C11Term *c) {
    C11Term **args = calloc(3, sizeof(*args));

    if (args == NULL) {
        term_free(a);
        term_free(b);
        term_free(c);
        return NULL;
    }
    args[0] = a;
    args[1] = b;
    args[2] = c;
    return term_op_take(name, args, 3);
}

static C11Term *term_op4_take(const char *name, C11Term *a, C11Term *b, C11Term *c, C11Term *d) {
    C11Term **args = calloc(4, sizeof(*args));

    if (args == NULL) {
        term_free(a);
        term_free(b);
        term_free(c);
        term_free(d);
        return NULL;
    }
    args[0] = a;
    args[1] = b;
    args[2] = c;
    args[3] = d;
    return term_op_take(name, args, 4);
}

static C11Term *term_skip(void) {
    return term_op1_take("skip", term_unit());
}

static C11Term *term_opaque(void) {
    return term_op1_take("opaque", term_unit());
}

static C11Term *term_clone(const C11Term *term) {
    C11Term **args;

    if (term == NULL) return NULL;
    switch (term->kind) {
    case C11_TERM_VAR:
        return term_var(term->name);
    case C11_TERM_CONST_INT:
        return term_const_int(term->value);
    case C11_TERM_UNIT:
        return term_unit();
    case C11_TERM_OP:
        args = calloc(term->n_args, sizeof(*args));
        if (args == NULL && term->n_args > 0) return NULL;
        for (size_t i = 0; i < term->n_args; i++) {
            args[i] = term_clone(term->args[i]);
            if (args[i] == NULL) {
                for (size_t j = 0; j < i; j++) term_free(args[j]);
                free(args);
                return NULL;
            }
        }
        return term_op_take(term->name, args, term->n_args);
    }
    return NULL;
}

static void term_free(C11Term *term) {
    if (term == NULL) return;
    free(term->name);
    for (size_t i = 0; i < term->n_args; i++) term_free(term->args[i]);
    free(term->args);
    free(term);
}

static const char *op_name_from_cursor(CXCursor cursor) {
    const pk_c11_cursor_dispatch *info = pk_c11_cursor_dispatch_lookup(clang_getCursorKind(cursor));

    if (info == NULL || info->op_name == NULL) return "opaque";
    if (strncmp(info->op_name, "c11:", 4) == 0) return info->op_name + 4;
    return info->op_name;
}

static C11Term *wrap_generic_cursor(CXCursor cursor, const char *fallback) {
    CursorList children = {0};
    C11Term **args = NULL;
    const char *op = fallback == NULL ? op_name_from_cursor(cursor) : fallback;
    C11Term *out = NULL;

    if (strcmp(op, "opaque") == 0) return term_opaque();
    if (cursor_children(cursor, &children) != 0) return term_opaque();
    args = calloc(children.len == 0 ? 1 : children.len, sizeof(*args));
    if (args == NULL && children.len > 0) goto done;
    for (size_t i = 0; i < children.len; i++) {
        if (clang_isExpression(clang_getCursorKind(children.items[i]))) {
            args[i] = lift_expr(children.items[i]);
        } else {
            args[i] = lift_stmt(children.items[i]);
        }
        if (args[i] == NULL) goto done;
    }
    out = term_op_take(op, args, children.len);
    args = NULL;

done:
    if (args != NULL) {
        for (size_t i = 0; i < children.len; i++) term_free(args[i]);
        free(args);
    }
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *unwrap_expr(CXCursor cursor) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    CursorList children = {0};
    C11Term *term = NULL;

    if (kind != CXCursor_UnexposedExpr && kind != CXCursor_ParenExpr) {
        return lift_expr(cursor);
    }
    if (cursor_children(cursor, &children) != 0 || children.len != 1) {
        cursor_list_free(&children);
        return wrap_generic_cursor(cursor, kind == CXCursor_UnexposedExpr ? "unexposed-expr" : NULL);
    }
    term = unwrap_expr(children.items[0]);
    cursor_list_free(&children);
    return term;
}

static int source_has_token(const char *source, const char *token) {
    return source != NULL && strstr(source, token) != NULL;
}

static int source_starts_with(const char *source, const char *token) {
    return source != NULL && strncmp(source, token, strlen(token)) == 0;
}

static int source_ends_with(const char *source, const char *token) {
    size_t source_len;
    size_t token_len;

    if (source == NULL || token == NULL) return 0;
    source_len = strlen(source);
    token_len = strlen(token);
    return source_len >= token_len && strcmp(source + source_len - token_len, token) == 0;
}

static C11Term *lift_binary_expr(CXCursor cursor) {
    CursorList children = {0};
    char *source = NULL;
    C11Term *lhs = NULL;
    C11Term *rhs = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len != 2) goto done;
    source = pk_c_walk_cursor_source(cursor);
    lhs = unwrap_expr(children.items[0]);
    rhs = unwrap_expr(children.items[1]);
    if (lhs == NULL || rhs == NULL) goto done;

    if (source_has_token(source, "==")) {
        out = term_op2_take("eq", lhs, rhs);
    } else if (source_has_token(source, "!=")) {
        out = term_op2_take("ne", lhs, rhs);
    } else if (source_has_token(source, "&&")) {
        out = term_op2_take("and", lhs, rhs);
    } else if (source_has_token(source, "||")) {
        out = term_op2_take("or", lhs, rhs);
    } else if (source_has_token(source, "<=")) {
        out = term_op2_take("le", lhs, rhs);
    } else if (source_has_token(source, ">=")) {
        out = term_op2_take("ge", lhs, rhs);
    } else if (source_has_token(source, "<<")) {
        out = term_op2_take("shl", lhs, rhs);
    } else if (source_has_token(source, ">>")) {
        out = term_op2_take("shr", lhs, rhs);
    } else if (source_has_token(source, "<")) {
        out = term_op2_take("lt", lhs, rhs);
    } else if (source_has_token(source, ">")) {
        out = term_op2_take("gt", lhs, rhs);
    } else if (source_has_token(source, "=")) {
        out = term_op2_take("assign", lhs, rhs);
    } else if (source_has_token(source, "+")) {
        out = term_op2_take("add", lhs, rhs);
    } else if (source_has_token(source, "-")) {
        out = term_op2_take("sub", lhs, rhs);
    } else if (source_has_token(source, "*")) {
        out = term_op2_take("mul", lhs, rhs);
    } else if (source_has_token(source, "/")) {
        out = term_op2_take("div", lhs, rhs);
    } else if (source_has_token(source, "%")) {
        out = term_op2_take("mod", lhs, rhs);
    } else if (source_has_token(source, "&")) {
        out = term_op2_take("bit_and", lhs, rhs);
    } else if (source_has_token(source, "|")) {
        out = term_op2_take("bit_or", lhs, rhs);
    } else if (source_has_token(source, "^")) {
        out = term_op2_take("bit_xor", lhs, rhs);
    } else if (source_has_token(source, ",")) {
        out = term_op2_take("comma", lhs, rhs);
    } else {
        out = term_op2_take("binary-operator", lhs, rhs);
    }
    lhs = NULL;
    rhs = NULL;

done:
    free(source);
    term_free(lhs);
    term_free(rhs);
    cursor_list_free(&children);
    return out == NULL ? wrap_generic_cursor(cursor, "binary-operator") : out;
}

static C11Term *lift_compound_assign_expr(CXCursor cursor) {
    CursorList children = {0};
    char *source = NULL;
    C11Term *lhs = NULL;
    C11Term *lhs_for_rhs = NULL;
    C11Term *rhs = NULL;
    C11Term *value = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len != 2) goto done;
    source = pk_c_walk_cursor_source(cursor);
    lhs = unwrap_expr(children.items[0]);
    lhs_for_rhs = term_clone(lhs);
    rhs = unwrap_expr(children.items[1]);
    if (lhs == NULL || lhs_for_rhs == NULL || rhs == NULL) goto done;
    if (source_has_token(source, "+=")) {
        value = term_op2_take("add", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "-=")) {
        value = term_op2_take("sub", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "*=")) {
        value = term_op2_take("mul", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "/=")) {
        value = term_op2_take("div", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "%=")) {
        value = term_op2_take("mod", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "<<=")) {
        value = term_op2_take("shl", lhs_for_rhs, rhs);
    } else if (source_has_token(source, ">>=")) {
        value = term_op2_take("shr", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "&=")) {
        value = term_op2_take("bit_and", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "|=")) {
        value = term_op2_take("bit_or", lhs_for_rhs, rhs);
    } else if (source_has_token(source, "^=")) {
        value = term_op2_take("bit_xor", lhs_for_rhs, rhs);
    } else {
        value = term_op2_take("binary-operator", lhs_for_rhs, rhs);
    }
    lhs_for_rhs = NULL;
    rhs = NULL;
    if (value == NULL) goto done;
    out = term_op2_take("assign", lhs, value);
    lhs = NULL;
    value = NULL;

done:
    free(source);
    term_free(lhs);
    term_free(lhs_for_rhs);
    term_free(rhs);
    term_free(value);
    cursor_list_free(&children);
    return out == NULL ? wrap_generic_cursor(cursor, "binary-operator") : out;
}

static C11Term *lift_unary_expr(CXCursor cursor) {
    CursorList children = {0};
    char *source = NULL;
    C11Term *inner = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len != 1) goto done;
    source = pk_c_walk_cursor_source(cursor);
    inner = unwrap_expr(children.items[0]);
    if (inner == NULL) goto done;
    if (source_starts_with(source, "++")) {
        out = term_op1_take("pre_inc", inner);
    } else if (source_ends_with(source, "++")) {
        out = term_op1_take("post_inc", inner);
    } else if (source_starts_with(source, "--")) {
        out = term_op1_take("pre_dec", inner);
    } else if (source_ends_with(source, "--")) {
        out = term_op1_take("post_dec", inner);
    } else if (source != NULL && source[0] == '-') {
        out = term_op1_take("neg", inner);
    } else if (source != NULL && source[0] == '!') {
        out = term_op1_take("not", inner);
    } else if (source != NULL && source[0] == '*') {
        out = term_op1_take("deref", inner);
    } else if (source != NULL && source[0] == '~') {
        out = term_op1_take("bit_not", inner);
    } else if (source != NULL && source[0] == '&') {
        out = term_op1_take("addr_of", inner);
    } else if (source != NULL && source[0] == '+') {
        out = term_op1_take("plus", inner);
    } else {
        out = term_op1_take("unary-operator", inner);
    }
    inner = NULL;

done:
    free(source);
    term_free(inner);
    cursor_list_free(&children);
    return out == NULL ? wrap_generic_cursor(cursor, "unary-operator") : out;
}

static C11Term *lift_integer_literal(CXCursor cursor) {
    char *source = pk_c_walk_cursor_source(cursor);
    char *end = NULL;
    long value;

    if (source == NULL) return NULL;
    errno = 0;
    value = strtol(source, &end, 0);
    if (errno != 0 || end == source) {
        free(source);
        return term_opaque();
    }
    free(source);
    return term_const_int(value);
}

static C11Term *lift_expr(CXCursor cursor) {
    switch (clang_getCursorKind(cursor)) {
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
        return unwrap_expr(cursor);
    case CXCursor_DeclRefExpr:
        return term_var_take(cx_string_copy(clang_getCursorSpelling(cursor)));
    case CXCursor_IntegerLiteral:
        return lift_integer_literal(cursor);
    case CXCursor_UnaryOperator:
        return lift_unary_expr(cursor);
    case CXCursor_BinaryOperator:
        return lift_binary_expr(cursor);
    case CXCursor_CompoundAssignOperator:
        return lift_compound_assign_expr(cursor);
    case CXCursor_CStyleCastExpr: {
        CursorList children = {0};
        C11Term *inner = NULL;
        C11Term *out = NULL;

        if (cursor_children(cursor, &children) == 0 && children.len > 0) {
            inner = unwrap_expr(children.items[children.len - 1]);
            if (inner != NULL) {
                out = term_op1_take("cast", inner);
                inner = NULL;
            }
        }
        term_free(inner);
        cursor_list_free(&children);
        return out == NULL ? wrap_generic_cursor(cursor, "cast") : out;
    }
    case CXCursor_ArraySubscriptExpr: {
        CursorList children = {0};
        C11Term *base = NULL;
        C11Term *index = NULL;
        C11Term *out = NULL;

        if (cursor_children(cursor, &children) == 0 && children.len == 2) {
            base = unwrap_expr(children.items[0]);
            index = unwrap_expr(children.items[1]);
            if (base != NULL && index != NULL) {
                out = term_op2_take("array-subscript", base, index);
                base = NULL;
                index = NULL;
            }
        }
        term_free(base);
        term_free(index);
        cursor_list_free(&children);
        return out == NULL ? wrap_generic_cursor(cursor, "array-subscript") : out;
    }
    case CXCursor_ConditionalOperator: {
        CursorList children = {0};
        C11Term *cond = NULL;
        C11Term *then_expr = NULL;
        C11Term *else_expr = NULL;
        C11Term *out = NULL;

        if (cursor_children(cursor, &children) == 0 && children.len == 3) {
            cond = unwrap_expr(children.items[0]);
            then_expr = unwrap_expr(children.items[1]);
            else_expr = unwrap_expr(children.items[2]);
            if (cond != NULL && then_expr != NULL && else_expr != NULL) {
                out = term_op3_take("conditional", cond, then_expr, else_expr);
                cond = NULL;
                then_expr = NULL;
                else_expr = NULL;
            }
        }
        term_free(cond);
        term_free(then_expr);
        term_free(else_expr);
        cursor_list_free(&children);
        return out == NULL ? wrap_generic_cursor(cursor, "conditional") : out;
    }
    case CXCursor_GNUNullExpr:
        return term_op1_take("null", term_unit());
    case CXCursor_StringLiteral:
        return term_op1_take("string-literal", term_unit());
    case CXCursor_CharacterLiteral:
        return term_op1_take("char-literal", term_unit());
    case CXCursor_FloatingLiteral:
        return term_op1_take("float-literal", term_unit());
    default:
        return wrap_generic_cursor(cursor, NULL);
    }
}

static C11Term *fold_terms(CursorList *children, size_t start) {
    C11Term *acc = NULL;

    if (children->len <= start) return term_skip();
    for (size_t i = start; i < children->len; i++) {
        C11Term *next = lift_stmt(children->items[i]);

        if (next == NULL) {
            term_free(acc);
            return NULL;
        }
        if (acc == NULL) {
            acc = next;
        } else {
            acc = term_op2_take("seq", acc, next);
            if (acc == NULL) return NULL;
        }
    }
    return acc == NULL ? term_skip() : acc;
}

static C11Term *lift_compound_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *term;

    if (cursor_children(cursor, &children) != 0) return term_opaque();
    term = fold_terms(&children, 0);
    cursor_list_free(&children);
    return term;
}

static C11Term *lift_return_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *expr = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len > 1) goto done;
    expr = children.len == 0 ? term_unit() : unwrap_expr(children.items[0]);
    if (expr == NULL) goto done;
    out = term_op1_take("return", expr);
    expr = NULL;

done:
    term_free(expr);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_if_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *cond = NULL;
    C11Term *then_term = NULL;
    C11Term *else_term = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len < 2 || children.len > 3) goto done;
    cond = unwrap_expr(children.items[0]);
    then_term = lift_stmt(children.items[1]);
    else_term = children.len == 3 ? lift_stmt(children.items[2]) : term_skip();
    if (cond == NULL || then_term == NULL || else_term == NULL) goto done;
    out = term_op3_take("if", cond, then_term, else_term);
    cond = NULL;
    then_term = NULL;
    else_term = NULL;

done:
    term_free(cond);
    term_free(then_term);
    term_free(else_term);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_var_decl(CXCursor cursor) {
    char *name = cx_string_copy(clang_getCursorSpelling(cursor));
    CXCursor init = clang_Cursor_getVarDeclInitializer(cursor);
    C11Term *var = NULL;
    C11Term *value = NULL;
    C11Term *out = NULL;

    if (name == NULL || name[0] == '\0') goto done;
    var = term_var(name);
    value = clang_Cursor_isNull(init) ? term_unit() : unwrap_expr(init);
    if (var == NULL || value == NULL) goto done;
    out = term_op2_take("decl", var, value);
    var = NULL;
    value = NULL;

done:
    free(name);
    term_free(var);
    term_free(value);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_decl_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *acc = NULL;

    if (cursor_children(cursor, &children) != 0) return term_opaque();
    for (size_t i = 0; i < children.len; i++) {
        C11Term *next;

        if (clang_getCursorKind(children.items[i]) != CXCursor_VarDecl) continue;
        next = lift_var_decl(children.items[i]);
        if (next == NULL) {
            term_free(acc);
            cursor_list_free(&children);
            return term_opaque();
        }
        if (acc == NULL) {
            acc = next;
        } else {
            acc = term_op2_take("seq", acc, next);
            if (acc == NULL) {
                cursor_list_free(&children);
                return term_opaque();
            }
        }
    }
    cursor_list_free(&children);
    return acc == NULL ? term_skip() : acc;
}

static C11Term *lift_switch_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *cond = NULL;
    C11Term *body = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len < 2) goto done;
    cond = unwrap_expr(children.items[0]);
    body = lift_stmt(children.items[1]);
    if (cond == NULL || body == NULL) goto done;
    out = term_op2_take("switch", cond, body);
    cond = NULL;
    body = NULL;

done:
    term_free(cond);
    term_free(body);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_case_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *value = NULL;
    C11Term *body = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len < 1) goto done;
    value = unwrap_expr(children.items[0]);
    body = fold_terms(&children, 1);
    if (value == NULL || body == NULL) goto done;
    out = term_op2_take("case", value, body);
    value = NULL;
    body = NULL;

done:
    term_free(value);
    term_free(body);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_default_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *body = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0) goto done;
    body = fold_terms(&children, 0);
    if (body == NULL) goto done;
    out = term_op1_take("default", body);
    body = NULL;

done:
    term_free(body);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_while_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *cond = NULL;
    C11Term *body = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len < 2) goto done;
    cond = unwrap_expr(children.items[0]);
    body = lift_stmt(children.items[1]);
    if (cond == NULL || body == NULL) goto done;
    out = term_op2_take("while", cond, body);
    cond = NULL;
    body = NULL;

done:
    term_free(cond);
    term_free(body);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_for_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *init = NULL;
    C11Term *cond = NULL;
    C11Term *step = NULL;
    C11Term *body = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0) goto done;
    init = children.len > 0 ? lift_stmt(children.items[0]) : term_skip();
    cond = children.len > 1 ? unwrap_expr(children.items[1]) : term_const_int(1);
    step = children.len > 2 ? lift_stmt(children.items[2]) : term_skip();
    body = children.len > 3 ? lift_stmt(children.items[3]) : term_skip();
    if (init == NULL || cond == NULL || step == NULL || body == NULL) goto done;
    out = term_op4_take("for", init, cond, step, body);
    init = NULL;
    cond = NULL;
    step = NULL;
    body = NULL;

done:
    term_free(init);
    term_free(cond);
    term_free(step);
    term_free(body);
    cursor_list_free(&children);
    return out == NULL ? term_opaque() : out;
}

static C11Term *lift_stmt(CXCursor cursor) {
    switch (clang_getCursorKind(cursor)) {
    case CXCursor_CompoundStmt:
        return lift_compound_stmt(cursor);
    case CXCursor_ReturnStmt:
        return lift_return_stmt(cursor);
    case CXCursor_IfStmt:
        return lift_if_stmt(cursor);
    case CXCursor_NullStmt:
        return term_skip();
    case CXCursor_BinaryOperator:
        return lift_binary_expr(cursor);
    case CXCursor_CompoundAssignOperator:
        return lift_compound_assign_expr(cursor);
    case CXCursor_DeclStmt:
        return lift_decl_stmt(cursor);
    case CXCursor_VarDecl:
        return lift_var_decl(cursor);
    case CXCursor_SwitchStmt:
        return lift_switch_stmt(cursor);
    case CXCursor_CaseStmt:
        return lift_case_stmt(cursor);
    case CXCursor_DefaultStmt:
        return lift_default_stmt(cursor);
    case CXCursor_BreakStmt:
        return term_op1_take("break", term_unit());
    case CXCursor_ContinueStmt:
        return term_op1_take("continue", term_unit());
    case CXCursor_WhileStmt:
        return lift_while_stmt(cursor);
    case CXCursor_ForStmt:
        return lift_for_stmt(cursor);
    case CXCursor_DoStmt:
        return wrap_generic_cursor(cursor, "do");
    default:
        if (clang_isExpression(clang_getCursorKind(cursor))) return lift_expr(cursor);
        return wrap_generic_cursor(cursor, NULL);
    }
}

static enum CXChildVisitResult function_body_finder(CXCursor cursor, CXCursor parent, CXClientData data) {
    CXCursor *body = (CXCursor *)data;

    (void)parent;
    if (clang_getCursorKind(cursor) == CXCursor_CompoundStmt) {
        *body = cursor;
        return CXChildVisit_Break;
    }
    return CXChildVisit_Continue;
}

static CXCursor function_body(CXCursor function_cursor) {
    CXCursor body = clang_getNullCursor();

    (void)clang_visitChildren(function_cursor, function_body_finder, &body);
    return body;
}

static enum CXChildVisitResult find_function_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    FindFunctionCtx *ctx = (FindFunctionCtx *)data;
    char *name;

    (void)parent;
    if (ctx->found || !cursor_is_main_file(cursor) ||
        clang_getCursorKind(cursor) != CXCursor_FunctionDecl ||
        clang_isCursorDefinition(cursor) == 0) {
        return CXChildVisit_Recurse;
    }
    name = cx_string_copy(clang_getCursorSpelling(cursor));
    if (name != NULL && strcmp(name, ctx->name) == 0) {
        ctx->cursor = cursor;
        ctx->found = 1;
        free(name);
        return CXChildVisit_Break;
    }
    free(name);
    return CXChildVisit_Recurse;
}

static CXTranslationUnit parse_unit(const char *path, const char *source, CXIndex *index_out) {
    static const char *const args[] = {"-x", "c", "-std=c11"};
    struct CXUnsavedFile unsaved;
    CXTranslationUnit unit = NULL;

    *index_out = clang_createIndex(0, 0);
    if (*index_out == NULL) return NULL;
    unsaved.Filename = path;
    unsaved.Contents = source;
    unsaved.Length = (unsigned long)strlen(source);
    if (clang_parseTranslationUnit2(
            *index_out,
            path,
            args,
            (int)(sizeof(args) / sizeof(args[0])),
            &unsaved,
            1,
            CXTranslationUnit_DetailedPreprocessingRecord |
                CXTranslationUnit_Incomplete |
                CXTranslationUnit_KeepGoing,
            &unit) != CXError_Success) {
        return NULL;
    }
    return unit;
}

static int term_json(Buf *b, const C11Term *term) {
    if (term == NULL) return -1;
    switch (term->kind) {
    case C11_TERM_VAR:
        return buf_append(b, "{\"kind\":\"var\",\"name\":") == 0 &&
            buf_append_json_string(b, term->name) == 0 &&
            buf_append_char(b, '}') == 0 ? 0 : -1;
    case C11_TERM_CONST_INT:
        return buf_append(b, "{\"kind\":\"const\",\"value\":") == 0 &&
            buf_append_long(b, term->value) == 0 &&
            buf_append(b, ",\"sort\":{\"kind\":\"ctor\",\"name\":\"Int\",\"args\":[]}}") == 0 ? 0 : -1;
    case C11_TERM_UNIT:
        return buf_append(b, "{\"kind\":\"unit\"}");
    case C11_TERM_OP:
        if (buf_append(b, "{\"kind\":\"op\",\"name\":") != 0 ||
            buf_append_json_string(b, term->name) != 0 ||
            buf_append(b, ",\"args\":[") != 0) {
            return -1;
        }
        for (size_t i = 0; i < term->n_args; i++) {
            if ((i > 0 && buf_append_char(b, ',') != 0) ||
                term_json(b, term->args[i]) != 0) {
                return -1;
            }
        }
        return buf_append(b, "]}");
    }
    return -1;
}

static int term_surface(Buf *b, const C11Term *term) {
    if (term == NULL) return -1;
    switch (term->kind) {
    case C11_TERM_VAR:
        return buf_append(b, term->name);
    case C11_TERM_CONST_INT:
        return buf_append_long(b, term->value);
    case C11_TERM_UNIT:
        return buf_append(b, "unit");
    case C11_TERM_OP:
        if (strcmp(term->name, "skip") == 0) return buf_append(b, "skip");
        if (buf_append(b, term->name) != 0 || buf_append_char(b, '(') != 0) return -1;
        for (size_t i = 0; i < term->n_args; i++) {
            if ((i > 0 && buf_append(b, ", ") != 0) ||
                term_surface(b, term->args[i]) != 0) {
                return -1;
            }
        }
        return buf_append_char(b, ')');
    }
    return -1;
}

static int emit_term_memento(Buf *out, const char *source_path, const C11Term *term) {
    Buf surface = {0};
    const char *base = path_basename(source_path);
    const char *note = strcmp(base, "foo.c") == 0 ? FOO_WP_NOTE : GENERIC_WP_NOTE;
    int rc = -1;

    if (term_surface(&surface, term) != 0) goto done;
    if (buf_append(out, "{\"kind\":\"c11-algebra-term\",\"signature_cid\":\"") != 0 ||
        buf_append(out, pk_c11_signature_cid()) != 0 ||
        buf_append(out, "\",\"source\":") != 0 ||
        buf_append_json_string(out, base) != 0 ||
        buf_append(out, ",\"term_surface\":") != 0 ||
        buf_append_json_string(out, surface.data) != 0 ||
        buf_append(out, ",\"term\":") != 0 ||
        term_json(out, term) != 0 ||
        buf_append(out, ",\"wp_value_note\":") != 0 ||
        buf_append_json_string(out, note) != 0 ||
        buf_append(out, "}\n") != 0) {
        goto done;
    }
    rc = 0;

done:
    buf_free(&surface);
    return rc;
}

static C11Term *project_value(const C11Term *term, const C11Term *continuation) {
    C11Term *cont_value = NULL;
    C11Term *then_value = NULL;
    C11Term *else_value = NULL;
    C11Term *cond = NULL;
    C11Term *out = NULL;

    if (term == NULL) return NULL;
    if (term->kind != C11_TERM_OP) return continuation == NULL ? term_clone(term) : term_clone(continuation);
    if (strcmp(term->name, "return") == 0 && term->n_args == 1) return term_clone(term->args[0]);
    if (strcmp(term->name, "skip") == 0) return continuation == NULL ? NULL : term_clone(continuation);
    if ((strcmp(term->name, "decl") == 0 || strcmp(term->name, "assign") == 0 ||
         strcmp(term->name, "break") == 0 || strcmp(term->name, "continue") == 0) &&
        continuation != NULL) {
        return term_clone(continuation);
    }
    if (strcmp(term->name, "seq") == 0 && term->n_args == 2) {
        cont_value = project_value(term->args[1], continuation);
        if (cont_value == NULL) return NULL;
        out = project_value(term->args[0], cont_value);
        term_free(cont_value);
        return out;
    }
    if (strcmp(term->name, "if") == 0 && term->n_args == 3) {
        cond = term_clone(term->args[0]);
        then_value = project_value(term->args[1], continuation);
        else_value = project_value(term->args[2], continuation);
        if (cond == NULL || then_value == NULL || else_value == NULL) {
            term_free(cond);
            term_free(then_value);
            term_free(else_value);
            return NULL;
        }
        return term_op3_take("ite", cond, then_value, else_value);
    }
    return continuation == NULL ? term_clone(term) : term_clone(continuation);
}

static const char *contract_ctor_name(const char *op_name) {
    if (strcmp(op_name, "add") == 0) return "+";
    if (strcmp(op_name, "sub") == 0) return "-";
    if (strcmp(op_name, "mul") == 0) return "*";
    if (strcmp(op_name, "div") == 0) return "/";
    if (strcmp(op_name, "mod") == 0) return "%";
    if (strcmp(op_name, "shl") == 0) return "<<";
    if (strcmp(op_name, "shr") == 0) return ">>";
    if (strcmp(op_name, "bit_and") == 0) return "&";
    if (strcmp(op_name, "bit_or") == 0) return "|";
    if (strcmp(op_name, "bit_xor") == 0) return "^";
    if (strcmp(op_name, "eq") == 0) return "=";
    if (strcmp(op_name, "lt") == 0) return "<";
    if (strcmp(op_name, "le") == 0) return "<=";
    if (strcmp(op_name, "gt") == 0) return ">";
    if (strcmp(op_name, "ge") == 0) return ">=";
    if (strcmp(op_name, "ne") == 0) return "!=";
    return op_name;
}

static int contract_term_json(Buf *b, const C11Term *term) {
    if (term == NULL) return -1;
    switch (term->kind) {
    case C11_TERM_VAR:
        return buf_append(b, "{\"kind\":\"var\",\"name\":") == 0 &&
            buf_append_json_string(b, term->name) == 0 &&
            buf_append_char(b, '}') == 0 ? 0 : -1;
    case C11_TERM_CONST_INT:
        return buf_append(b, "{\"kind\":\"const\",\"value\":") == 0 &&
            buf_append_long(b, term->value) == 0 &&
            buf_append(b, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}") == 0 ? 0 : -1;
    case C11_TERM_UNIT:
        return buf_append(b, "{\"kind\":\"ctor\",\"name\":\"unit\",\"args\":[]}");
    case C11_TERM_OP:
        if (strcmp(term->name, "neg") == 0 && term->n_args == 1 &&
            term->args[0]->kind == C11_TERM_CONST_INT) {
            return buf_append(b, "{\"kind\":\"const\",\"value\":") == 0 &&
                buf_append_long(b, -term->args[0]->value) == 0 &&
                buf_append(b, ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}") == 0 ? 0 : -1;
        }
        if (buf_append(b, "{\"kind\":\"ctor\",\"name\":") != 0 ||
            buf_append_json_string(b, contract_ctor_name(term->name)) != 0 ||
            buf_append(b, ",\"args\":[") != 0) {
            return -1;
        }
        for (size_t i = 0; i < term->n_args; i++) {
            if ((i > 0 && buf_append_char(b, ',') != 0) ||
                contract_term_json(b, term->args[i]) != 0) {
                return -1;
            }
        }
        return buf_append(b, "]}");
    }
    return -1;
}

static char *sort_name_for_type(CXType type) {
    char *spelling = cx_string_copy(clang_getTypeSpelling(type));

    if (spelling == NULL) return NULL;
    if (strcmp(spelling, "int") == 0) {
        free(spelling);
        return pk_c_walk_copy("i32");
    }
    return spelling;
}

static int append_sort(Buf *b, const char *sort_name) {
    return buf_append(b, "{\"kind\":\"primitive\",\"name\":") == 0 &&
        buf_append_json_string(b, sort_name == NULL ? "unknown" : sort_name) == 0 &&
        buf_append_char(b, '}') == 0 ? 0 : -1;
}

static int emit_projected_contract(Buf *out, CXCursor function_cursor, const C11Term *term) {
    char *fn_name = cx_string_copy(clang_getCursorSpelling(function_cursor));
    char **formals = NULL;
    char **sorts = NULL;
    char *return_sort = NULL;
    size_t n_formals = 0;
    C11Term *value = NULL;
    int rc = -1;

    if (fn_name == NULL) goto done;
    formals = pk_c_walk_lift_formals(function_cursor, &n_formals);
    sorts = calloc(n_formals == 0 ? 1 : n_formals, sizeof(*sorts));
    if ((formals == NULL && n_formals > 0) || sorts == NULL) goto done;
    for (size_t i = 0; i < n_formals; i++) {
        CXCursor arg = clang_Cursor_getArgument(function_cursor, (unsigned)i);

        sorts[i] = sort_name_for_type(clang_getCursorType(arg));
        if (sorts[i] == NULL) goto done;
    }
    return_sort = sort_name_for_type(clang_getCursorResultType(function_cursor));
    value = project_value(term, NULL);
    if (return_sort == NULL || value == NULL) goto done;
    if (buf_append(out, "{\"auto_minted_mementos\":[],\"body_cid\":null,\"fn_name\":") != 0 ||
        buf_append_json_string(out, fn_name) != 0 ||
        buf_append(out, ",\"formal_sorts\":[") != 0) {
        goto done;
    }
    for (size_t i = 0; i < n_formals; i++) {
        if ((i > 0 && buf_append_char(out, ',') != 0) || append_sort(out, sorts[i]) != 0) goto done;
    }
    if (buf_append(out, "],\"formals\":[") != 0) goto done;
    for (size_t i = 0; i < n_formals; i++) {
        if ((i > 0 && buf_append_char(out, ',') != 0) || buf_append_json_string(out, formals[i]) != 0) goto done;
    }
    if (buf_append(out, "],\"kind\":\"function-contract\",\"locus\":{\"col\":0,\"file\":null,\"line\":0},\"post\":{\"args\":[{\"kind\":\"var\",\"name\":\"result\"},") != 0 ||
        contract_term_json(out, value) != 0 ||
        buf_append(out, "],\"kind\":\"atomic\",\"name\":\"=\"},\"pre\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"true\"},\"effects\":{\"effects\":[]},\"return_sort\":") != 0 ||
        append_sort(out, return_sort) != 0 ||
        buf_append(out, "}\n") != 0) {
        goto done;
    }
    rc = 0;

done:
    free(fn_name);
    pk_c_walk_lift_string_list_free(formals, n_formals);
    if (sorts != NULL) {
        for (size_t i = 0; i < n_formals; i++) free(sorts[i]);
    }
    free(sorts);
    free(return_sort);
    term_free(value);
    return rc;
}

static void json_skip_ws(const char **p) {
    while (**p == ' ' || **p == '\t' || **p == '\r' || **p == '\n') (*p)++;
}

static int json_hex_value(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static char *decode_json_string(const char *start, const char **end_out) {
    size_t cap = strlen(start) + 1;
    char *out = malloc(cap);
    size_t len = 0;
    const char *p = start;

    if (out == NULL) return NULL;
    while (*p != '\0' && *p != '"') {
        if (*p == '\\') {
            p++;
            switch (*p) {
            case '"':
            case '\\':
            case '/':
                out[len++] = *p++;
                break;
            case 'b':
                out[len++] = '\b';
                p++;
                break;
            case 'f':
                out[len++] = '\f';
                p++;
                break;
            case 'n':
                out[len++] = '\n';
                p++;
                break;
            case 'r':
                out[len++] = '\r';
                p++;
                break;
            case 't':
                out[len++] = '\t';
                p++;
                break;
            case 'u': {
                unsigned value = 0;
                p++;
                for (int i = 0; i < 4; i++) {
                    int h = json_hex_value(p[i]);
                    if (h < 0) {
                        free(out);
                        return NULL;
                    }
                    value = (value << 4) | (unsigned)h;
                }
                p += 4;
                out[len++] = value < 0x80 ? (char)value : '?';
                break;
            }
            default:
                free(out);
                return NULL;
            }
        } else {
            out[len++] = *p++;
        }
        if (len + 1 >= cap) {
            char *next;

            cap *= 2;
            next = realloc(out, cap);
            if (next == NULL) {
                free(out);
                return NULL;
            }
            out = next;
        }
    }
    if (*p != '"') {
        free(out);
        return NULL;
    }
    out[len] = '\0';
    if (end_out != NULL) *end_out = p;
    return out;
}

static int json_skip_value(const char **p);

static int json_skip_string(const char **p) {
    char *tmp;
    const char *end = NULL;

    if (**p != '"') return 0;
    tmp = decode_json_string(*p + 1, &end);
    if (tmp == NULL || end == NULL || *end != '"') {
        free(tmp);
        return 0;
    }
    free(tmp);
    *p = end + 1;
    return 1;
}

static int json_skip_number(const char **p) {
    char *end = NULL;

    errno = 0;
    (void)strtol(*p, &end, 10);
    if (errno != 0 || end == *p) return 0;
    *p = end;
    return 1;
}

static int json_skip_array(const char **p) {
    if (**p != '[') return 0;
    (*p)++;
    json_skip_ws(p);
    if (**p == ']') {
        (*p)++;
        return 1;
    }
    for (;;) {
        if (!json_skip_value(p)) return 0;
        json_skip_ws(p);
        if (**p == ']') {
            (*p)++;
            return 1;
        }
        if (**p != ',') return 0;
        (*p)++;
        json_skip_ws(p);
    }
}

static int json_skip_object(const char **p) {
    if (**p != '{') return 0;
    (*p)++;
    json_skip_ws(p);
    if (**p == '}') {
        (*p)++;
        return 1;
    }
    for (;;) {
        if (!json_skip_string(p)) return 0;
        json_skip_ws(p);
        if (**p != ':') return 0;
        (*p)++;
        if (!json_skip_value(p)) return 0;
        json_skip_ws(p);
        if (**p == '}') {
            (*p)++;
            return 1;
        }
        if (**p != ',') return 0;
        (*p)++;
        json_skip_ws(p);
    }
}

static int json_skip_value(const char **p) {
    json_skip_ws(p);
    if (**p == '"') return json_skip_string(p);
    if (**p == '{') return json_skip_object(p);
    if (**p == '[') return json_skip_array(p);
    if (strncmp(*p, "true", 4) == 0) {
        *p += 4;
        return 1;
    }
    if (strncmp(*p, "false", 5) == 0) {
        *p += 5;
        return 1;
    }
    if (strncmp(*p, "null", 4) == 0) {
        *p += 4;
        return 1;
    }
    return json_skip_number(p);
}

static int parse_term_value(const char **p, C11Term **out);

static int parse_args_array(const char **p, C11Term ***args_out, size_t *n_args_out) {
    C11Term **args = NULL;
    size_t len = 0;
    size_t cap = 0;

    json_skip_ws(p);
    if (**p != '[') return 0;
    (*p)++;
    json_skip_ws(p);
    if (**p == ']') {
        (*p)++;
        *args_out = NULL;
        *n_args_out = 0;
        return 1;
    }
    for (;;) {
        C11Term **next;

        if (len == cap) {
            cap = cap == 0 ? 4 : cap * 2;
            next = realloc(args, cap * sizeof(*args));
            if (next == NULL) goto fail;
            args = next;
        }
        args[len] = NULL;
        if (!parse_term_value(p, &args[len])) goto fail;
        len++;
        json_skip_ws(p);
        if (**p == ']') {
            (*p)++;
            *args_out = args;
            *n_args_out = len;
            return 1;
        }
        if (**p != ',') goto fail;
        (*p)++;
        json_skip_ws(p);
    }

fail:
    for (size_t i = 0; i < len; i++) term_free(args[i]);
    free(args);
    return 0;
}

static int parse_term_object(const char **p, C11Term **out) {
    char *kind = NULL;
    char *name = NULL;
    C11Term **args = NULL;
    size_t n_args = 0;
    long value = 0;
    int have_value = 0;
    int ok = 0;

    json_skip_ws(p);
    if (**p != '{') return 0;
    (*p)++;
    json_skip_ws(p);
    while (**p != '\0' && **p != '}') {
        char *key = NULL;
        const char *end = NULL;

        if (**p != '"') goto done;
        key = decode_json_string(*p + 1, &end);
        if (key == NULL || end == NULL || *end != '"') {
            free(key);
            goto done;
        }
        *p = end + 1;
        json_skip_ws(p);
        if (**p != ':') {
            free(key);
            goto done;
        }
        (*p)++;
        json_skip_ws(p);
        if (strcmp(key, "kind") == 0 || strcmp(key, "name") == 0) {
            char **target = strcmp(key, "kind") == 0 ? &kind : &name;
            free(*target);
            if (**p != '"') {
                free(key);
                goto done;
            }
            *target = decode_json_string(*p + 1, &end);
            if (*target == NULL || end == NULL || *end != '"') {
                free(key);
                goto done;
            }
            *p = end + 1;
        } else if (strcmp(key, "value") == 0) {
            char *num_end = NULL;

            errno = 0;
            value = strtol(*p, &num_end, 10);
            if (errno != 0 || num_end == *p) {
                free(key);
                goto done;
            }
            *p = num_end;
            have_value = 1;
        } else if (strcmp(key, "args") == 0) {
            for (size_t i = 0; i < n_args; i++) term_free(args[i]);
            free(args);
            args = NULL;
            n_args = 0;
            if (!parse_args_array(p, &args, &n_args)) {
                free(key);
                goto done;
            }
        } else if (!json_skip_value(p)) {
            free(key);
            goto done;
        }
        free(key);
        json_skip_ws(p);
        if (**p == ',') {
            (*p)++;
            json_skip_ws(p);
        } else if (**p != '}') {
            goto done;
        }
    }
    if (**p != '}') goto done;
    (*p)++;
    if (kind == NULL) goto done;
    if (strcmp(kind, "op") == 0) {
        if (name == NULL) goto done;
        *out = term_op_take(name, args, n_args);
        name = NULL;
        args = NULL;
        n_args = 0;
    } else if (strcmp(kind, "var") == 0) {
        if (name == NULL) goto done;
        *out = term_var_take(name);
        name = NULL;
    } else if (strcmp(kind, "const") == 0) {
        if (!have_value) goto done;
        *out = term_const_int(value);
    } else if (strcmp(kind, "unit") == 0) {
        *out = term_unit();
    } else {
        goto done;
    }
    ok = *out != NULL;

done:
    free(kind);
    free(name);
    for (size_t i = 0; i < n_args; i++) term_free(args[i]);
    free(args);
    return ok;
}

static int parse_term_value(const char **p, C11Term **out) {
    return parse_term_object(p, out);
}

static int parse_term_document(const char *json, C11Term **out) {
    const char *p = json;
    const char *term_start = NULL;

    json_skip_ws(&p);
    if (*p != '{') return 0;
    p++;
    json_skip_ws(&p);
    while (*p != '\0' && *p != '}') {
        char *key;
        const char *end = NULL;

        if (*p != '"') return 0;
        key = decode_json_string(p + 1, &end);
        if (key == NULL || end == NULL || *end != '"') {
            free(key);
            return 0;
        }
        p = end + 1;
        json_skip_ws(&p);
        if (*p != ':') {
            free(key);
            return 0;
        }
        p++;
        json_skip_ws(&p);
        if (strcmp(key, "term") == 0) {
            term_start = p;
            free(key);
            break;
        }
        free(key);
        if (!json_skip_value(&p)) return 0;
        json_skip_ws(&p);
        if (*p == ',') {
            p++;
            json_skip_ws(&p);
        }
    }
    if (term_start != NULL) {
        p = term_start;
    } else {
        p = json;
    }
    return parse_term_value(&p, out);
}

static int serialize_expr(Buf *out, const C11Term *term);
static int serialize_stmt(Buf *out, const C11Term *term, int indent);

static int serialize_infix(Buf *out, const C11Term *term, const char *op) {
    return term->n_args == 2 &&
        buf_append_char(out, '(') == 0 &&
        serialize_expr(out, term->args[0]) == 0 &&
        buf_append_char(out, ' ') == 0 &&
        buf_append(out, op) == 0 &&
        buf_append_char(out, ' ') == 0 &&
        serialize_expr(out, term->args[1]) == 0 &&
        buf_append_char(out, ')') == 0 ? 0 : -1;
}

static int serialize_expr(Buf *out, const C11Term *term) {
    if (term == NULL) return -1;
    switch (term->kind) {
    case C11_TERM_VAR:
        return buf_append(out, term->name);
    case C11_TERM_CONST_INT:
        return buf_append_long(out, term->value);
    case C11_TERM_UNIT:
        return buf_append(out, "0");
    case C11_TERM_OP:
        if (strcmp(term->name, "add") == 0) return serialize_infix(out, term, "+");
        if (strcmp(term->name, "sub") == 0) return serialize_infix(out, term, "-");
        if (strcmp(term->name, "mul") == 0) return serialize_infix(out, term, "*");
        if (strcmp(term->name, "div") == 0) return serialize_infix(out, term, "/");
        if (strcmp(term->name, "mod") == 0) return serialize_infix(out, term, "%");
        if (strcmp(term->name, "shl") == 0) return serialize_infix(out, term, "<<");
        if (strcmp(term->name, "shr") == 0) return serialize_infix(out, term, ">>");
        if (strcmp(term->name, "bit_and") == 0) return serialize_infix(out, term, "&");
        if (strcmp(term->name, "bit_or") == 0) return serialize_infix(out, term, "|");
        if (strcmp(term->name, "bit_xor") == 0) return serialize_infix(out, term, "^");
        if (strcmp(term->name, "eq") == 0) return serialize_infix(out, term, "==");
        if (strcmp(term->name, "lt") == 0) return serialize_infix(out, term, "<");
        if (strcmp(term->name, "le") == 0) return serialize_infix(out, term, "<=");
        if (strcmp(term->name, "gt") == 0) return serialize_infix(out, term, ">");
        if (strcmp(term->name, "ge") == 0) return serialize_infix(out, term, ">=");
        if (strcmp(term->name, "ne") == 0) return serialize_infix(out, term, "!=");
        if (strcmp(term->name, "and") == 0) return serialize_infix(out, term, "&&");
        if (strcmp(term->name, "or") == 0) return serialize_infix(out, term, "||");
        if (strcmp(term->name, "comma") == 0) return serialize_infix(out, term, ",");
        if (strcmp(term->name, "assign") == 0) return serialize_infix(out, term, "=");
        if (strcmp(term->name, "neg") == 0 && term->n_args == 1) {
            return buf_append(out, "(-") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "not") == 0 && term->n_args == 1) {
            return buf_append(out, "(!") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "deref") == 0 && term->n_args == 1) {
            return buf_append(out, "(*") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "bit_not") == 0 && term->n_args == 1) {
            return buf_append(out, "(~") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "addr_of") == 0 && term->n_args == 1) {
            return buf_append(out, "(&") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "pre_inc") == 0 && term->n_args == 1) {
            return buf_append(out, "(++") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "post_inc") == 0 && term->n_args == 1) {
            return buf_append_char(out, '(') == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append(out, "++)") == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "pre_dec") == 0 && term->n_args == 1) {
            return buf_append(out, "(--") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "post_dec") == 0 && term->n_args == 1) {
            return buf_append_char(out, '(') == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append(out, "--)") == 0 ? 0 : -1;
        }
        if (strcmp(term->name, "plus") == 0 && term->n_args == 1) {
            return buf_append(out, "(+") == 0 && serialize_expr(out, term->args[0]) == 0 && buf_append_char(out, ')') == 0 ? 0 : -1;
        }
        fprintf(stderr, "serialize not implemented for expression op c11:%s\n", term->name);
        return -1;
    }
    return -1;
}

static int serialize_stmt(Buf *out, const C11Term *term, int indent) {
    if (term == NULL) return -1;
    if (term->kind != C11_TERM_OP) {
        return buf_append_indent(out, indent) == 0 &&
            serialize_expr(out, term) == 0 &&
            buf_append(out, ";\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "seq") == 0 && term->n_args == 2) {
        return serialize_stmt(out, term->args[0], indent) == 0 &&
            serialize_stmt(out, term->args[1], indent) == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "skip") == 0) {
        return buf_append_indent(out, indent) == 0 && buf_append(out, ";\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "return") == 0 && term->n_args == 1) {
        return buf_append_indent(out, indent) == 0 &&
            buf_append(out, "return ") == 0 &&
            serialize_expr(out, term->args[0]) == 0 &&
            buf_append(out, ";\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "if") == 0 && term->n_args == 3) {
        return buf_append_indent(out, indent) == 0 &&
            buf_append(out, "if (") == 0 &&
            serialize_expr(out, term->args[0]) == 0 &&
            buf_append(out, ") {\n") == 0 &&
            serialize_stmt(out, term->args[1], indent + 1) == 0 &&
            buf_append_indent(out, indent) == 0 &&
            buf_append(out, "} else {\n") == 0 &&
            serialize_stmt(out, term->args[2], indent + 1) == 0 &&
            buf_append_indent(out, indent) == 0 &&
            buf_append(out, "}\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "decl") == 0 && term->n_args == 2 && term->args[0]->kind == C11_TERM_VAR) {
        if (buf_append_indent(out, indent) != 0 ||
            buf_append(out, "int ") != 0 ||
            buf_append(out, term->args[0]->name) != 0) {
            return -1;
        }
        if (term->args[1]->kind != C11_TERM_UNIT &&
            (buf_append(out, " = ") != 0 || serialize_expr(out, term->args[1]) != 0)) {
            return -1;
        }
        return buf_append(out, ";\n");
    }
    if (strcmp(term->name, "assign") == 0 && term->n_args == 2) {
        return buf_append_indent(out, indent) == 0 &&
            serialize_expr(out, term->args[0]) == 0 &&
            buf_append(out, " = ") == 0 &&
            serialize_expr(out, term->args[1]) == 0 &&
            buf_append(out, ";\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "switch") == 0 && term->n_args == 2) {
        return buf_append_indent(out, indent) == 0 &&
            buf_append(out, "switch (") == 0 &&
            serialize_expr(out, term->args[0]) == 0 &&
            buf_append(out, ") {\n") == 0 &&
            serialize_stmt(out, term->args[1], indent + 1) == 0 &&
            buf_append_indent(out, indent) == 0 &&
            buf_append(out, "}\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "case") == 0 && term->n_args == 2) {
        return buf_append_indent(out, indent) == 0 &&
            buf_append(out, "case ") == 0 &&
            serialize_expr(out, term->args[0]) == 0 &&
            buf_append(out, ":\n") == 0 &&
            serialize_stmt(out, term->args[1], indent + 1) == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "default") == 0 && term->n_args == 1) {
        return buf_append_indent(out, indent) == 0 &&
            buf_append(out, "default:\n") == 0 &&
            serialize_stmt(out, term->args[0], indent + 1) == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "break") == 0) {
        return buf_append_indent(out, indent) == 0 && buf_append(out, "break;\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "continue") == 0) {
        return buf_append_indent(out, indent) == 0 && buf_append(out, "continue;\n") == 0 ? 0 : -1;
    }
    if (strcmp(term->name, "opaque") == 0) {
        fprintf(stderr, "serialize not implemented for c11:opaque\n");
        return -1;
    }
    return buf_append_indent(out, indent) == 0 &&
        serialize_expr(out, term) == 0 &&
        buf_append(out, ";\n") == 0 ? 0 : -1;
}

static void name_list_free(NameList *list) {
    if (list == NULL) return;
    for (size_t i = 0; i < list->len; i++) free(list->items[i]);
    free(list->items);
    memset(list, 0, sizeof(*list));
}

static int name_list_contains(const NameList *list, const char *name) {
    for (size_t i = 0; i < list->len; i++) {
        if (strcmp(list->items[i], name) == 0) return 1;
    }
    return 0;
}

static int name_list_add(NameList *list, const char *name) {
    char **items;
    size_t cap;

    if (name == NULL || name[0] == '\0' || name_list_contains(list, name)) return 0;
    if (list->len == list->cap) {
        cap = list->cap == 0 ? 4 : list->cap * 2;
        if (cap < list->cap) return -1;
        items = realloc(list->items, cap * sizeof(*items));
        if (items == NULL) return -1;
        list->items = items;
        list->cap = cap;
    }
    list->items[list->len] = pk_c_walk_copy(name);
    if (list->items[list->len] == NULL) return -1;
    list->len++;
    return 0;
}

static int collect_names(const C11Term *term, NameList *locals, NameList *params, int decl_name_position) {
    if (term == NULL) return 0;
    if (term->kind == C11_TERM_VAR) {
        if (decl_name_position) return name_list_add(locals, term->name);
        if (!name_list_contains(locals, term->name)) return name_list_add(params, term->name);
        return 0;
    }
    if (term->kind != C11_TERM_OP) return 0;
    for (size_t i = 0; i < term->n_args; i++) {
        int is_decl_name = strcmp(term->name, "decl") == 0 && i == 0;

        if (collect_names(term->args[i], locals, params, is_decl_name) != 0) return -1;
    }
    return 0;
}

static int serialize_translation_unit(Buf *out, const C11Term *term, const char *function_name) {
    NameList locals = {0};
    NameList params = {0};
    int rc = -1;

    if (collect_names(term, &locals, &params, 0) != 0) goto done;
    if (buf_append(out, "int ") != 0 ||
        buf_append(out, function_name == NULL ? "roundtrip" : function_name) != 0 ||
        buf_append_char(out, '(') != 0) {
        goto done;
    }
    for (size_t i = 0; i < params.len; i++) {
        if ((i > 0 && buf_append(out, ", ") != 0) ||
            buf_append(out, "int ") != 0 ||
            buf_append(out, params.items[i]) != 0) {
            goto done;
        }
    }
    if (buf_append(out, ") {\n") != 0 ||
        serialize_stmt(out, term, 1) != 0 ||
        buf_append(out, "}\n") != 0) {
        goto done;
    }
    rc = 0;

done:
    name_list_free(&locals);
    name_list_free(&params);
    return rc;
}

static int build_term_from_source(const char *path, const char *function_name, C11Term **term_out, CXCursor *function_out, CXIndex *index_out, CXTranslationUnit *unit_out) {
    char *source = NULL;
    FindFunctionCtx find_ctx = {0};
    CXCursor body;

    source = read_file(path);
    if (source == NULL) {
        fprintf(stderr, "failed to read %s\n", path);
        return -1;
    }
    *unit_out = parse_unit(path, source, index_out);
    free(source);
    if (*unit_out == NULL) {
        fprintf(stderr, "libclang parse failed for %s\n", path);
        return -1;
    }
    find_ctx.name = function_name;
    (void)clang_visitChildren(clang_getTranslationUnitCursor(*unit_out), find_function_visitor, &find_ctx);
    if (!find_ctx.found) {
        fprintf(stderr, "function not found: %s\n", function_name);
        return -1;
    }
    body = function_body(find_ctx.cursor);
    if (clang_Cursor_isNull(body)) {
        fprintf(stderr, "function has no body: %s\n", function_name);
        return -1;
    }
    *term_out = lift_compound_stmt(body);
    if (*term_out == NULL) return -1;
    if (function_out != NULL) *function_out = find_ctx.cursor;
    return 0;
}

static int usage(const char *argv0) {
    fprintf(stderr, "usage: %s SOURCE.c --function NAME (--term|--contract)\n", argv0);
    fprintf(stderr, "       %s --serialize TERM.json --function NAME\n", argv0);
    return 2;
}

int main(int argc, char **argv) {
    const char *path = NULL;
    const char *function_name = NULL;
    const char *serialize_path = NULL;
    int want_term = 0;
    int want_contract = 0;
    CXIndex index = NULL;
    CXTranslationUnit unit = NULL;
    CXCursor function_cursor = clang_getNullCursor();
    C11Term *term = NULL;
    Buf out = {0};
    int rc = 1;

    if (argc < 2) return usage(argv[0]);
    if (strcmp(argv[1], "--serialize") == 0) {
        if (argc < 5) return usage(argv[0]);
        serialize_path = argv[2];
    } else {
        path = argv[1];
    }
    for (int i = serialize_path == NULL ? 2 : 3; i < argc; i++) {
        if (strcmp(argv[i], "--function") == 0 && i + 1 < argc) {
            function_name = argv[++i];
        } else if (strcmp(argv[i], "--term") == 0) {
            want_term = 1;
        } else if (strcmp(argv[i], "--contract") == 0) {
            want_contract = 1;
        } else {
            return usage(argv[0]);
        }
    }
    if (function_name == NULL) return usage(argv[0]);
    if (serialize_path != NULL) {
        char *json = read_file(serialize_path);

        if (json == NULL) {
            fprintf(stderr, "failed to read %s\n", serialize_path);
            goto done;
        }
        if (!parse_term_document(json, &term)) {
            free(json);
            fprintf(stderr, "failed to parse term JSON %s\n", serialize_path);
            goto done;
        }
        free(json);
        if (serialize_translation_unit(&out, term, function_name) != 0) goto done;
        fputs(out.data == NULL ? "" : out.data, stdout);
        rc = 0;
        goto done;
    }
    if (path == NULL || (want_term == want_contract)) return usage(argv[0]);
    if (build_term_from_source(path, function_name, &term, &function_cursor, &index, &unit) != 0) goto done;
    if (want_term) {
        if (emit_term_memento(&out, path, term) != 0) goto done;
    } else if (emit_projected_contract(&out, function_cursor, term) != 0) {
        goto done;
    }
    fputs(out.data == NULL ? "" : out.data, stdout);
    rc = 0;

done:
    buf_free(&out);
    term_free(term);
    if (unit != NULL) clang_disposeTranslationUnit(unit);
    if (index != NULL) clang_disposeIndex(index);
    return rc;
}

#else

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    fprintf(stderr, "provekit-c11-term-project requires libclang support\n");
    return 1;
}

#endif
