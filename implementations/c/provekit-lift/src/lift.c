/* SPDX-License-Identifier: Apache-2.0
 *
 * provekit-lift-c — C kit lifter via libclang.
 * 
 * ISSUE #380: C kit via libclang — Linux empirical case (paper 07 §8)
 */

#include "provekit/libclang_wrapper.h"
#include "provekit/lift.h"

#include <errno.h>
#include <string.h>

/* ----------------------------------------------------------------------- */
/* State                                                                 */
/* ----------------------------------------------------------------------- */

static int g_kernel_mode = 0;
static char g_error[256] = {0};

void pk_enable_kernel_annotations(int enabled) { g_kernel_mode = enabled; }
const char *pk_last_error(void) { return g_error[0] ? g_error : NULL; }

static void set_error(const char *msg) {
    strncpy(g_error, msg, sizeof(g_error) - 1);
}

/* ----------------------------------------------------------------------- */
/* libclang AST parsing                                               */
/* ----------------------------------------------------------------------- */

typedef struct {
    CXString name;
    unsigned line;
} FuncInfo;

typedef struct {
    char callee[256];
    unsigned line;
} CallInfo;

static FuncInfo g_funcs[256];
static int g_n_funcs = 0;
static CallInfo g_calls[1024];
static int g_n_calls = 0;

static enum CXChildVisitResult visit_cursor(CXCursor cursor, CXCursor parent, CXClientData data) {
    (void)parent;
    (void)data;

    enum CXCursorKind kind = clang_getCursorKind(cursor);
    CXSourceLocation loc = clang_getCursorLocation(cursor);
    
    unsigned line, col, offset;
    clang_getExpansionLocation(loc, NULL, &line, &col, &offset);

    if (kind == CXCursor_FunctionDecl) {
        CXString name = clang_getCursorSpelling(cursor);
        if (g_n_funcs < 256) {
            g_funcs[g_n_funcs].name = name;
            g_funcs[g_n_funcs].line = line;
            g_n_funcs++;
        }
    }
    else if (kind == CXCursor_CallExpr) {
        CXString name = clang_getCursorSpelling(cursor);
        const char *s = clang_getCString(name);
        if (s && strlen(s) > 0 && g_n_calls < 1024) {
            strncpy(g_calls[g_n_calls].callee, s, 255);
            g_calls[g_n_calls].callee[255] = '\0';
            g_calls[g_n_calls].line = line;
            g_n_calls++;
        }
        clang_disposeString(name);
    }
    else if (kind == CXCursor_MacroExpansion) {
        CXString name = clang_getCursorSpelling(cursor);
        const char *s = clang_getCString(name);
        if (s && g_kernel_mode && g_n_calls < 1024) {
            if (strncmp(s, "BUG_ON", 5) == 0 || 
                strncmp(s, "WARN_ON", 6) == 0 ||
                strncmp(s, "assert", 6) == 0 ||
                s[0] == '_') {
                strncpy(g_calls[g_n_calls].callee, s, 255);
                g_calls[g_n_calls].callee[255] = '\0';
                g_calls[g_n_calls].line = line;
                g_n_calls++;
            }
        }
        clang_disposeString(name);
    }

    return CXChildVisit_Recurse;
}

/* ----------------------------------------------------------------------- */
/* API                                                                   */
/* ----------------------------------------------------------------------- */

pk_lift_result *pk_lift_file(const char *path) {
    if (!path) { set_error("null path"); return NULL; }

    CXIndex idx = clang_createIndex(0, 0);
    if (!idx) { set_error("clang_createIndex failed"); return NULL; }

    const char *args[] = { "-Wall", "-Wextra", "-fsyntax-only" };
    CXTranslationUnit tu = clang_parseTranslationUnit(
        idx, path, args, 3, NULL, 0, CXTranslationUnit_None);
    
    if (!tu) { 
        set_error("clang_parseTranslationUnit failed");
        clang_disposeIndex(idx);
        return NULL; 
    }

    CXCursor root = clang_getTranslationUnitCursor(tu);
    g_n_funcs = g_n_calls = 0;
    clang_visitChildren(root, visit_cursor, NULL);

    pk_lift_result *r = calloc(1, sizeof(pk_lift_result));
    if (!r) { 
        set_error("out of memory"); 
        clang_disposeTranslationUnit(tu);
        clang_disposeIndex(idx);
        return NULL; 
    }

    /* Build JSON bundle */
    char *bundle = malloc(16384);
    sprintf(bundle, "{\"functions\":[");
    for (int i = 0; i < g_n_funcs; i++) {
        if (i) strcat(bundle, ",");
        const char *n = clang_getCString(g_funcs[i].name);
        sprintf(bundle + strlen(bundle), "{\"name\":\"%s\",\"line\":%u}", n, g_funcs[i].line);
        clang_disposeString(g_funcs[i].name);
    }
    strcat(bundle, "],\"calls\":[");
    for (int i = 0; i < g_n_calls; i++) {
        if (i) strcat(bundle, ",");
        sprintf(bundle + strlen(bundle), "{\"callee\":\"%s\",\"line\":%u}",
            g_calls[i].callee, g_calls[i].line);
    }
    strcat(bundle, "]}");

    r->cid = strdup("c-kit-v0.1.0");
    r->proof_ir_bundle = bundle;

    clang_disposeTranslationUnit(tu);
    clang_disposeIndex(idx);
    return r;
}

pk_lift_result *pk_lift_source(const char *src) {
    if (!src) { set_error("null source"); return NULL; }

    /* Write to temp file */
    const char *tmppath = "/tmp/provekit_lift_input.c";
    FILE *f = fopen(tmppath, "w");
    if (!f) { set_error("failed to write temp file"); return NULL; }
    fwrite(src, 1, strlen(src), f);
    fclose(f);

    return pk_lift_file(tmppath);
}

void pk_lift_result_free(pk_lift_result *r) {
    if (!r) return;
    free(r->cid);
    free(r->proof_ir_bundle);
    free(r);
}

#ifdef PROVEKIT_LIFT_C_MAIN
int main(int argc, char **argv) {
    int kernel = 0;
    const char *path = NULL;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--kernel") == 0) kernel = 1;
        else path = argv[i];
    }
    pk_enable_kernel_annotations(kernel);
    
    pk_lift_result *r = path ? pk_lift_file(path) : pk_lift_source("int main() { return 0; }");
    if (r) {
        printf("%s\n", r->proof_ir_bundle);
        pk_lift_result_free(r);
        return 0;
    }
    fprintf(stderr, "Error: %s\n", pk_last_error() ?: "unknown");
    return 1;
}
#endif