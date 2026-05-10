/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef PK_C_ENABLE_CLANG_AST

typedef struct {
    char *name;
    char **formals;
    size_t n_formals;
    pk_c_walk_formula *pre;
    CXCursor cursor;
    int line;
    int column;
} WalkFunction;

typedef struct {
    WalkFunction *items;
    size_t len;
    size_t cap;
} FunctionList;

typedef struct {
    CXCursor *items;
    size_t len;
    size_t cap;
} CursorVec;

typedef struct {
    FunctionList *functions;
} FunctionVisitCtx;

typedef struct {
    const char *path;
    pk_c_lift_result *result;
    const FunctionList *functions;
    const WalkFunction *caller;
    CXCursor *stmts;
    size_t n_stmts;
    size_t stmt_index;
    int failed;
} CallVisitCtx;

typedef struct {
    pk_c_walk_formula **wp;
    pk_c_walk_chain *chain;
    size_t stmt_index;
    int failed;
} DeclApplyCtx;

static void walk_function_free(WalkFunction *fn) {
    if (fn == NULL) {
        return;
    }
    free(fn->name);
    pk_c_walk_lift_string_list_free(fn->formals, fn->n_formals);
    pk_c_walk_formula_free(fn->pre);
    memset(fn, 0, sizeof(*fn));
}

static void function_list_free(FunctionList *list) {
    if (list == NULL) {
        return;
    }
    for (size_t i = 0; i < list->len; i++) {
        walk_function_free(&list->items[i]);
    }
    free(list->items);
    memset(list, 0, sizeof(*list));
}

static int function_list_append(FunctionList *list, WalkFunction *fn) {
    WalkFunction *items;
    size_t cap;

    if (list->len == list->cap) {
        cap = list->cap == 0 ? 8 : list->cap * 2;
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
    list->items[list->len++] = *fn;
    memset(fn, 0, sizeof(*fn));
    return 0;
}

static void cursor_vec_free(CursorVec *vec) {
    free(vec->items);
    memset(vec, 0, sizeof(*vec));
}

static int cursor_vec_append(CursorVec *vec, CXCursor cursor) {
    CXCursor *items;
    size_t cap;

    if (vec->len == vec->cap) {
        cap = vec->cap == 0 ? 8 : vec->cap * 2;
        if (cap < vec->cap) {
            return -1;
        }
        items = realloc(vec->items, cap * sizeof(*items));
        if (items == NULL) {
            return -1;
        }
        vec->items = items;
        vec->cap = cap;
    }
    vec->items[vec->len++] = cursor;
    return 0;
}

static int cursor_is_main_file(CXCursor cursor) {
    return clang_Location_isFromMainFile(clang_getCursorLocation(cursor)) != 0;
}

static char *cursor_name(CXCursor cursor) {
    CXString spelling = clang_getCursorSpelling(cursor);
    const char *cstr = clang_getCString(spelling);
    char *copy = pk_c_walk_copy(cstr == NULL ? "" : cstr);

    clang_disposeString(spelling);
    return copy;
}

static enum CXChildVisitResult function_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    FunctionVisitCtx *ctx = (FunctionVisitCtx *)data;
    WalkFunction fn = {0};

    (void)parent;
    if (!cursor_is_main_file(cursor)) {
        return CXChildVisit_Continue;
    }
    if (clang_getCursorKind(cursor) != CXCursor_FunctionDecl || clang_isCursorDefinition(cursor) == 0) {
        return CXChildVisit_Recurse;
    }
    fn.name = cursor_name(cursor);
    fn.cursor = cursor;
    pk_c_walk_locus(cursor, &fn.line, &fn.column);
    fn.formals = pk_c_walk_lift_formals(cursor, &fn.n_formals);
    fn.pre = pk_c_walk_lift_function_pre(cursor);
    if (fn.name == NULL || fn.formals == NULL || fn.pre == NULL) {
        walk_function_free(&fn);
        return CXChildVisit_Break;
    }
    if (function_list_append(ctx->functions, &fn) != 0) {
        walk_function_free(&fn);
        return CXChildVisit_Break;
    }
    return CXChildVisit_Continue;
}

static const WalkFunction *find_function(const FunctionList *functions, const char *name) {
    if (name == NULL) {
        return NULL;
    }
    for (size_t i = 0; i < functions->len; i++) {
        if (functions->items[i].name != NULL && strcmp(functions->items[i].name, name) == 0) {
            return &functions->items[i];
        }
    }
    return NULL;
}

static enum CXChildVisitResult body_finder(CXCursor cursor, CXCursor parent, CXClientData data) {
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

    (void)clang_visitChildren(function_cursor, body_finder, &body);
    return body;
}

static enum CXChildVisitResult stmt_collector(CXCursor cursor, CXCursor parent, CXClientData data) {
    CursorVec *vec = (CursorVec *)data;

    (void)parent;
    return cursor_vec_append(vec, cursor) == 0 ? CXChildVisit_Continue : CXChildVisit_Break;
}

static int collect_body_stmts(CXCursor function_cursor, CursorVec *out) {
    CXCursor body = function_body(function_cursor);

    if (clang_Cursor_isNull(body)) {
        return 0;
    }
    return clang_visitChildren(body, stmt_collector, out) == 0 ? 0 : -1;
}

static int clone_formals_into_chain(pk_c_walk_chain *chain, const WalkFunction *caller) {
    chain->formals = calloc(caller->n_formals, sizeof(*chain->formals));
    if (chain->formals == NULL && caller->n_formals > 0) {
        return -1;
    }
    chain->n_formals = caller->n_formals;
    for (size_t i = 0; i < caller->n_formals; i++) {
        chain->formals[i] = pk_c_walk_copy(caller->formals[i]);
        if (chain->formals[i] == NULL) {
            return -1;
        }
    }
    return 0;
}

static enum CXChildVisitResult decl_apply_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    DeclApplyCtx *ctx = (DeclApplyCtx *)data;
    CXCursor init;
    char *name;
    pk_c_walk_term *term;
    pk_c_walk_formula *next;
    int line = 0;
    int column = 0;

    (void)parent;
    if (ctx->failed) {
        return CXChildVisit_Break;
    }
    if (clang_getCursorKind(cursor) != CXCursor_VarDecl) {
        return CXChildVisit_Recurse;
    }
    init = clang_Cursor_getVarDeclInitializer(cursor);
    if (clang_Cursor_isNull(init)) {
        return CXChildVisit_Continue;
    }
    name = cursor_name(cursor);
    term = pk_c_walk_lift_term(init);
    if (name == NULL || term == NULL) {
        free(name);
        pk_c_walk_term_free(term);
        ctx->failed = 1;
        return CXChildVisit_Break;
    }
    next = pk_c_walk_substitute_formula(*ctx->wp, name, term);
    pk_c_walk_term_free(term);
    if (next == NULL) {
        free(name);
        ctx->failed = 1;
        return CXChildVisit_Break;
    }
    pk_c_walk_formula_free(*ctx->wp);
    *ctx->wp = next;
    pk_c_walk_locus(cursor, &line, &column);
    if (pk_c_walk_chain_add_arrival(ctx->chain, "LetBinding", name, ctx->stmt_index, line, column, *ctx->wp) != 0) {
        free(name);
        ctx->failed = 1;
        return CXChildVisit_Break;
    }
    free(name);
    return CXChildVisit_Continue;
}

static int apply_decl_stmt(CXCursor stmt, pk_c_walk_formula **wp, pk_c_walk_chain *chain, size_t stmt_index) {
    DeclApplyCtx ctx = {wp, chain, stmt_index, 0};

    if (clang_getCursorKind(stmt) != CXCursor_DeclStmt) {
        return 0;
    }
    (void)clang_visitChildren(stmt, decl_apply_visitor, &ctx);
    return ctx.failed ? -1 : 0;
}

static int apply_guard_stmt(CXCursor stmt, pk_c_walk_formula **wp) {
    pk_c_walk_formula *guard;

    if (clang_getCursorKind(stmt) != CXCursor_IfStmt) {
        return 0;
    }
    guard = pk_c_walk_lift_if_exit_pre(stmt);
    if (guard == NULL) {
        return 0;
    }
    *wp = pk_c_walk_formula_and_take(*wp, guard);
    return *wp == NULL ? -1 : 0;
}

static int substitute_actuals(
    pk_c_walk_formula **wp,
    const WalkFunction *callee,
    CXCursor call_cursor
) {
    int n_args = clang_Cursor_getNumArguments(call_cursor);

    if (n_args < 0 || (size_t)n_args != callee->n_formals) {
        return 1;
    }
    for (size_t i = 0; i < callee->n_formals; i++) {
        CXCursor arg = clang_Cursor_getArgument(call_cursor, (unsigned)i);
        pk_c_walk_term *term = pk_c_walk_lift_term(arg);
        pk_c_walk_formula *next;

        if (term == NULL) {
            return -1;
        }
        next = pk_c_walk_substitute_formula(*wp, callee->formals[i], term);
        pk_c_walk_term_free(term);
        if (next == NULL) {
            return -1;
        }
        pk_c_walk_formula_free(*wp);
        *wp = next;
    }
    return 0;
}

static int process_call(CallVisitCtx *ctx, CXCursor call_cursor) {
    char *callee_name = pk_c_walk_call_callee(call_cursor);
    const WalkFunction *callee;
    pk_c_walk_formula *wp;
    pk_c_walk_chain chain;
    int line = 0;
    int column = 0;
    int rc;

    if (callee_name == NULL || callee_name[0] == '\0') {
        free(callee_name);
        return 0;
    }
    callee = find_function(ctx->functions, callee_name);
    if (callee == NULL || pk_c_walk_formula_is_true(callee->pre)) {
        free(callee_name);
        return 0;
    }
    wp = pk_c_walk_formula_clone(callee->pre);
    if (wp == NULL) {
        free(callee_name);
        return -1;
    }
    rc = substitute_actuals(&wp, callee, call_cursor);
    if (rc != 0) {
        pk_c_walk_formula_free(wp);
        free(callee_name);
        return rc < 0 ? -1 : 0;
    }
    pk_c_walk_chain_init(&chain);
    chain.caller_name = pk_c_walk_copy(ctx->caller->name);
    chain.callee_name = pk_c_walk_copy(callee_name);
    chain.path = pk_c_walk_copy(ctx->path);
    pk_c_walk_locus(call_cursor, &line, &column);
    chain.line = line;
    chain.column = column;
    if (chain.caller_name == NULL || chain.callee_name == NULL || chain.path == NULL ||
        clone_formals_into_chain(&chain, ctx->caller) != 0 ||
        pk_c_walk_chain_add_arrival(&chain, "Callsite", callee_name, ctx->stmt_index, line, column, wp) != 0) {
        pk_c_walk_chain_free(&chain);
        pk_c_walk_formula_free(wp);
        free(callee_name);
        return -1;
    }
    if (pk_c_walk_apply_call_context(ctx->stmts[ctx->stmt_index], call_cursor, &wp, &chain, ctx->stmt_index) != 0) {
        pk_c_walk_chain_free(&chain);
        pk_c_walk_formula_free(wp);
        free(callee_name);
        return -1;
    }
    for (size_t pos = ctx->stmt_index; pos > 0; pos--) {
        CXCursor prev = ctx->stmts[pos - 1];

        if (apply_decl_stmt(prev, &wp, &chain, pos - 1) != 0 ||
            pk_c_walk_apply_two_armed_if_stmt(prev, &wp, &chain, pos - 1) != 0 ||
            apply_guard_stmt(prev, &wp) != 0) {
            pk_c_walk_chain_free(&chain);
            pk_c_walk_formula_free(wp);
            free(callee_name);
            return -1;
        }
    }
    if (pk_c_walk_chain_add_arrival(
            &chain,
            "FunctionEntry",
            ctx->caller->name,
            ctx->n_stmts,
            ctx->caller->line,
            ctx->caller->column,
            wp) != 0 ||
        pk_c_walk_emit_chain_contract(ctx->result, &chain) != 0) {
        pk_c_walk_chain_free(&chain);
        pk_c_walk_formula_free(wp);
        free(callee_name);
        return -1;
    }
    pk_c_walk_chain_free(&chain);
    pk_c_walk_formula_free(wp);
    free(callee_name);
    return 0;
}

static enum CXChildVisitResult call_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    CallVisitCtx *ctx = (CallVisitCtx *)data;

    (void)parent;
    if (ctx->failed) {
        return CXChildVisit_Break;
    }
    if (clang_getCursorKind(cursor) == CXCursor_CallExpr) {
        if (process_call(ctx, cursor) != 0) {
            ctx->failed = 1;
            return CXChildVisit_Break;
        }
    }
    return CXChildVisit_Recurse;
}

static int walk_one_function(
    const char *path,
    pk_c_lift_result *result,
    const FunctionList *functions,
    const WalkFunction *caller
) {
    CursorVec stmts = {0};
    int rc = 0;

    if (collect_body_stmts(caller->cursor, &stmts) != 0) {
        return -1;
    }
    for (size_t i = 0; i < stmts.len; i++) {
        CallVisitCtx ctx;

        memset(&ctx, 0, sizeof(ctx));
        ctx.path = path;
        ctx.result = result;
        ctx.functions = functions;
        ctx.caller = caller;
        ctx.stmts = stmts.items;
        ctx.n_stmts = stmts.len;
        ctx.stmt_index = i;
        (void)clang_visitChildren(stmts.items[i], call_visitor, &ctx);
        if (ctx.failed) {
            rc = -1;
            break;
        }
    }
    cursor_vec_free(&stmts);
    return rc;
}

static CXTranslationUnit parse_unit(
    const char *path,
    const char *source,
    const pk_c_parse_options *options,
    CXIndex *index_out
) {
    static const char *const default_args[] = {"-x", "c", "-std=c11"};
    const char *filename = path == NULL || path[0] == '\0' ? "source.c" : path;
    const char *contents = source == NULL ? "" : source;
    const char *const *args = default_args;
    int n_args = (int)(sizeof(default_args) / sizeof(default_args[0]));
    struct CXUnsavedFile unsaved;
    CXTranslationUnit unit = NULL;

    if (options != NULL && options->clang_args != NULL && options->n_clang_args > 0) {
        args = options->clang_args;
        n_args = (int)options->n_clang_args;
    }
    unsaved.Filename = filename;
    unsaved.Contents = contents;
    unsaved.Length = (unsigned long)strlen(contents);
    *index_out = clang_createIndex(0, 0);
    if (*index_out == NULL) {
        return NULL;
    }
    if (clang_parseTranslationUnit2(
            *index_out,
            filename,
            args,
            n_args,
            &unsaved,
            1,
            CXTranslationUnit_DetailedPreprocessingRecord |
                CXTranslationUnit_Incomplete |
                CXTranslationUnit_KeepGoing,
            &unit) != CXError_Success) {
        unit = NULL;
    }
    return unit;
}

static int append_core_result(
    pk_c_lift_result *result,
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_source_facts *facts = pk_c_parse_source_with_options(path, source, options);
    int rc = 0;

    if (facts == NULL) {
        return 0;
    }
    if (facts->extraction_result != NULL) {
        rc = pk_c_lift_result_extend(result, facts->extraction_result);
    }
    pk_c_source_facts_free(facts);
    return rc;
}

pk_c_lift_result *pk_c_walk_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_lift_result *result = pk_c_lift_result_new();
    CXIndex index = NULL;
    CXTranslationUnit unit;
    FunctionList functions = {0};
    FunctionVisitCtx visit_ctx = {&functions};

    if (result == NULL) {
        return NULL;
    }
    if (source == NULL) {
        return result;
    }
    if (append_core_result(result, path, source, options) != 0) {
        pk_c_lift_result_free(result);
        return NULL;
    }
    unit = parse_unit(path, source, options, &index);
    if (unit == NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"message\":\"libclang parse failed\",\"severity\":\"error\"}");
        if (index != NULL) clang_disposeIndex(index);
        return result;
    }
    if (clang_visitChildren(clang_getTranslationUnitCursor(unit), function_visitor, &visit_ctx) != 0) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"message\":\"function collection failed\",\"severity\":\"error\"}");
    } else {
        for (size_t i = 0; i < functions.len; i++) {
            if (walk_one_function(path, result, &functions, &functions.items[i]) != 0) {
                pk_c_lift_result_free(result);
                result = NULL;
                break;
            }
        }
    }
    function_list_free(&functions);
    clang_disposeTranslationUnit(unit);
    clang_disposeIndex(index);
    return result;
}

pk_c_lift_result *pk_c_walk_lift_source(const char *path, const char *source) {
    return pk_c_walk_lift_source_with_options(path, source, NULL);
}

#else

pk_c_lift_result *pk_c_walk_lift_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    pk_c_lift_result *result = pk_c_lift_result_new();

    (void)path;
    (void)source;
    (void)options;
    if (result != NULL) {
        (void)pk_c_lift_result_add_diagnostic(
            result,
            "{\"message\":\"provekit-walk-c requires libclang\",\"severity\":\"error\"}");
    }
    return result;
}

pk_c_lift_result *pk_c_walk_lift_source(const char *path, const char *source) {
    return pk_c_walk_lift_source_with_options(path, source, NULL);
}

#endif
