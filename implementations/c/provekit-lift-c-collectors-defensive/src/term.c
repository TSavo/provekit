/* SPDX-License-Identifier: Apache-2.0 */

#include "term.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef PK_C_ENABLE_CLANG_AST
#include <clang-c/Index.h>
#include <errno.h>
#endif

static const char *const C11_SIGNATURE_CID =
    "blake3-512:c942ba70e4b701e139a46590116f5cdc16ab41db277e80e54c01f23e4a7cf6241d4431c60473409cb3f0b61ce27f593071c1c224291f04c61aa04b4773764945";

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} TermBuf;

static int term_buf_init(TermBuf *b) {
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

static void term_buf_free(TermBuf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int term_buf_grow(TermBuf *b, size_t need) {
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

static int term_buf_append_n(TermBuf *b, const char *s, size_t n) {
    if (term_buf_grow(b, n) != 0) {
        return -1;
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int term_buf_append(TermBuf *b, const char *s) {
    return term_buf_append_n(b, s, strlen(s));
}

static int term_buf_append_char(TermBuf *b, char c) {
    return term_buf_append_n(b, &c, 1);
}

static int term_buf_append_json_escaped(TermBuf *b, const char *src) {
    const unsigned char *p = (const unsigned char *)src;

    for (; p != NULL && *p != '\0'; p++) {
        switch (*p) {
        case '"':
            if (term_buf_append(b, "\\\"") != 0) return -1;
            break;
        case '\\':
            if (term_buf_append(b, "\\\\") != 0) return -1;
            break;
        case '\n':
            if (term_buf_append(b, "\\n") != 0) return -1;
            break;
        case '\r':
            if (term_buf_append(b, "\\r") != 0) return -1;
            break;
        case '\t':
            if (term_buf_append(b, "\\t") != 0) return -1;
            break;
        default:
            if (*p < 0x20) {
                char esc[7];
                (void)snprintf(esc, sizeof(esc), "\\u00%02x", *p);
                if (term_buf_append(b, esc) != 0) return -1;
            } else {
                if (term_buf_append_char(b, (char)*p) != 0) return -1;
            }
            break;
        }
    }
    return 0;
}

static int term_buf_append_quoted(TermBuf *b, const char *s) {
    if (term_buf_append_char(b, '"') != 0) return -1;
    if (term_buf_append_json_escaped(b, s == NULL ? "" : s) != 0) return -1;
    return term_buf_append_char(b, '"');
}

static char *term_strdup(const char *s) {
    size_t n;
    char *out;

    if (s == NULL) {
        s = "";
    }
    n = strlen(s);
    out = malloc(n + 1);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, s, n + 1);
    return out;
}

typedef enum {
    TERM_OP = 0,
    TERM_VAR = 1,
    TERM_CONST_INT = 2,
    TERM_CONST_BOOL = 3,
    TERM_CONST_STRING = 4,
    TERM_UNIT = 5,
    TERM_CTOR = 6
} TermKind;

typedef struct C11Term {
    TermKind kind;
    char *name;
    char *sort_name;
    long long int_value;
    int bool_value;
    struct C11Term **args;
    size_t len;
    size_t cap;
} C11Term;

static void term_free(C11Term *term) {
    if (term == NULL) {
        return;
    }
    for (size_t i = 0; i < term->len; i++) {
        term_free(term->args[i]);
    }
    free(term->args);
    free(term->name);
    free(term->sort_name);
    free(term);
}

static C11Term *term_new(TermKind kind) {
    C11Term *term = calloc(1, sizeof(*term));
    if (term != NULL) {
        term->kind = kind;
    }
    return term;
}

static C11Term *term_named(TermKind kind, const char *name) {
    C11Term *term = term_new(kind);

    if (term == NULL) {
        return NULL;
    }
    term->name = term_strdup(name);
    if (term->name == NULL) {
        term_free(term);
        return NULL;
    }
    return term;
}

static C11Term *term_op(const char *name) {
    return term_named(TERM_OP, name);
}

static C11Term *term_var(const char *name) {
    return term_named(TERM_VAR, name);
}

static C11Term *term_unit(void) {
    return term_new(TERM_UNIT);
}

static C11Term *term_const_int(long long value) {
    C11Term *term = term_new(TERM_CONST_INT);

    if (term != NULL) {
        term->int_value = value;
    }
    return term;
}

static C11Term *term_const_string(const char *value, const char *sort_name) {
    C11Term *term = term_named(TERM_CONST_STRING, value);

    if (term == NULL) {
        return NULL;
    }
    term->sort_name = term_strdup(sort_name);
    if (term->sort_name == NULL) {
        term_free(term);
        return NULL;
    }
    return term;
}

static int term_add_arg(C11Term *term, C11Term *arg) {
    C11Term **items;
    size_t cap;

    if (term->len == term->cap) {
        cap = term->cap == 0 ? 2 : term->cap * 2;
        if (cap < term->cap) {
            return -1;
        }
        items = realloc(term->args, cap * sizeof(*term->args));
        if (items == NULL) {
            return -1;
        }
        term->args = items;
        term->cap = cap;
    }
    term->args[term->len++] = arg;
    return 0;
}

static C11Term *term_op1(const char *name, C11Term *arg0) {
    C11Term *term = term_op(name);

    if (term == NULL) {
        term_free(arg0);
        return NULL;
    }
    if (term_add_arg(term, arg0) != 0) {
        term_free(term);
        term_free(arg0);
        return NULL;
    }
    return term;
}

static C11Term *term_op2(const char *name, C11Term *arg0, C11Term *arg1) {
    C11Term *term = term_op(name);

    if (term == NULL) {
        term_free(arg0);
        term_free(arg1);
        return NULL;
    }
    if (term_add_arg(term, arg0) != 0) {
        term_free(term);
        term_free(arg0);
        term_free(arg1);
        return NULL;
    }
    if (term_add_arg(term, arg1) != 0) {
        term_free(term);
        term_free(arg1);
        return NULL;
    }
    return term;
}

static C11Term *term_op3(
    const char *name,
    C11Term *arg0,
    C11Term *arg1,
    C11Term *arg2
) {
    C11Term *term = term_op(name);

    if (term == NULL) {
        term_free(arg0);
        term_free(arg1);
        term_free(arg2);
        return NULL;
    }
    if (term_add_arg(term, arg0) != 0) {
        term_free(term);
        term_free(arg0);
        term_free(arg1);
        term_free(arg2);
        return NULL;
    }
    if (term_add_arg(term, arg1) != 0) {
        term_free(term);
        term_free(arg1);
        term_free(arg2);
        return NULL;
    }
    if (term_add_arg(term, arg2) != 0) {
        term_free(term);
        term_free(arg2);
        return NULL;
    }
    return term;
}

static C11Term *term_skip(void) {
    return term_op1("skip", term_unit());
}

static int term_append_sort(TermBuf *b, const char *sort_name) {
    if (term_buf_append(b, "\"sort\":{\"kind\":\"ctor\",\"name\":") != 0) return -1;
    if (term_buf_append_quoted(b, sort_name) != 0) return -1;
    return term_buf_append(b, ",\"args\":[]}");
}

static int term_append_json(TermBuf *b, const C11Term *term) {
    if (term == NULL) {
        return -1;
    }
    switch (term->kind) {
    case TERM_OP:
        if (term_buf_append(b, "{\"kind\":\"op\",\"name\":") != 0) return -1;
        if (term_buf_append_quoted(b, term->name) != 0) return -1;
        if (term_buf_append(b, ",\"args\":[") != 0) return -1;
        for (size_t i = 0; i < term->len; i++) {
            if (i > 0 && term_buf_append_char(b, ',') != 0) return -1;
            if (term_append_json(b, term->args[i]) != 0) return -1;
        }
        return term_buf_append(b, "]}");
    case TERM_VAR:
        if (term_buf_append(b, "{\"kind\":\"var\",\"name\":") != 0) return -1;
        if (term_buf_append_quoted(b, term->name) != 0) return -1;
        return term_buf_append_char(b, '}');
    case TERM_CONST_INT: {
        char value[64];

        (void)snprintf(value, sizeof(value), "%lld", term->int_value);
        if (term_buf_append(b, "{\"kind\":\"const\",\"value\":") != 0) return -1;
        if (term_buf_append(b, value) != 0) return -1;
        if (term_buf_append_char(b, ',') != 0) return -1;
        if (term_append_sort(b, "Int") != 0) return -1;
        return term_buf_append_char(b, '}');
    }
    case TERM_CONST_BOOL:
        if (term_buf_append(b, "{\"kind\":\"const\",\"value\":") != 0) return -1;
        if (term_buf_append(b, term->bool_value ? "true" : "false") != 0) return -1;
        if (term_buf_append_char(b, ',') != 0) return -1;
        if (term_append_sort(b, "Bool") != 0) return -1;
        return term_buf_append_char(b, '}');
    case TERM_CONST_STRING:
        if (term_buf_append(b, "{\"kind\":\"const\",\"value\":") != 0) return -1;
        if (term_buf_append_quoted(b, term->name) != 0) return -1;
        if (term_buf_append_char(b, ',') != 0) return -1;
        if (term_append_sort(b, term->sort_name == NULL ? "Value" : term->sort_name) != 0) {
            return -1;
        }
        return term_buf_append_char(b, '}');
    case TERM_UNIT:
        return term_buf_append(b, "{\"kind\":\"unit\"}");
    case TERM_CTOR:
        if (term_buf_append(b, "{\"kind\":\"ctor\",\"name\":") != 0) return -1;
        if (term_buf_append_quoted(b, term->name) != 0) return -1;
        if (term_buf_append(b, ",\"args\":[") != 0) return -1;
        for (size_t i = 0; i < term->len; i++) {
            if (i > 0 && term_buf_append_char(b, ',') != 0) return -1;
            if (term_append_json(b, term->args[i]) != 0) return -1;
        }
        return term_buf_append(b, "]}");
    }
    return -1;
}

static int term_append_surface(TermBuf *b, const C11Term *term) {
    if (term == NULL) {
        return -1;
    }
    switch (term->kind) {
    case TERM_OP:
        if (strcmp(term->name, "skip") == 0 &&
            term->len == 1 &&
            term->args[0]->kind == TERM_UNIT) {
            return term_buf_append(b, "skip");
        }
        if (term_buf_append(b, term->name) != 0) return -1;
        if (term_buf_append_char(b, '(') != 0) return -1;
        for (size_t i = 0; i < term->len; i++) {
            if (i > 0 && term_buf_append(b, ", ") != 0) return -1;
            if (term_append_surface(b, term->args[i]) != 0) return -1;
        }
        return term_buf_append_char(b, ')');
    case TERM_VAR:
        return term_buf_append(b, term->name);
    case TERM_CONST_INT: {
        char value[64];

        (void)snprintf(value, sizeof(value), "%lld", term->int_value);
        return term_buf_append(b, value);
    }
    case TERM_CONST_BOOL:
        return term_buf_append(b, term->bool_value ? "true" : "false");
    case TERM_CONST_STRING:
        return term_buf_append(b, term->name);
    case TERM_UNIT:
        return term_buf_append(b, "unit");
    case TERM_CTOR:
        if (term_buf_append(b, term->name) != 0) return -1;
        if (term_buf_append_char(b, '(') != 0) return -1;
        for (size_t i = 0; i < term->len; i++) {
            if (i > 0 && term_buf_append(b, ", ") != 0) return -1;
            if (term_append_surface(b, term->args[i]) != 0) return -1;
        }
        return term_buf_append_char(b, ')');
    }
    return -1;
}

static const char *source_basename(const char *path) {
    const char *slash;

    if (path == NULL || path[0] == '\0') {
        return "source.c";
    }
    slash = strrchr(path, '/');
    return slash == NULL ? path : slash + 1;
}

static int append_term_memento(
    TermBuf *b,
    const char *fn_name,
    const char *source,
    const C11Term *term
) {
    TermBuf surface;
    int rc = -1;

    if (term_buf_init(&surface) != 0) {
        return -1;
    }
    if (term_append_surface(&surface, term) != 0) {
        goto done;
    }

    if (term_buf_append(b, "{\"kind\":\"c11-algebra-term\",") != 0) goto done;
    if (term_buf_append(b, "\"signature_cid\":") != 0) goto done;
    if (term_buf_append_quoted(b, C11_SIGNATURE_CID) != 0) goto done;
    if (term_buf_append(b, ",\"source\":") != 0) goto done;
    if (term_buf_append_quoted(b, source_basename(source)) != 0) goto done;
    if (term_buf_append(b, ",\"fn_name\":") != 0) goto done;
    if (term_buf_append_quoted(b, fn_name) != 0) goto done;
    if (term_buf_append(b, ",\"term_surface\":") != 0) goto done;
    if (term_buf_append_quoted(b, surface.data) != 0) goto done;
    if (term_buf_append(b, ",\"term\":") != 0) goto done;
    if (term_append_json(b, term) != 0) goto done;
    if (term_buf_append_char(b, '}') != 0) goto done;
    rc = 0;

done:
    term_buf_free(&surface);
    return rc;
}

#ifndef PK_C_ENABLE_CLANG_AST

int pk_c_walker_emit_c11_terms(
    pk_c_lift_result *result,
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    (void)result;
    (void)path;
    (void)source;
    (void)options;
    return 0;
}

#else

typedef struct {
    int oom;
    char unsupported[128];
} LowerError;

typedef struct {
    CXCursor *items;
    size_t len;
    size_t cap;
    int oom;
} CursorVec;

typedef struct {
    pk_c_lift_result *result;
    const char *path;
    int oom;
} TermEmitCtx;

typedef struct {
    char *name;
} NameCtx;

static char *cx_string_copy(CXString s) {
    const char *cstr = clang_getCString(s);
    char *copy = term_strdup(cstr == NULL ? "" : cstr);

    clang_disposeString(s);
    return copy;
}

static void lower_set_oom(LowerError *err) {
    if (err != NULL) {
        err->oom = 1;
    }
}

static void lower_set_unsupported_name(LowerError *err, const char *name) {
    if (err == NULL || err->oom || err->unsupported[0] != '\0') {
        return;
    }
    (void)snprintf(err->unsupported, sizeof(err->unsupported), "%s", name);
}

static void lower_set_unsupported_cursor(LowerError *err, CXCursor cursor) {
    char *name;

    if (err == NULL || err->oom || err->unsupported[0] != '\0') {
        return;
    }
    name = cx_string_copy(clang_getCursorKindSpelling(clang_getCursorKind(cursor)));
    if (name == NULL || name[0] == '\0') {
        free(name);
        lower_set_unsupported_name(err, "Unknown");
        return;
    }
    lower_set_unsupported_name(err, name);
    free(name);
}

static enum CXChildVisitResult collect_child_cursor(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    CursorVec *vec = (CursorVec *)client_data;
    CXCursor *items;
    size_t cap;

    (void)parent;
    if (vec->oom) {
        return CXChildVisit_Break;
    }
    if (vec->len == vec->cap) {
        cap = vec->cap == 0 ? 4 : vec->cap * 2;
        if (cap < vec->cap) {
            vec->oom = 1;
            return CXChildVisit_Break;
        }
        items = realloc(vec->items, cap * sizeof(*vec->items));
        if (items == NULL) {
            vec->oom = 1;
            return CXChildVisit_Break;
        }
        vec->items = items;
        vec->cap = cap;
    }
    vec->items[vec->len++] = cursor;
    return CXChildVisit_Continue;
}

static int cursor_children(CXCursor cursor, CursorVec *vec, LowerError *err) {
    memset(vec, 0, sizeof(*vec));
    (void)clang_visitChildren(cursor, collect_child_cursor, vec);
    if (vec->oom) {
        lower_set_oom(err);
        free(vec->items);
        memset(vec, 0, sizeof(*vec));
        return -1;
    }
    return 0;
}

static void cursor_vec_free(CursorVec *vec) {
    free(vec->items);
    memset(vec, 0, sizeof(*vec));
}

static int get_nth_child(CXCursor parent, int n, CXCursor *out) {
    CursorVec vec;
    LowerError err = {0};

    if (cursor_children(parent, &vec, &err) != 0) {
        return 0;
    }
    if (n >= 0 && (size_t)n < vec.len && out != NULL) {
        *out = vec.items[n];
        cursor_vec_free(&vec);
        return 1;
    }
    cursor_vec_free(&vec);
    return 0;
}

static char *cursor_source_text(CXCursor cursor) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    TermBuf b;
    char *out;

    if (term_buf_init(&b) != 0) {
        return NULL;
    }
    clang_tokenize(tu, range, &tokens, &ntoks);
    for (unsigned i = 0; i < ntoks; i++) {
        char *spelling = cx_string_copy(clang_getTokenSpelling(tu, tokens[i]));

        if (spelling == NULL) {
            term_buf_free(&b);
            if (tokens != NULL) {
                clang_disposeTokens(tu, tokens, ntoks);
            }
            return NULL;
        }
        if (term_buf_append(&b, spelling) != 0) {
            free(spelling);
            term_buf_free(&b);
            if (tokens != NULL) {
                clang_disposeTokens(tu, tokens, ntoks);
            }
            return NULL;
        }
        free(spelling);
    }
    if (tokens != NULL) {
        clang_disposeTokens(tu, tokens, ntoks);
    }
    out = b.data;
    b.data = NULL;
    term_buf_free(&b);
    return out;
}

static char *operator_token(CXCursor cursor, int binary) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    char *out = NULL;
    unsigned skip_through_offset = 0;
    int have_skip = 0;

    if (binary) {
        CXCursor first;

        if (get_nth_child(cursor, 0, &first)) {
            CXSourceRange first_range = clang_getCursorExtent(first);
            CXSourceLocation end_loc = clang_getRangeEnd(first_range);

            clang_getFileLocation(end_loc, NULL, NULL, NULL, &skip_through_offset);
            have_skip = 1;
        }
    }

    clang_tokenize(tu, range, &tokens, &ntoks);
    for (unsigned i = 0; i < ntoks; i++) {
        CXString spelling;
        const char *cstr;

        if (clang_getTokenKind(tokens[i]) != CXToken_Punctuation) {
            continue;
        }
        if (have_skip) {
            CXSourceLocation tok_loc = clang_getTokenLocation(tu, tokens[i]);
            unsigned tok_offset = 0;

            clang_getFileLocation(tok_loc, NULL, NULL, NULL, &tok_offset);
            if (tok_offset < skip_through_offset) {
                continue;
            }
        }
        spelling = clang_getTokenSpelling(tu, tokens[i]);
        cstr = clang_getCString(spelling);
        if (cstr != NULL) {
            out = term_strdup(cstr);
        }
        clang_disposeString(spelling);
        break;
    }
    if (tokens != NULL) {
        clang_disposeTokens(tu, tokens, ntoks);
    }
    return out;
}

static int token_contains_arrow(CXCursor cursor) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    int found = 0;

    clang_tokenize(tu, range, &tokens, &ntoks);
    for (unsigned i = 0; i < ntoks; i++) {
        CXString spelling = clang_getTokenSpelling(tu, tokens[i]);
        const char *cstr = clang_getCString(spelling);

        if (cstr != NULL && strcmp(cstr, "->") == 0) {
            found = 1;
            clang_disposeString(spelling);
            break;
        }
        clang_disposeString(spelling);
    }
    if (tokens != NULL) {
        clang_disposeTokens(tu, tokens, ntoks);
    }
    return found;
}

static C11Term *lower_stmt(CXCursor cursor, LowerError *err);
static C11Term *lower_expr(CXCursor cursor, LowerError *err);
static C11Term *lower_lvalue(CXCursor cursor, LowerError *err);

static C11Term *lower_transparent_expr(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *term = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 1) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    term = lower_expr(children.items[0], err);
    cursor_vec_free(&children);
    return term;
}

static C11Term *lower_transparent_lvalue(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *term = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 1) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    term = lower_lvalue(children.items[0], err);
    cursor_vec_free(&children);
    return term;
}

static C11Term *lower_compound(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *acc = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    for (size_t i = 0; i < children.len; i++) {
        C11Term *stmt = lower_stmt(children.items[i], err);
        C11Term *seq;

        if (stmt == NULL) {
            term_free(acc);
            cursor_vec_free(&children);
            return NULL;
        }
        if (acc == NULL) {
            acc = stmt;
            continue;
        }
        seq = term_op2("seq", acc, stmt);
        if (seq == NULL) {
            lower_set_oom(err);
            cursor_vec_free(&children);
            return NULL;
        }
        acc = seq;
    }
    cursor_vec_free(&children);
    if (acc == NULL) {
        acc = term_skip();
        if (acc == NULL) {
            lower_set_oom(err);
        }
    }
    return acc;
}

static C11Term *lower_return(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *value = NULL;
    C11Term *ret;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len == 0) {
        value = term_unit();
    } else if (children.len == 1) {
        value = lower_expr(children.items[0], err);
    } else {
        lower_set_unsupported_cursor(err, cursor);
    }
    cursor_vec_free(&children);
    if (value == NULL) {
        if (!err->oom && err->unsupported[0] == '\0') {
            lower_set_oom(err);
        }
        return NULL;
    }
    ret = term_op1("return", value);
    if (ret == NULL) {
        lower_set_oom(err);
    }
    return ret;
}

static C11Term *lower_if(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *cond = NULL;
    C11Term *then_term = NULL;
    C11Term *else_term = NULL;
    C11Term *term = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 2 && children.len != 3) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    cond = lower_expr(children.items[0], err);
    then_term = cond == NULL ? NULL : lower_stmt(children.items[1], err);
    if (then_term != NULL) {
        if (children.len == 3) {
            else_term = lower_stmt(children.items[2], err);
        } else {
            else_term = term_skip();
            if (else_term == NULL) {
                lower_set_oom(err);
            }
        }
    }
    cursor_vec_free(&children);
    if (cond == NULL || then_term == NULL || else_term == NULL) {
        term_free(cond);
        term_free(then_term);
        term_free(else_term);
        return NULL;
    }
    term = term_op3("if", cond, then_term, else_term);
    if (term == NULL) {
        lower_set_oom(err);
    }
    return term;
}

static C11Term *lower_while(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *cond;
    C11Term *body;
    C11Term *term;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 2) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    cond = lower_expr(children.items[0], err);
    body = cond == NULL ? NULL : lower_stmt(children.items[1], err);
    cursor_vec_free(&children);
    if (cond == NULL || body == NULL) {
        term_free(cond);
        term_free(body);
        return NULL;
    }
    term = term_op2("while", cond, body);
    if (term == NULL) {
        lower_set_oom(err);
    }
    return term;
}

static C11Term *lower_for(CXCursor cursor, LowerError *err) {
    CursorVec children;
    C11Term *init = NULL;
    C11Term *cond = NULL;
    C11Term *step = NULL;
    C11Term *body = NULL;
    C11Term *term = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 4) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    init = lower_stmt(children.items[0], err);
    cond = init == NULL ? NULL : lower_expr(children.items[1], err);
    step = cond == NULL ? NULL : lower_stmt(children.items[2], err);
    body = step == NULL ? NULL : lower_stmt(children.items[3], err);
    cursor_vec_free(&children);
    if (init == NULL || cond == NULL || step == NULL || body == NULL) {
        term_free(init);
        term_free(cond);
        term_free(step);
        term_free(body);
        return NULL;
    }
    term = term_op("for");
    if (term == NULL) {
        term_free(init);
        term_free(cond);
        term_free(step);
        term_free(body);
        lower_set_oom(err);
        return NULL;
    }
    if (term_add_arg(term, init) != 0) {
        term_free(term);
        term_free(init);
        term_free(cond);
        term_free(step);
        term_free(body);
        lower_set_oom(err);
        return NULL;
    }
    init = NULL;
    if (term_add_arg(term, cond) != 0) {
        term_free(term);
        term_free(cond);
        term_free(step);
        term_free(body);
        lower_set_oom(err);
        return NULL;
    }
    cond = NULL;
    if (term_add_arg(term, step) != 0) {
        term_free(term);
        term_free(step);
        term_free(body);
        lower_set_oom(err);
        return NULL;
    }
    step = NULL;
    if (term_add_arg(term, body) != 0) {
        term_free(term);
        term_free(body);
        lower_set_oom(err);
        return NULL;
    }
    return term;
}

static C11Term *lower_expr_stmt(CXCursor cursor, LowerError *err) {
    C11Term *term = lower_expr(cursor, err);

    if (term == NULL) {
        return NULL;
    }
    if (term->kind == TERM_OP &&
        (strcmp(term->name, "assign") == 0 ||
            strcmp(term->name, "call") == 0 ||
            strcmp(term->name, "if") == 0)) {
        return term;
    }
    return term;
}

static C11Term *lower_stmt(CXCursor cursor, LowerError *err) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_CompoundStmt:
        return lower_compound(cursor, err);
    case CXCursor_ReturnStmt:
        return lower_return(cursor, err);
    case CXCursor_IfStmt:
        return lower_if(cursor, err);
    case CXCursor_WhileStmt:
        return lower_while(cursor, err);
    case CXCursor_ForStmt:
        return lower_for(cursor, err);
    case CXCursor_BreakStmt: {
        C11Term *term = term_op1("break", term_unit());

        if (term == NULL) lower_set_oom(err);
        return term;
    }
    case CXCursor_ContinueStmt: {
        C11Term *term = term_op1("continue", term_unit());

        if (term == NULL) lower_set_oom(err);
        return term;
    }
    case CXCursor_NullStmt: {
        C11Term *term = term_skip();

        if (term == NULL) lower_set_oom(err);
        return term;
    }
    case CXCursor_BinaryOperator:
    case CXCursor_CallExpr:
    case CXCursor_UnaryOperator:
    case CXCursor_ParenExpr:
    case CXCursor_UnexposedExpr:
        return lower_expr_stmt(cursor, err);
    default:
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
}

static int parse_integer_literal(CXCursor cursor, long long *out) {
    char *text = cursor_source_text(cursor);
    char *end = NULL;
    long long value;

    if (text == NULL) {
        return -1;
    }
    errno = 0;
    value = strtoll(text, &end, 0);
    if (end == text || errno == ERANGE) {
        free(text);
        return -1;
    }
    *out = value;
    free(text);
    return 0;
}

static C11Term *lower_integer(CXCursor cursor, LowerError *err) {
    long long value = 0;
    C11Term *term;

    if (parse_integer_literal(cursor, &value) != 0) {
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
    term = term_const_int(value);
    if (term == NULL) {
        lower_set_oom(err);
    }
    return term;
}

static C11Term *lower_decl_ref(CXCursor cursor, LowerError *err) {
    char *name = cx_string_copy(clang_getCursorSpelling(cursor));
    C11Term *term;

    if (name == NULL || name[0] == '\0') {
        free(name);
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
    term = term_var(name);
    free(name);
    if (term == NULL) {
        lower_set_oom(err);
    }
    return term;
}

static C11Term *lower_member(CXCursor cursor, LowerError *err) {
    CursorVec children;
    char *field = NULL;
    C11Term *base = NULL;
    C11Term *field_term = NULL;
    C11Term *member = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 1) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    base = lower_lvalue(children.items[0], err);
    cursor_vec_free(&children);
    if (base == NULL) {
        return NULL;
    }
    if (token_contains_arrow(cursor)) {
        base = term_op1("deref", base);
        if (base == NULL) {
            lower_set_oom(err);
            return NULL;
        }
    }
    field = cx_string_copy(clang_getCursorSpelling(cursor));
    if (field == NULL || field[0] == '\0') {
        free(field);
        term_free(base);
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
    field_term = term_const_string(field, "FieldName");
    free(field);
    if (field_term == NULL) {
        term_free(base);
        lower_set_oom(err);
        return NULL;
    }
    member = term_op2("member", base, field_term);
    if (member == NULL) {
        lower_set_oom(err);
    }
    return member;
}

static C11Term *lower_unary(CXCursor cursor, LowerError *err) {
    CursorVec children;
    char *op;
    C11Term *operand;
    C11Term *term = NULL;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 1) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    op = operator_token(cursor, 0);
    if (op == NULL) {
        lower_set_oom(err);
        cursor_vec_free(&children);
        return NULL;
    }
    if (strcmp(op, "-") == 0) {
        operand = lower_expr(children.items[0], err);
        term = operand == NULL ? NULL : term_op1("neg", operand);
    } else if (strcmp(op, "!") == 0) {
        operand = lower_expr(children.items[0], err);
        term = operand == NULL ? NULL : term_op1("not", operand);
    } else if (strcmp(op, "*") == 0) {
        operand = lower_expr(children.items[0], err);
        term = operand == NULL ? NULL : term_op1("deref", operand);
    } else {
        lower_set_unsupported_cursor(err, cursor);
    }
    free(op);
    cursor_vec_free(&children);
    if (term == NULL && err->unsupported[0] == '\0' && !err->oom) {
        lower_set_oom(err);
    }
    return term;
}

static const char *binary_op_name(const char *op) {
    if (strcmp(op, "==") == 0) return "eq";
    if (strcmp(op, "<") == 0) return "lt";
    if (strcmp(op, "<=") == 0) return "le";
    if (strcmp(op, "+") == 0) return "add";
    if (strcmp(op, "-") == 0) return "sub";
    if (strcmp(op, "*") == 0) return "mul";
    if (strcmp(op, "&&") == 0) return "and";
    if (strcmp(op, "||") == 0) return "or";
    return NULL;
}

static C11Term *lower_reversed_binary(
    const char *name,
    CXCursor lhs_cursor,
    CXCursor rhs_cursor,
    LowerError *err
) {
    C11Term *lhs = lower_expr(rhs_cursor, err);
    C11Term *rhs = lhs == NULL ? NULL : lower_expr(lhs_cursor, err);
    C11Term *term = NULL;

    if (lhs != NULL && rhs != NULL) {
        term = term_op2(name, lhs, rhs);
        lhs = NULL;
        rhs = NULL;
    }
    term_free(lhs);
    term_free(rhs);
    if (term == NULL && err->unsupported[0] == '\0' && !err->oom) {
        lower_set_oom(err);
    }
    return term;
}

static C11Term *lower_not_equal(
    CXCursor lhs_cursor,
    CXCursor rhs_cursor,
    LowerError *err
) {
    C11Term *lhs = lower_expr(lhs_cursor, err);
    C11Term *rhs = lhs == NULL ? NULL : lower_expr(rhs_cursor, err);
    C11Term *eq = NULL;
    C11Term *term = NULL;

    if (lhs != NULL && rhs != NULL) {
        eq = term_op2("eq", lhs, rhs);
        lhs = NULL;
        rhs = NULL;
    }
    if (eq != NULL) {
        term = term_op1("not", eq);
        eq = NULL;
    }
    term_free(lhs);
    term_free(rhs);
    term_free(eq);
    if (term == NULL && err->unsupported[0] == '\0' && !err->oom) {
        lower_set_oom(err);
    }
    return term;
}

static C11Term *lower_binary(CXCursor cursor, LowerError *err) {
    CursorVec children;
    char *op;
    C11Term *lhs = NULL;
    C11Term *rhs = NULL;
    C11Term *term = NULL;
    const char *op_name;

    if (cursor_children(cursor, &children, err) != 0) {
        return NULL;
    }
    if (children.len != 2) {
        lower_set_unsupported_cursor(err, cursor);
        cursor_vec_free(&children);
        return NULL;
    }
    op = operator_token(cursor, 1);
    if (op == NULL) {
        lower_set_oom(err);
        cursor_vec_free(&children);
        return NULL;
    }

    if (strcmp(op, "=") == 0) {
        lhs = lower_lvalue(children.items[0], err);
        rhs = lhs == NULL ? NULL : lower_expr(children.items[1], err);
        if (lhs != NULL && rhs != NULL) {
            term = term_op2("assign", lhs, rhs);
            lhs = NULL;
            rhs = NULL;
        }
    } else if (strcmp(op, ">") == 0) {
        term = lower_reversed_binary("lt", children.items[0], children.items[1], err);
    } else if (strcmp(op, ">=") == 0) {
        term = lower_reversed_binary("le", children.items[0], children.items[1], err);
    } else if (strcmp(op, "!=") == 0) {
        term = lower_not_equal(children.items[0], children.items[1], err);
    } else {
        op_name = binary_op_name(op);
        if (op_name == NULL) {
            lower_set_unsupported_cursor(err, cursor);
        } else {
            lhs = lower_expr(children.items[0], err);
            rhs = lhs == NULL ? NULL : lower_expr(children.items[1], err);
            if (lhs != NULL && rhs != NULL) {
                term = term_op2(op_name, lhs, rhs);
                lhs = NULL;
                rhs = NULL;
            }
        }
    }

    free(op);
    cursor_vec_free(&children);
    term_free(lhs);
    term_free(rhs);
    if (term == NULL && err->unsupported[0] == '\0' && !err->oom) {
        lower_set_oom(err);
    }
    return term;
}

static enum CXChildVisitResult find_callee_ref(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    NameCtx *ctx = (NameCtx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    (void)parent;
    if (ctx->name != NULL) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_DeclRefExpr || kind == CXCursor_MemberRefExpr) {
        ctx->name = cx_string_copy(clang_getCursorSpelling(cursor));
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_UnexposedExpr || kind == CXCursor_ParenExpr) {
        return CXChildVisit_Recurse;
    }
    return CXChildVisit_Break;
}

static char *call_callee_name(CXCursor cursor) {
    CXCursor referenced = clang_getCursorReferenced(cursor);
    char *name = NULL;
    NameCtx ctx = {0};

    if (clang_Cursor_isNull(referenced) == 0) {
        name = cx_string_copy(clang_getCursorSpelling(referenced));
        if (name != NULL && name[0] != '\0') {
            return name;
        }
        free(name);
    }
    (void)clang_visitChildren(cursor, find_callee_ref, &ctx);
    return ctx.name;
}

static C11Term *lower_call(CXCursor cursor, LowerError *err) {
    char *callee_name = call_callee_name(cursor);
    C11Term *call = NULL;
    C11Term *callee = NULL;
    int n_args;

    if (callee_name == NULL || callee_name[0] == '\0') {
        free(callee_name);
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
    call = term_op("call");
    callee = term_var(callee_name);
    free(callee_name);
    if (call == NULL || callee == NULL || term_add_arg(call, callee) != 0) {
        term_free(call);
        term_free(callee);
        lower_set_oom(err);
        return NULL;
    }
    callee = NULL;

    n_args = clang_Cursor_getNumArguments(cursor);
    if (n_args < 0) {
        lower_set_unsupported_cursor(err, cursor);
        term_free(call);
        return NULL;
    }
    for (int i = 0; i < n_args; i++) {
        CXCursor arg_cursor = clang_Cursor_getArgument(cursor, (unsigned)i);
        C11Term *arg = lower_expr(arg_cursor, err);

        if (arg == NULL) {
            term_free(call);
            return NULL;
        }
        if (term_add_arg(call, arg) != 0) {
            term_free(arg);
            term_free(call);
            lower_set_oom(err);
            return NULL;
        }
    }
    return call;
}

static C11Term *lower_expr(CXCursor cursor, LowerError *err) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
        return lower_transparent_expr(cursor, err);
    case CXCursor_DeclRefExpr:
        return lower_decl_ref(cursor, err);
    case CXCursor_IntegerLiteral:
        return lower_integer(cursor, err);
    case CXCursor_UnaryOperator:
        return lower_unary(cursor, err);
    case CXCursor_BinaryOperator:
        return lower_binary(cursor, err);
    case CXCursor_CallExpr:
        return lower_call(cursor, err);
    case CXCursor_MemberRefExpr:
        return lower_member(cursor, err);
    default:
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
}

static C11Term *lower_lvalue(CXCursor cursor, LowerError *err) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
        return lower_transparent_lvalue(cursor, err);
    case CXCursor_DeclRefExpr:
        return lower_decl_ref(cursor, err);
    case CXCursor_MemberRefExpr:
        return lower_member(cursor, err);
    case CXCursor_UnaryOperator: {
        char *op = operator_token(cursor, 0);
        C11Term *value = NULL;

        if (op == NULL) {
            lower_set_oom(err);
            return NULL;
        }
        if (strcmp(op, "*") != 0) {
            free(op);
            lower_set_unsupported_cursor(err, cursor);
            return NULL;
        }
        free(op);
        value = lower_unary(cursor, err);
        return value;
    }
    default:
        lower_set_unsupported_cursor(err, cursor);
        return NULL;
    }
}

static int add_skip_diagnostic(
    pk_c_lift_result *result,
    const char *fn_name,
    const char *unsupported
) {
    TermBuf b;
    TermBuf msg;
    int rc = -1;

    if (term_buf_init(&b) != 0) {
        return -1;
    }
    if (term_buf_init(&msg) != 0) {
        term_buf_free(&b);
        return -1;
    }
    if (term_buf_append(&msg, "term-emit skipped fn=") != 0) goto done;
    if (term_buf_append(&msg, fn_name == NULL ? "<unknown>" : fn_name) != 0) goto done;
    if (term_buf_append(&msg, ": unsupported AST node ") != 0) goto done;
    if (term_buf_append(&msg, unsupported == NULL || unsupported[0] == '\0' ? "Unknown" : unsupported) != 0) {
        goto done;
    }
    if (term_buf_append(&b, "{\"message\":") != 0) goto done;
    if (term_buf_append_quoted(&b, msg.data) != 0) goto done;
    if (term_buf_append(&b, ",\"severity\":\"warning\"}") != 0) goto done;
    rc = pk_c_lift_result_add_diagnostic(result, b.data);

done:
    term_buf_free(&msg);
    term_buf_free(&b);
    return rc;
}

static int add_parse_diagnostic(pk_c_lift_result *result, const char *message) {
    TermBuf b;
    int rc = -1;

    if (term_buf_init(&b) != 0) {
        return -1;
    }
    if (term_buf_append(&b, "{\"message\":") != 0 ||
        term_buf_append_quoted(&b, message) != 0 ||
        term_buf_append(&b, ",\"severity\":\"warning\"}") != 0) {
        term_buf_free(&b);
        return -1;
    }
    rc = pk_c_lift_result_add_diagnostic(result, b.data);
    term_buf_free(&b);
    return rc;
}

static int find_function_body(CXCursor function, CXCursor *body, LowerError *err) {
    CursorVec children;

    if (cursor_children(function, &children, err) != 0) {
        return -1;
    }
    for (size_t i = 0; i < children.len; i++) {
        if (clang_getCursorKind(children.items[i]) == CXCursor_CompoundStmt) {
            if (body != NULL) {
                *body = children.items[i];
            }
            cursor_vec_free(&children);
            return 0;
        }
    }
    cursor_vec_free(&children);
    lower_set_unsupported_name(err, "FunctionDeclWithoutCompoundStmt");
    return 1;
}

static int emit_function_term(TermEmitCtx *ctx, CXCursor function) {
    LowerError err = {0};
    CXCursor body;
    C11Term *term = NULL;
    TermBuf json;
    char *fn_name = cx_string_copy(clang_getCursorSpelling(function));
    int body_rc;
    int rc = 0;

    if (fn_name == NULL || fn_name[0] == '\0') {
        free(fn_name);
        return 0;
    }
    body_rc = find_function_body(function, &body, &err);
    if (body_rc != 0) {
        if (err.oom) {
            free(fn_name);
            return -1;
        }
        rc = add_skip_diagnostic(ctx->result, fn_name, err.unsupported);
        free(fn_name);
        return rc;
    }

    term = lower_stmt(body, &err);
    if (term == NULL) {
        if (err.oom) {
            free(fn_name);
            return -1;
        }
        rc = add_skip_diagnostic(ctx->result, fn_name, err.unsupported);
        free(fn_name);
        return rc;
    }
    if (term_buf_init(&json) != 0) {
        term_free(term);
        free(fn_name);
        return -1;
    }
    if (append_term_memento(&json, fn_name, ctx->path, term) != 0) {
        term_buf_free(&json);
        term_free(term);
        free(fn_name);
        return -1;
    }
    rc = pk_c_lift_result_add_declaration(ctx->result, json.data);
    term_buf_free(&json);
    term_free(term);
    free(fn_name);
    return rc;
}

static int location_is_main(CXCursor cursor) {
    return clang_Location_isFromMainFile(clang_getCursorLocation(cursor)) != 0;
}

static enum CXChildVisitResult visit_function_for_terms(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    TermEmitCtx *ctx = (TermEmitCtx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    (void)parent;
    if (ctx->oom) {
        return CXChildVisit_Break;
    }
    if (!location_is_main(cursor)) {
        return CXChildVisit_Continue;
    }
    if (kind == CXCursor_FunctionDecl) {
        if (clang_isCursorDefinition(cursor) != 0 &&
            emit_function_term(ctx, cursor) != 0) {
            ctx->oom = 1;
            return CXChildVisit_Break;
        }
        return CXChildVisit_Continue;
    }
    return CXChildVisit_Recurse;
}

int pk_c_walker_emit_c11_terms(
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
    TermEmitCtx ctx;

    if (result == NULL || source == NULL) {
        return 0;
    }
    if (options != NULL && options->clang_args != NULL && options->n_clang_args > 0) {
        args = options->clang_args;
        n_args = (int)options->n_clang_args;
    }

    unsaved.Filename = filename;
    unsaved.Contents = contents;
    unsaved.Length = (unsigned long)strlen(contents);

    index = clang_createIndex(0, 0);
    if (index == NULL) {
        return add_parse_diagnostic(result, "term-emit skipped: libclang could not create an index");
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
        return add_parse_diagnostic(result, "term-emit skipped: libclang could not build a translation unit");
    }

    ctx.result = result;
    ctx.path = filename;
    ctx.oom = 0;
    (void)clang_visitChildren(
        clang_getTranslationUnitCursor(unit),
        visit_function_for_terms,
        &ctx);

    clang_disposeTranslationUnit(unit);
    clang_disposeIndex(index);
    return ctx.oom ? -1 : 0;
}

#endif
