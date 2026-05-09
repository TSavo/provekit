#include "provekit/c_lift_core.h"

#include <clang-c/Index.h>
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    pk_c_source_facts *facts;
    const char *path;
    const char *current_function;
} pk_c_clang_visit_ctx;

typedef struct {
    char *name;
} pk_c_clang_callee_ctx;

static char *pk_c_clang_copy_n(const char *src, size_t len) {
    char *copy = malloc(len + 1);

    if (copy == NULL) {
        return NULL;
    }
    memcpy(copy, src, len);
    copy[len] = '\0';
    return copy;
}

static char *pk_c_clang_copy(const char *src) {
    if (src == NULL) {
        return NULL;
    }
    return pk_c_clang_copy_n(src, strlen(src));
}

static char *pk_c_clang_string(CXString string) {
    const char *cstr = clang_getCString(string);
    char *copy = pk_c_clang_copy(cstr == NULL ? "" : cstr);

    clang_disposeString(string);
    return copy;
}

static int pk_c_clang_checked_mul(size_t lhs, size_t rhs, size_t *out) {
    if (lhs != 0 && rhs > SIZE_MAX / lhs) {
        return -1;
    }
    *out = lhs * rhs;
    return 0;
}

static int pk_c_clang_grow_functions(pk_c_source_facts *facts) {
    pk_c_function_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_functions < facts->cap_functions) {
        return 0;
    }
    cap = facts->cap_functions == 0 ? 4 : facts->cap_functions * 2;
    if (cap < facts->cap_functions ||
        pk_c_clang_checked_mul(cap, sizeof(*facts->functions), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->functions, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->functions = items;
    facts->cap_functions = cap;
    return 0;
}

static int pk_c_clang_grow_macros(pk_c_source_facts *facts) {
    pk_c_macro_call_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_macro_calls < facts->cap_macro_calls) {
        return 0;
    }
    cap = facts->cap_macro_calls == 0 ? 4 : facts->cap_macro_calls * 2;
    if (cap < facts->cap_macro_calls ||
        pk_c_clang_checked_mul(cap, sizeof(*facts->macro_calls), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->macro_calls, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->macro_calls = items;
    facts->cap_macro_calls = cap;
    return 0;
}

static int pk_c_clang_grow_calls(pk_c_source_facts *facts) {
    pk_c_call_site_fact *items;
    size_t cap;
    size_t bytes;

    if (facts->n_call_sites < facts->cap_call_sites) {
        return 0;
    }
    cap = facts->cap_call_sites == 0 ? 4 : facts->cap_call_sites * 2;
    if (cap < facts->cap_call_sites ||
        pk_c_clang_checked_mul(cap, sizeof(*facts->call_sites), &bytes) != 0) {
        return -1;
    }
    items = realloc(facts->call_sites, bytes);
    if (items == NULL) {
        return -1;
    }
    facts->call_sites = items;
    facts->cap_call_sites = cap;
    return 0;
}

static void pk_c_clang_set_locus(pk_c_locus *locus, CXCursor cursor, const char *fallback_path) {
    CXSourceLocation location = clang_getCursorLocation(cursor);
    unsigned line = 0;
    unsigned column = 0;

    clang_getExpansionLocation(location, NULL, &line, &column, NULL);
    locus->path = pk_c_clang_copy(fallback_path == NULL ? "" : fallback_path);
    locus->line = (int)line;
    locus->column = (int)column;
}

static int pk_c_clang_append_function(
    pk_c_source_facts *facts,
    const char *path,
    const char *name,
    CXCursor cursor,
    int has_body
) {
    pk_c_function_fact *fact;

    if (pk_c_clang_grow_functions(facts) != 0) {
        return -1;
    }
    fact = &facts->functions[facts->n_functions];
    memset(fact, 0, sizeof(*fact));
    fact->name = pk_c_clang_copy(name);
    pk_c_clang_set_locus(&fact->locus, cursor, path);
    if (fact->name == NULL || fact->locus.path == NULL) {
        free(fact->name);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    fact->has_body = has_body;
    facts->n_functions++;
    return 0;
}

static int pk_c_clang_append_macro(
    pk_c_source_facts *facts,
    const char *path,
    const char *name,
    const char *caller,
    CXCursor cursor
) {
    pk_c_macro_call_fact *fact;

    if (caller == NULL || caller[0] == '\0') {
        return 0;
    }
    if (pk_c_clang_grow_macros(facts) != 0) {
        return -1;
    }
    fact = &facts->macro_calls[facts->n_macro_calls];
    memset(fact, 0, sizeof(*fact));
    fact->name = pk_c_clang_copy(name);
    fact->enclosing_function = pk_c_clang_copy(caller);
    fact->argument_text = pk_c_clang_copy("");
    pk_c_clang_set_locus(&fact->locus, cursor, path);
    if (fact->name == NULL || fact->enclosing_function == NULL ||
        fact->argument_text == NULL || fact->locus.path == NULL) {
        free(fact->name);
        free(fact->enclosing_function);
        free(fact->argument_text);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    facts->n_macro_calls++;
    return 0;
}

static char *pk_c_clang_get_cursor_source(CXCursor cursor) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    char *out = NULL;
    size_t out_len = 0;
    size_t out_cap = 0;

    clang_tokenize(tu, range, &tokens, &ntoks);
    if (ntoks == 0) {
        if (tokens != NULL) {
            clang_disposeTokens(tu, tokens, ntoks);
        }
        return pk_c_clang_copy("");
    }
    out_cap = 64;
    out = malloc(out_cap);
    if (out == NULL) {
        clang_disposeTokens(tu, tokens, ntoks);
        return NULL;
    }
    out[0] = '\0';
    for (unsigned i = 0; i < ntoks; i++) {
        CXString sp = clang_getTokenSpelling(tu, tokens[i]);
        const char *cstr = clang_getCString(sp);
        size_t add = cstr ? strlen(cstr) : 0;

        if (add > 0) {
            if (out_len + add + 1 > out_cap) {
                size_t new_cap = out_cap;
                char *new_out;

                while (new_cap < out_len + add + 1) {
                    if (new_cap > ((size_t)-1) / 2) {
                        clang_disposeString(sp);
                        clang_disposeTokens(tu, tokens, ntoks);
                        free(out);
                        return NULL;
                    }
                    new_cap *= 2;
                }
                new_out = realloc(out, new_cap);
                if (new_out == NULL) {
                    clang_disposeString(sp);
                    clang_disposeTokens(tu, tokens, ntoks);
                    free(out);
                    return NULL;
                }
                out = new_out;
                out_cap = new_cap;
            }
            memcpy(out + out_len, cstr, add);
            out_len += add;
            out[out_len] = '\0';
        }
        clang_disposeString(sp);
    }
    clang_disposeTokens(tu, tokens, ntoks);
    return out;
}

static const char *pk_c_clang_classify_arg_text(const char *text) {
    const char *p;

    if (text == NULL || text[0] == '\0') {
        return "expr";
    }
    if (text[0] == '\'' || text[0] == '"') {
        return "literal";
    }
    if (isdigit((unsigned char)text[0])) {
        return "literal";
    }
    if (text[0] == '-' && isdigit((unsigned char)text[1])) {
        return "literal";
    }
    for (p = text; *p != '\0'; p++) {
        if (!isalnum((unsigned char)*p) && *p != '_') {
            return "expr";
        }
    }
    return "var";
}

static char *pk_c_clang_extract_args(CXCursor call_cursor) {
    int n = clang_Cursor_getNumArguments(call_cursor);
    char *out;
    size_t out_len;
    size_t out_cap;
    int i;

    if (n < 0) {
        n = 0;
    }
    out_cap = 32;
    out = malloc(out_cap);
    if (out == NULL) {
        return NULL;
    }
    out[0] = '[';
    out[1] = '\0';
    out_len = 1;

    for (i = 0; i < n; i++) {
        CXCursor arg = clang_Cursor_getArgument(call_cursor, (unsigned)i);
        char *text = pk_c_clang_get_cursor_source(arg);
        const char *kind = pk_c_clang_classify_arg_text(text);
        char *escaped = pk_c_lift_json_escape(text == NULL ? "" : text);
        int written;
        char *piece;

        if (escaped == NULL) {
            free(text);
            free(out);
            return NULL;
        }
        written = snprintf(NULL, 0,
            "%s{\"position\":%d,\"kind\":\"%s\",\"text\":\"%s\"}",
            i == 0 ? "" : ",",
            i,
            kind,
            escaped);
        if (written < 0) {
            free(escaped);
            free(text);
            free(out);
            return NULL;
        }
        piece = malloc((size_t)written + 1);
        if (piece == NULL) {
            free(escaped);
            free(text);
            free(out);
            return NULL;
        }
        (void)snprintf(piece, (size_t)written + 1,
            "%s{\"position\":%d,\"kind\":\"%s\",\"text\":\"%s\"}",
            i == 0 ? "" : ",",
            i,
            kind,
            escaped);
        if (out_len + (size_t)written + 2 > out_cap) {
            size_t new_cap = out_cap;
            char *new_out;

            while (new_cap < out_len + (size_t)written + 2) {
                if (new_cap > ((size_t)-1) / 2) {
                    free(piece);
                    free(escaped);
                    free(text);
                    free(out);
                    return NULL;
                }
                new_cap *= 2;
            }
            new_out = realloc(out, new_cap);
            if (new_out == NULL) {
                free(piece);
                free(escaped);
                free(text);
                free(out);
                return NULL;
            }
            out = new_out;
            out_cap = new_cap;
        }
        memcpy(out + out_len, piece, (size_t)written);
        out_len += (size_t)written;
        out[out_len] = '\0';
        free(piece);
        free(escaped);
        free(text);
    }
    if (out_len + 2 > out_cap) {
        char *new_out = realloc(out, out_len + 2);
        if (new_out == NULL) {
            free(out);
            return NULL;
        }
        out = new_out;
    }
    out[out_len++] = ']';
    out[out_len] = '\0';
    return out;
}

static int pk_c_clang_append_call(
    pk_c_source_facts *facts,
    const char *path,
    const char *caller,
    const char *callee,
    CXCursor cursor
) {
    pk_c_call_site_fact *fact;

    if (caller == NULL || caller[0] == '\0' || callee == NULL || callee[0] == '\0') {
        return 0;
    }
    if (pk_c_clang_grow_calls(facts) != 0) {
        return -1;
    }
    fact = &facts->call_sites[facts->n_call_sites];
    memset(fact, 0, sizeof(*fact));
    fact->caller = pk_c_clang_copy(caller);
    fact->callee = pk_c_clang_copy(callee);
    fact->args_json = pk_c_clang_extract_args(cursor);
    pk_c_clang_set_locus(&fact->locus, cursor, path);
    if (fact->caller == NULL || fact->callee == NULL || fact->locus.path == NULL) {
        free(fact->caller);
        free(fact->callee);
        free(fact->locus.path);
        memset(fact, 0, sizeof(*fact));
        return -1;
    }
    facts->n_call_sites++;
    return 0;
}

static char *pk_c_clang_compile_command(const pk_c_parse_options *options) {
    size_t len = strlen("clang");
    char *out;
    char *cursor;

    if (options != NULL && options->compile_command != NULL) {
        return pk_c_clang_copy(options->compile_command);
    }
    if (options == NULL || options->clang_args == NULL || options->n_clang_args == 0) {
        return pk_c_clang_copy("clang -x c -std=c11");
    }
    for (size_t i = 0; i < options->n_clang_args; i++) {
        const char *arg = options->clang_args[i] == NULL ? "" : options->clang_args[i];
        len += 1 + strlen(arg);
    }
    out = malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    cursor = out;
    memcpy(cursor, "clang", strlen("clang"));
    cursor += strlen("clang");
    for (size_t i = 0; i < options->n_clang_args; i++) {
        const char *arg = options->clang_args[i] == NULL ? "" : options->clang_args[i];
        *cursor++ = ' ';
        memcpy(cursor, arg, strlen(arg));
        cursor += strlen(arg);
    }
    *cursor = '\0';
    return out;
}

static int pk_c_clang_location_is_main(CXCursor cursor) {
    return clang_Location_isFromMainFile(clang_getCursorLocation(cursor)) != 0;
}

static enum CXChildVisitResult pk_c_clang_find_callee(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    pk_c_clang_callee_ctx *ctx = (pk_c_clang_callee_ctx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    (void)parent;

    if (ctx->name != NULL) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_DeclRefExpr || kind == CXCursor_MemberRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);

        ctx->name = pk_c_clang_string(clang_getCursorSpelling(referenced));
        if (ctx->name == NULL || ctx->name[0] == '\0') {
            free(ctx->name);
            ctx->name = pk_c_clang_string(clang_getCursorSpelling(cursor));
        }
        return CXChildVisit_Break;
    }
    return CXChildVisit_Recurse;
}

static char *pk_c_clang_call_callee(CXCursor cursor) {
    CXCursor referenced = clang_getCursorReferenced(cursor);
    char *callee = pk_c_clang_string(clang_getCursorSpelling(referenced));
    pk_c_clang_callee_ctx ctx = {0};

    if (callee != NULL && callee[0] != '\0') {
        return callee;
    }
    free(callee);
    (void)clang_visitChildren(cursor, pk_c_clang_find_callee, &ctx);
    return ctx.name == NULL ? pk_c_clang_copy("") : ctx.name;
}

/* Walk children looking for a DeclRefExpr whose referenced cursor is a
 * FunctionDecl. Used to recover the callee name from libclang's
 * RecoveryExpr nodes (CXCursor_UnexposedExpr) that wrap calls whose
 * argument types could not be resolved. */
static enum CXChildVisitResult pk_c_clang_find_recovery_function_ref(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    pk_c_clang_callee_ctx *ctx = (pk_c_clang_callee_ctx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    (void)parent;

    if (ctx->name != NULL) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_DeclRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);

        if (clang_getCursorKind(referenced) == CXCursor_FunctionDecl) {
            ctx->name = pk_c_clang_string(clang_getCursorSpelling(referenced));
            if (ctx->name != NULL && ctx->name[0] != '\0') {
                return CXChildVisit_Break;
            }
            free(ctx->name);
            ctx->name = NULL;
        }
        /* Not a function ref; the recovery wraps something else. */
        return CXChildVisit_Break;
    }
    /* Step through transparent wrappers (ImplicitCastExpr is reported as
     * UnexposedExpr by libclang) until we hit the function reference. */
    if (kind == CXCursor_UnexposedExpr) {
        return CXChildVisit_Recurse;
    }
    /* Anything else as a first child means the cursor is not a recovery-
     * wrapped call; bail out instead of scanning the rest of the children
     * (which would be argument expressions). */
    return CXChildVisit_Break;
}

static enum CXChildVisitResult pk_c_clang_visit(CXCursor cursor, CXCursor parent, CXClientData client_data) {
    pk_c_clang_visit_ctx *ctx = (pk_c_clang_visit_ctx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    enum CXCursorKind parent_kind = clang_getCursorKind(parent);

    if (!pk_c_clang_location_is_main(cursor)) {
        return CXChildVisit_Continue;
    }

    if (kind == CXCursor_FunctionDecl) {
        char *name = pk_c_clang_string(clang_getCursorSpelling(cursor));
        int has_body = clang_isCursorDefinition(cursor) != 0;
        pk_c_clang_visit_ctx child_ctx = *ctx;

        if (name == NULL) {
            return CXChildVisit_Break;
        }
        if (pk_c_clang_append_function(ctx->facts, ctx->path, name, cursor, has_body) != 0) {
            free(name);
            return CXChildVisit_Break;
        }
        if (has_body) {
            child_ctx.current_function = name;
            if (clang_visitChildren(cursor, pk_c_clang_visit, &child_ctx) != 0) {
                free(name);
                return CXChildVisit_Break;
            }
        }
        free(name);
        return CXChildVisit_Continue;
    }

    if (kind == CXCursor_CallExpr) {
        char *callee = pk_c_clang_call_callee(cursor);
        int rc;

        if (callee == NULL) {
            return CXChildVisit_Break;
        }
        rc = pk_c_clang_append_call(ctx->facts, ctx->path, ctx->current_function, callee, cursor);
        free(callee);
        return rc == 0 ? CXChildVisit_Recurse : CXChildVisit_Break;
    }

    if (kind == CXCursor_MacroInstantiation) {
        char *name = pk_c_clang_string(clang_getCursorSpelling(cursor));
        int rc;

        if (name == NULL) {
            return CXChildVisit_Break;
        }
        rc = pk_c_clang_append_macro(ctx->facts, ctx->path, name, ctx->current_function, cursor);
        free(name);
        return rc == 0 ? CXChildVisit_Continue : CXChildVisit_Break;
    }

    /* libclang represents call expressions whose arguments cannot be
     * type-checked (e.g. an arg with `<dependent type>` because a
     * referenced typedef was not declared) as RecoveryExpr nodes
     * surfaced through CXCursor_UnexposedExpr instead of CXCursor_CallExpr.
     * The function reference is preserved as a child DeclRefExpr.
     * Detect that shape and emit a call edge so the cluster predicate
     * does not silently lose calls in real kernel C, where dependent
     * types are common without a built kernel tree on the include path.
     *
     * Skip UnexposedExprs that are immediate children of CallExpr or of
     * other UnexposedExprs we are already processing, since those are the
     * ImplicitCastExpr / FunctionToPointerDecay wrappers the regular
     * CallExpr branch already handles. */
    if (kind == CXCursor_UnexposedExpr &&
        parent_kind != CXCursor_CallExpr &&
        parent_kind != CXCursor_UnexposedExpr) {
        pk_c_clang_callee_ctx fc = {0};

        (void)clang_visitChildren(cursor, pk_c_clang_find_recovery_function_ref, &fc);
        if (fc.name != NULL && fc.name[0] != '\0') {
            int rc = pk_c_clang_append_call(
                ctx->facts, ctx->path, ctx->current_function, fc.name, cursor);

            free(fc.name);
            if (rc != 0) {
                return CXChildVisit_Break;
            }
        } else {
            free(fc.name);
        }
        return CXChildVisit_Recurse;
    }

    return CXChildVisit_Recurse;
}

static pk_c_source_facts *pk_c_clang_empty_facts(
    const char *path,
    const pk_c_parse_options *options
) {
    pk_c_source_facts *facts = calloc(1, sizeof(*facts));

    if (facts == NULL) {
        return NULL;
    }
    facts->parser_backend = pk_c_clang_copy("libclang");
    facts->parser_compile_command = pk_c_clang_compile_command(options);
    if (options != NULL && options->target_triple != NULL) {
        facts->parser_target_triple = pk_c_clang_copy(options->target_triple);
    }
    if (facts->parser_backend == NULL || facts->parser_compile_command == NULL ||
        (options != NULL && options->target_triple != NULL && facts->parser_target_triple == NULL)) {
        pk_c_source_facts_free(facts);
        return NULL;
    }
    (void)path;
    return facts;
}

static pk_c_source_facts *pk_c_parse_source_clang(
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
    pk_c_source_facts *facts = pk_c_clang_empty_facts(filename, options);
    pk_c_clang_visit_ctx ctx;

    if (facts == NULL) {
        return NULL;
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
        facts->extraction_result = pk_c_lift_result_new();
        if (facts->extraction_result != NULL) {
            (void)pk_c_lift_result_add_opacity_entry(
                facts->extraction_result,
                "ast-index-unavailable",
                filename,
                1,
                1,
                "libclang could not create an index",
                "c-core");
        }
        return facts;
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
        facts->extraction_result = pk_c_lift_result_new();
        if (facts->extraction_result != NULL) {
            (void)pk_c_lift_result_add_opacity_entry(
                facts->extraction_result,
                "ast-parse-failed",
                filename,
                1,
                1,
                "libclang could not build a translation unit",
                "c-core");
        }
        clang_disposeIndex(index);
        return facts;
    }

    ctx.facts = facts;
    ctx.path = filename;
    ctx.current_function = NULL;
    if (clang_visitChildren(clang_getTranslationUnitCursor(unit), pk_c_clang_visit, &ctx) != 0) {
        if (facts->extraction_result == NULL) {
            facts->extraction_result = pk_c_lift_result_new();
        }
        if (facts->extraction_result != NULL) {
            (void)pk_c_lift_result_add_opacity_entry(
                facts->extraction_result,
                "ast-walk-aborted",
                filename,
                1,
                1,
                "libclang AST walk stopped before all facts were extracted",
                "c-core");
        }
    }

    clang_disposeTranslationUnit(unit);
    clang_disposeIndex(index);
    (void)pk_c_emit_call_edges(facts);
    return facts;
}

pk_c_source_facts *pk_c_parse_source_with_options(
    const char *path,
    const char *source,
    const pk_c_parse_options *options
) {
    if (options == NULL ||
        options->backend == PK_C_PARSE_BACKEND_AUTO ||
        options->backend == PK_C_PARSE_BACKEND_REGEX) {
        return pk_c_parse_source(path, source);
    }
    if (options->backend == PK_C_PARSE_BACKEND_CLANG_AST) {
        return pk_c_parse_source_clang(path, source, options);
    }
    return pk_c_parse_source(path, source);
}
