/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"

#ifdef PK_C_ENABLE_CLANG_AST

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    CXCursor items[8];
    unsigned len;
} CursorList;

typedef struct {
    pk_c_walk_formula *pre;
} PreCtx;

typedef struct {
    char *name;
} NameCtx;

typedef struct {
    int exits;
} ExitCtx;

static char *cx_string_copy(CXString string) {
    const char *cstr = clang_getCString(string);
    char *copy = pk_c_walk_copy(cstr == NULL ? "" : cstr);

    clang_disposeString(string);
    return copy;
}

void pk_c_walk_locus(CXCursor cursor, int *line, int *column) {
    CXSourceLocation location = clang_getCursorLocation(cursor);
    unsigned l = 0;
    unsigned c = 0;

    clang_getExpansionLocation(location, NULL, &l, &c, NULL);
    if (line != NULL) *line = (int)l;
    if (column != NULL) *column = (int)c;
}

char *pk_c_walk_cursor_source(CXCursor cursor) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    char *out = NULL;
    size_t len = 0;
    size_t cap = 64;

    clang_tokenize(tu, range, &tokens, &ntoks);
    out = malloc(cap);
    if (out == NULL) {
        if (tokens != NULL) clang_disposeTokens(tu, tokens, ntoks);
        return NULL;
    }
    out[0] = '\0';
    for (unsigned i = 0; i < ntoks; i++) {
        CXString spelling = clang_getTokenSpelling(tu, tokens[i]);
        const char *text = clang_getCString(spelling);
        size_t add = text == NULL ? 0 : strlen(text);

        if (len + add + 1 > cap) {
            size_t next = cap;
            char *data;

            while (next < len + add + 1) {
                if (next > ((size_t)-1) / 2) {
                    clang_disposeString(spelling);
                    clang_disposeTokens(tu, tokens, ntoks);
                    free(out);
                    return NULL;
                }
                next *= 2;
            }
            data = realloc(out, next);
            if (data == NULL) {
                clang_disposeString(spelling);
                clang_disposeTokens(tu, tokens, ntoks);
                free(out);
                return NULL;
            }
            out = data;
            cap = next;
        }
        if (add > 0) {
            memcpy(out + len, text, add);
            len += add;
            out[len] = '\0';
        }
        clang_disposeString(spelling);
    }
    if (tokens != NULL) clang_disposeTokens(tu, tokens, ntoks);
    return out;
}

static enum CXChildVisitResult collect_children(CXCursor cursor, CXCursor parent, CXClientData data) {
    CursorList *list = (CursorList *)data;

    (void)parent;
    if (list->len < sizeof(list->items) / sizeof(list->items[0])) {
        list->items[list->len++] = cursor;
    }
    return CXChildVisit_Continue;
}

static enum CXChildVisitResult find_decl_ref(CXCursor cursor, CXCursor parent, CXClientData data) {
    NameCtx *ctx = (NameCtx *)data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    (void)parent;
    if (ctx->name != NULL) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_DeclRefExpr || kind == CXCursor_MemberRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);

        ctx->name = cx_string_copy(clang_getCursorSpelling(referenced));
        if (ctx->name == NULL || ctx->name[0] == '\0') {
            free(ctx->name);
            ctx->name = cx_string_copy(clang_getCursorSpelling(cursor));
        }
        return CXChildVisit_Break;
    }
    return CXChildVisit_Recurse;
}

char *pk_c_walk_call_callee(CXCursor cursor) {
    CXCursor referenced = clang_getCursorReferenced(cursor);
    char *name = cx_string_copy(clang_getCursorSpelling(referenced));
    NameCtx ctx = {0};

    if (name != NULL && name[0] != '\0') {
        return name;
    }
    free(name);
    (void)clang_visitChildren(cursor, find_decl_ref, &ctx);
    return ctx.name == NULL ? pk_c_walk_copy("") : ctx.name;
}

pk_c_walk_term *pk_c_walk_lift_term(CXCursor cursor) {
    char *source = pk_c_walk_cursor_source(cursor);
    pk_c_walk_term *term;

    if (source == NULL) {
        return NULL;
    }
    term = pk_c_walk_term_from_text(source, strlen(source));
    if (term == NULL) {
        term = pk_c_walk_term_var(source);
    }
    free(source);
    return term;
}

pk_c_walk_formula *pk_c_walk_lift_condition(CXCursor cursor) {
    char *source = pk_c_walk_cursor_source(cursor);
    pk_c_walk_formula *formula;

    if (source == NULL) {
        return NULL;
    }
    formula = pk_c_walk_formula_from_text(source, strlen(source));
    free(source);
    return formula;
}

static enum CXChildVisitResult exits_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    ExitCtx *ctx = (ExitCtx *)data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    (void)parent;
    if (ctx->exits) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_ReturnStmt || kind == CXCursor_GotoStmt || kind == CXCursor_BreakStmt) {
        ctx->exits = 1;
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_CallExpr) {
        char *callee = pk_c_walk_call_callee(cursor);

        if (callee != NULL && (strcmp(callee, "BUG") == 0 || strcmp(callee, "panic") == 0)) {
            ctx->exits = 1;
            free(callee);
            return CXChildVisit_Break;
        }
        free(callee);
    }
    return CXChildVisit_Recurse;
}

static int stmt_exits(CXCursor cursor) {
    ExitCtx ctx = {0};

    if (clang_getCursorKind(cursor) == CXCursor_ReturnStmt ||
        clang_getCursorKind(cursor) == CXCursor_GotoStmt ||
        clang_getCursorKind(cursor) == CXCursor_BreakStmt) {
        return 1;
    }
    (void)clang_visitChildren(cursor, exits_visitor, &ctx);
    return ctx.exits;
}

pk_c_walk_formula *pk_c_walk_lift_if_exit_pre(CXCursor if_cursor) {
    CursorList list = {{0}, 0};
    pk_c_walk_formula *condition;

    if (clang_getCursorKind(if_cursor) != CXCursor_IfStmt) {
        return NULL;
    }
    (void)clang_visitChildren(if_cursor, collect_children, &list);
    if (list.len < 2 || !stmt_exits(list.items[1])) {
        return NULL;
    }
    condition = pk_c_walk_lift_condition(list.items[0]);
    return pk_c_walk_formula_negate_take(condition);
}

static int add_pre(PreCtx *ctx, pk_c_walk_formula *part) {
    if (part == NULL) {
        return 0;
    }
    if (ctx->pre == NULL) {
        ctx->pre = part;
    } else {
        ctx->pre = pk_c_walk_formula_and_take(ctx->pre, part);
    }
    return ctx->pre == NULL ? -1 : 0;
}

static enum CXChildVisitResult pre_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    PreCtx *ctx = (PreCtx *)data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    (void)parent;
    if (kind == CXCursor_IfStmt) {
        if (add_pre(ctx, pk_c_walk_lift_if_exit_pre(cursor)) != 0) {
            return CXChildVisit_Break;
        }
    } else if (kind == CXCursor_CallExpr) {
        char *callee = pk_c_walk_call_callee(cursor);

        if (callee != NULL && strcmp(callee, "assert") == 0 && clang_Cursor_getNumArguments(cursor) == 1) {
            CXCursor arg = clang_Cursor_getArgument(cursor, 0);

            if (add_pre(ctx, pk_c_walk_lift_condition(arg)) != 0) {
                free(callee);
                return CXChildVisit_Break;
            }
        } else if (callee != NULL && strcmp(callee, "BUG_ON") == 0 && clang_Cursor_getNumArguments(cursor) == 1) {
            CXCursor arg = clang_Cursor_getArgument(cursor, 0);

            if (add_pre(ctx, pk_c_walk_formula_negate_take(pk_c_walk_lift_condition(arg))) != 0) {
                free(callee);
                return CXChildVisit_Break;
            }
        }
        free(callee);
    }
    return CXChildVisit_Recurse;
}

pk_c_walk_formula *pk_c_walk_lift_function_pre(CXCursor function_cursor) {
    PreCtx ctx = {0};

    (void)clang_visitChildren(function_cursor, pre_visitor, &ctx);
    if (ctx.pre == NULL) {
        return pk_c_walk_formula_true();
    }
    return ctx.pre;
}

char **pk_c_walk_lift_formals(CXCursor function_cursor, size_t *len_out) {
    int n = clang_Cursor_getNumArguments(function_cursor);
    char **items;

    if (n < 0) {
        n = 0;
    }
    items = calloc((size_t)n, sizeof(*items));
    if (items == NULL && n > 0) {
        return NULL;
    }
    for (int i = 0; i < n; i++) {
        CXCursor arg = clang_Cursor_getArgument(function_cursor, (unsigned)i);

        items[i] = cx_string_copy(clang_getCursorSpelling(arg));
        if (items[i] == NULL || items[i][0] == '\0') {
            char fallback[32];

            free(items[i]);
            (void)snprintf(fallback, sizeof(fallback), "x%d", i);
            items[i] = pk_c_walk_copy(fallback);
        }
        if (items[i] == NULL) {
            pk_c_walk_lift_string_list_free(items, (size_t)i);
            return NULL;
        }
    }
    if (len_out != NULL) {
        *len_out = (size_t)n;
    }
    return items;
}

void pk_c_walk_lift_string_list_free(char **items, size_t len) {
    if (items == NULL) {
        return;
    }
    for (size_t i = 0; i < len; i++) {
        free(items[i]);
    }
    free(items);
}

#endif
