/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"

#include <clang-c/Index.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef PK_C_ENABLE_CLANG_AST

#define C11_SIGNATURE_CID "blake3-512:c942ba70e4b701e139a46590116f5cdc16ab41db277e80e54c01f23e4a7cf6241d4431c60473409cb3f0b61ce27f593071c1c224291f04c61aa04b4773764945"
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

static void term_free(C11Term *term);

static void buf_free(Buf *b) {
    if (b == NULL) {
        return;
    }
    free(b->data);
    memset(b, 0, sizeof(*b));
}

static int buf_grow(Buf *b, size_t add) {
    size_t cap = b->cap == 0 ? 256 : b->cap;
    char *data;

    while (cap < b->len + add + 1) {
        if (cap > ((size_t)-1) / 2) {
            return -1;
        }
        cap *= 2;
    }
    if (cap == b->cap) {
        return 0;
    }
    data = realloc(b->data, cap);
    if (data == NULL) {
        return -1;
    }
    b->data = data;
    b->cap = cap;
    if (b->len == 0) {
        b->data[0] = '\0';
    }
    return 0;
}

static int buf_append_n(Buf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) {
        return -1;
    }
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

static int buf_append_json_string(Buf *b, const char *s) {
    if (buf_append_char(b, '"') != 0) {
        return -1;
    }
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

    if (path == NULL || path[0] == '\0') {
        return "source.c";
    }
    slash = strrchr(path, '/');
    return slash == NULL ? path : slash + 1;
}

static char *read_file(const char *path) {
    FILE *fp = fopen(path, "rb");
    long size;
    char *data;

    if (fp == NULL) {
        return NULL;
    }
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
        if (cap < list->cap) {
            return -1;
        }
        items = realloc(list->items, cap * sizeof(*items));
        if (items == NULL) {
            return -1;
        }
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

    if (term != NULL) {
        term->kind = kind;
    }
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

    if (term != NULL) {
        term->value = value;
    }
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

static C11Term *term_op0(const char *name) {
    return term_op_take(name, NULL, 0);
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

static C11Term *term_skip(void) {
    return term_op1_take("skip", term_unit());
}

static C11Term *term_clone(const C11Term *term) {
    C11Term **args;

    if (term == NULL) {
        return NULL;
    }
    switch (term->kind) {
    case C11_TERM_VAR:
        return term_var(term->name);
    case C11_TERM_CONST_INT:
        return term_const_int(term->value);
    case C11_TERM_UNIT:
        return term_unit();
    case C11_TERM_OP:
        args = calloc(term->n_args, sizeof(*args));
        if (args == NULL && term->n_args > 0) {
            return NULL;
        }
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
    if (term == NULL) {
        return;
    }
    free(term->name);
    for (size_t i = 0; i < term->n_args; i++) {
        term_free(term->args[i]);
    }
    free(term->args);
    free(term);
}

static int report_unsupported(CXCursor cursor, const char *context) {
    CXString spelling = clang_getCursorKindSpelling(clang_getCursorKind(cursor));
    const char *kind = clang_getCString(spelling);
    int line = 0;
    int column = 0;

    pk_c_walk_locus(cursor, &line, &column);
    fprintf(stderr, "unsupported C11 term node in %s: %s at %d:%d\n",
        context == NULL ? "unknown" : context,
        kind == NULL ? "<unknown>" : kind,
        line,
        column);
    clang_disposeString(spelling);
    return -1;
}

static C11Term *lift_stmt(CXCursor cursor);
static C11Term *lift_expr(CXCursor cursor);

static C11Term *unwrap_expr(CXCursor cursor) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    CursorList children = {0};
    C11Term *term = NULL;

    if (kind != CXCursor_UnexposedExpr &&
        kind != CXCursor_ParenExpr) {
        return lift_expr(cursor);
    }
    if (cursor_children(cursor, &children) != 0 || children.len != 1) {
        (void)report_unsupported(cursor, "expression unwrap");
        cursor_list_free(&children);
        return NULL;
    }
    term = unwrap_expr(children.items[0]);
    cursor_list_free(&children);
    return term;
}

static const char *binary_op_name(const char *source) {
    if (source == NULL) {
        return NULL;
    }
    if (strstr(source, "==") != NULL) return "eq";
    if (strstr(source, "&&") != NULL) return "and";
    if (strstr(source, "||") != NULL) return "or";
    if (strstr(source, "<=") != NULL) return "le";
    if (strchr(source, '<') != NULL) return "lt";
    if (strchr(source, '+') != NULL) return "add";
    if (strchr(source, '*') != NULL) return "mul";
    if (strchr(source, '-') != NULL) return "sub";
    if (strchr(source, '=') != NULL) return "assign";
    return NULL;
}

static C11Term *lift_binary_expr(CXCursor cursor) {
    CursorList children = {0};
    char *source = NULL;
    const char *name;
    C11Term *lhs = NULL;
    C11Term *rhs = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len != 2) {
        (void)report_unsupported(cursor, "binary expression arity");
        goto done;
    }
    source = pk_c_walk_cursor_source(cursor);
    name = binary_op_name(source);
    if (name == NULL) {
        (void)report_unsupported(cursor, "binary expression operator");
        goto done;
    }
    lhs = unwrap_expr(children.items[0]);
    rhs = unwrap_expr(children.items[1]);
    if (lhs == NULL || rhs == NULL) {
        goto done;
    }
    out = term_op2_take(name, lhs, rhs);
    lhs = NULL;
    rhs = NULL;

done:
    free(source);
    term_free(lhs);
    term_free(rhs);
    cursor_list_free(&children);
    return out;
}

static C11Term *lift_unary_expr(CXCursor cursor) {
    CursorList children = {0};
    char *source = NULL;
    C11Term *inner = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len != 1) {
        (void)report_unsupported(cursor, "unary expression arity");
        goto done;
    }
    source = pk_c_walk_cursor_source(cursor);
    inner = unwrap_expr(children.items[0]);
    if (inner == NULL) {
        goto done;
    }
    if (source != NULL && source[0] == '-') {
        out = term_op1_take("neg", inner);
        inner = NULL;
    } else if (source != NULL && source[0] == '!') {
        out = term_op1_take("not", inner);
        inner = NULL;
    } else {
        (void)report_unsupported(cursor, "unary expression operator");
    }

done:
    free(source);
    term_free(inner);
    cursor_list_free(&children);
    return out;
}

static C11Term *lift_expr(CXCursor cursor) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
        return unwrap_expr(cursor);
    case CXCursor_DeclRefExpr:
        return term_var_take(cx_string_copy(clang_getCursorSpelling(cursor)));
    case CXCursor_IntegerLiteral: {
        char *source = pk_c_walk_cursor_source(cursor);
        char *end = NULL;
        long value;

        if (source == NULL) {
            return NULL;
        }
        errno = 0;
        value = strtol(source, &end, 0);
        if (errno != 0 || end == source) {
            free(source);
            (void)report_unsupported(cursor, "integer literal");
            return NULL;
        }
        free(source);
        return term_const_int(value);
    }
    case CXCursor_UnaryOperator:
        return lift_unary_expr(cursor);
    case CXCursor_BinaryOperator:
        return lift_binary_expr(cursor);
    case CXCursor_CallExpr:
        return report_unsupported(cursor, "call expression") == 0 ? term_op0("call") : NULL;
    default:
        (void)report_unsupported(cursor, "expression");
        return NULL;
    }
}

static C11Term *fold_sequence(CursorList *children) {
    C11Term *acc = NULL;

    if (children->len == 0) {
        return term_skip();
    }
    for (size_t i = 0; i < children->len; i++) {
        C11Term *next = lift_stmt(children->items[i]);

        if (next == NULL) {
            term_free(acc);
            return NULL;
        }
        if (acc == NULL) {
            acc = next;
        } else {
            acc = term_op2_take("seq", acc, next);
            if (acc == NULL) {
                return NULL;
            }
        }
    }
    return acc;
}

static C11Term *lift_compound_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *term;

    if (cursor_children(cursor, &children) != 0) {
        (void)report_unsupported(cursor, "compound statement");
        return NULL;
    }
    term = fold_sequence(&children);
    cursor_list_free(&children);
    return term;
}

static C11Term *lift_return_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *expr = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len > 1) {
        (void)report_unsupported(cursor, "return statement");
        goto done;
    }
    if (children.len == 0) {
        expr = term_unit();
    } else {
        expr = unwrap_expr(children.items[0]);
    }
    if (expr == NULL) {
        goto done;
    }
    out = term_op1_take("return", expr);
    expr = NULL;

done:
    term_free(expr);
    cursor_list_free(&children);
    return out;
}

static C11Term *lift_if_stmt(CXCursor cursor) {
    CursorList children = {0};
    C11Term *cond = NULL;
    C11Term *then_term = NULL;
    C11Term *else_term = NULL;
    C11Term *out = NULL;

    if (cursor_children(cursor, &children) != 0 || children.len < 2 || children.len > 3) {
        (void)report_unsupported(cursor, "if statement");
        goto done;
    }
    cond = unwrap_expr(children.items[0]);
    then_term = lift_stmt(children.items[1]);
    else_term = children.len == 3 ? lift_stmt(children.items[2]) : term_skip();
    if (cond == NULL || then_term == NULL || else_term == NULL) {
        goto done;
    }
    out = term_op3_take("if", cond, then_term, else_term);
    cond = NULL;
    then_term = NULL;
    else_term = NULL;

done:
    term_free(cond);
    term_free(then_term);
    term_free(else_term);
    cursor_list_free(&children);
    return out;
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
    default:
        (void)report_unsupported(cursor, "statement");
        return NULL;
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
    if (*index_out == NULL) {
        return NULL;
    }
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
    if (term == NULL) {
        return -1;
    }
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
    if (term == NULL) {
        return -1;
    }
    switch (term->kind) {
    case C11_TERM_VAR:
        return buf_append(b, term->name);
    case C11_TERM_CONST_INT:
        return buf_append_long(b, term->value);
    case C11_TERM_UNIT:
        return buf_append(b, "unit");
    case C11_TERM_OP:
        if (strcmp(term->name, "skip") == 0) {
            return buf_append(b, "skip");
        }
        if (buf_append(b, term->name) != 0 || buf_append_char(b, '(') != 0) {
            return -1;
        }
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

static int emit_term_memento(
    Buf *out,
    const char *source_path,
    const C11Term *term
) {
    Buf surface = {0};
    const char *base = path_basename(source_path);
    const char *note = strcmp(base, "foo.c") == 0 ? FOO_WP_NOTE : GENERIC_WP_NOTE;
    int rc = -1;

    if (term_surface(&surface, term) != 0) {
        goto done;
    }
    if (buf_append(out, "{\"kind\":\"c11-algebra-term\",\"signature_cid\":\"") != 0 ||
        buf_append(out, C11_SIGNATURE_CID) != 0 ||
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

    if (term == NULL) {
        return NULL;
    }
    if (term->kind != C11_TERM_OP) {
        return continuation == NULL ? term_clone(term) : term_clone(continuation);
    }
    if (strcmp(term->name, "return") == 0 && term->n_args == 1) {
        return term_clone(term->args[0]);
    }
    if (strcmp(term->name, "skip") == 0) {
        return continuation == NULL ? NULL : term_clone(continuation);
    }
    if (strcmp(term->name, "seq") == 0 && term->n_args == 2) {
        cont_value = project_value(term->args[1], continuation);
        if (cont_value == NULL) {
            return NULL;
        }
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
    if (strcmp(op_name, "eq") == 0) return "=";
    if (strcmp(op_name, "lt") == 0) return "<";
    if (strcmp(op_name, "le") == 0) return "<=";
    return op_name;
}

static int contract_term_json(Buf *b, const C11Term *term) {
    if (term == NULL) {
        return -1;
    }
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

    if (spelling == NULL) {
        return NULL;
    }
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

    if (fn_name == NULL) {
        goto done;
    }
    formals = pk_c_walk_lift_formals(function_cursor, &n_formals);
    sorts = calloc(n_formals, sizeof(*sorts));
    if ((formals == NULL && n_formals > 0) || (sorts == NULL && n_formals > 0)) {
        goto done;
    }
    for (size_t i = 0; i < n_formals; i++) {
        CXCursor arg = clang_Cursor_getArgument(function_cursor, (unsigned)i);

        sorts[i] = sort_name_for_type(clang_getCursorType(arg));
        if (sorts[i] == NULL) {
            goto done;
        }
    }
    return_sort = sort_name_for_type(clang_getCursorResultType(function_cursor));
    value = project_value(term, NULL);
    if (return_sort == NULL || value == NULL) {
        goto done;
    }
    if (buf_append(out, "{\"auto_minted_mementos\":[],\"body_cid\":null,\"fn_name\":") != 0 ||
        buf_append_json_string(out, fn_name) != 0 ||
        buf_append(out, ",\"formal_sorts\":[") != 0) {
        goto done;
    }
    for (size_t i = 0; i < n_formals; i++) {
        if ((i > 0 && buf_append_char(out, ',') != 0) ||
            append_sort(out, sorts[i]) != 0) {
            goto done;
        }
    }
    if (buf_append(out, "],\"formals\":[") != 0) {
        goto done;
    }
    for (size_t i = 0; i < n_formals; i++) {
        if ((i > 0 && buf_append_char(out, ',') != 0) ||
            buf_append_json_string(out, formals[i]) != 0) {
            goto done;
        }
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
        for (size_t i = 0; i < n_formals; i++) {
            free(sorts[i]);
        }
    }
    free(sorts);
    free(return_sort);
    term_free(value);
    return rc;
}

static int usage(const char *argv0) {
    fprintf(stderr, "usage: %s SOURCE.c --function NAME (--term|--contract)\n", argv0);
    return 2;
}

int main(int argc, char **argv) {
    const char *path;
    const char *function_name = NULL;
    int want_term = 0;
    int want_contract = 0;
    char *source = NULL;
    CXIndex index = NULL;
    CXTranslationUnit unit = NULL;
    FindFunctionCtx find_ctx = {0};
    CXCursor body;
    C11Term *term = NULL;
    Buf out = {0};
    int rc = 1;

    if (argc < 5) {
        return usage(argv[0]);
    }
    path = argv[1];
    for (int i = 2; i < argc; i++) {
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
    if (function_name == NULL || (want_term == want_contract)) {
        return usage(argv[0]);
    }
    source = read_file(path);
    if (source == NULL) {
        fprintf(stderr, "failed to read %s\n", path);
        goto done;
    }
    unit = parse_unit(path, source, &index);
    if (unit == NULL) {
        fprintf(stderr, "libclang parse failed for %s\n", path);
        goto done;
    }
    find_ctx.name = function_name;
    (void)clang_visitChildren(clang_getTranslationUnitCursor(unit), find_function_visitor, &find_ctx);
    if (!find_ctx.found) {
        fprintf(stderr, "function not found: %s\n", function_name);
        goto done;
    }
    body = function_body(find_ctx.cursor);
    if (clang_Cursor_isNull(body)) {
        fprintf(stderr, "function has no body: %s\n", function_name);
        goto done;
    }
    term = lift_compound_stmt(body);
    if (term == NULL) {
        goto done;
    }
    if (want_term) {
        if (emit_term_memento(&out, path, term) != 0) {
            goto done;
        }
    } else if (emit_projected_contract(&out, find_ctx.cursor, term) != 0) {
        goto done;
    }
    fputs(out.data == NULL ? "" : out.data, stdout);
    rc = 0;

done:
    buf_free(&out);
    term_free(term);
    if (unit != NULL) clang_disposeTranslationUnit(unit);
    if (index != NULL) clang_disposeIndex(index);
    free(source);
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
