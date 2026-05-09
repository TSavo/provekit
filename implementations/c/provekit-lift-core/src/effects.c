/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Per-function effects extraction for the libclang AST backend, per
 * Contract Composition Protocol (CCP) v1.0.0 section 3.
 *
 * Effect kinds:
 *   - Reads  { target }
 *   - Writes { target }
 *   - Io
 *   - Unsafe
 *   - Panics
 *   - UnresolvedCall { name }
 *
 * Conservative tagging is acceptable; liberal (missing real effects) is
 * unsound. When in doubt this pass tags the effect with target
 * "<unknown>".
 *
 * The walker degrades gracefully on partial trees produced by KeepGoing
 * + Incomplete translation units: any unrecognized cursor shape simply
 * recurses or is ignored. clang_getCursorReferenced may return a null
 * cursor on partial trees; every reference is null-checked.
 */

#include "provekit/c_lift_core.h"

#include <clang-c/Index.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    pk_c_function_fact *fact;
} pk_c_effects_visit_ctx;

/* Forward declarations for helpers that have a circular use between
 * the operator-token resolver and the nth-child grabber. */
static int pk_c_effects_get_nth_child(CXCursor parent, int n, CXCursor *out);

/* Sorted lists of identifier names treated as Io-bearing entry points
 * and as panic / abort sites. Keep small but explicit; over-tagging is
 * sound, under-tagging is not. Sorted so bsearch can be used. */

static const char *const kPkCEffectsIoNames[] = {
    "copy_from_user",
    "copy_to_user",
    "debugfs_create",
    "debugfs_create_dir",
    "debugfs_create_file",
    "debugfs_create_u32",
    "debugfs_remove",
    "dev_dbg",
    "dev_err",
    "dev_info",
    "dev_warn",
    "device_create_file",
    "ioctl",
    "kfree",
    "kmalloc",
    "kvfree",
    "kvmalloc",
    "kzalloc",
    "netlink_broadcast",
    "netlink_kernel_create",
    "netlink_kernel_release",
    "netlink_unicast",
    "pr_alert",
    "pr_crit",
    "pr_debug",
    "pr_emerg",
    "pr_err",
    "pr_info",
    "pr_warn",
    "printk",
    "register_chrdev",
    "register_chrdev_region",
    "sysfs_create_file",
    "sysfs_create_group",
    "sysfs_remove_file",
    "sysfs_remove_group",
    "unregister_chrdev",
    "vfree",
    "vmalloc",
    "vzalloc",
};
static const size_t kPkCEffectsIoNamesLen =
    sizeof(kPkCEffectsIoNames) / sizeof(kPkCEffectsIoNames[0]);

static const char *const kPkCEffectsPanicNames[] = {
    "BUG",
    "BUG_ON",
    "BUG_ON_ONCE",
    "WARN",
    "WARN_ON",
    "WARN_ON_ONCE",
    "abort",
    "assert",
    "panic",
};
static const size_t kPkCEffectsPanicNamesLen =
    sizeof(kPkCEffectsPanicNames) / sizeof(kPkCEffectsPanicNames[0]);

static int pk_c_effects_str_cmp(const void *a, const void *b) {
    return strcmp(*(const char *const *)a, *(const char *const *)b);
}

static int pk_c_effects_name_in_set(
    const char *name,
    const char *const *set,
    size_t n
) {
    if (name == NULL || name[0] == '\0') {
        return 0;
    }
    return bsearch(&name, set, n, sizeof(*set), pk_c_effects_str_cmp) != NULL;
}

/* Conservative prefix tags for printk / dev_* / pr_* / sysfs_* /
 * debugfs_* / netlink_* families that aren't enumerated above. */
static int pk_c_effects_name_has_io_prefix(const char *name) {
    if (name == NULL) {
        return 0;
    }
    if (strncmp(name, "pr_", 3) == 0) {
        return 1;
    }
    if (strncmp(name, "dev_", 4) == 0) {
        return 1;
    }
    if (strncmp(name, "sysfs_", 6) == 0) {
        return 1;
    }
    if (strncmp(name, "debugfs_", 8) == 0) {
        return 1;
    }
    if (strncmp(name, "netlink_", 8) == 0) {
        return 1;
    }
    return 0;
}

static char *pk_c_effects_string(CXString s) {
    const char *cstr = clang_getCString(s);
    char *copy = NULL;

    if (cstr != NULL) {
        size_t n = strlen(cstr);
        copy = malloc(n + 1);
        if (copy != NULL) {
            memcpy(copy, cstr, n + 1);
        }
    }
    clang_disposeString(s);
    return copy;
}

/* Find the operator token of a BinaryOperator / CompoundAssignOperator
 * / UnaryOperator cursor. libclang's BinaryOperatorKind /
 * UnaryOperatorKind accessors are version-gated and not present in
 * older libclang versions, so we tokenize the cursor extent and pick
 * the punctuation token that begins at or after the end of the
 * operator's first child (for binary operators) or at the start of the
 * cursor (for unary prefix) / end (for unary postfix). For unary
 * operators we fall back to "first punct" since the operator spelling
 * is unambiguous within the unary cursor extent.
 *
 * `mode_binary` non-zero: the operator sits between the first and
 * second children. `mode_binary` zero: the operator sits at one of the
 * cursor's edges (unary).
 */
static char *pk_c_effects_operator_token(CXCursor cursor, int mode_binary) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    char *out = NULL;
    unsigned skip_through_offset = 0;
    int have_skip = 0;

    if (mode_binary) {
        /* Find the end offset of the first child cursor; the operator
         * sits in the gap between that offset and the second child. */
        CXCursor first;

        if (pk_c_effects_get_nth_child(cursor, 0, &first)) {
            CXSourceRange first_range = clang_getCursorExtent(first);
            CXSourceLocation end_loc = clang_getRangeEnd(first_range);

            clang_getFileLocation(end_loc, NULL, NULL, NULL, &skip_through_offset);
            have_skip = 1;
        }
    }

    clang_tokenize(tu, range, &tokens, &ntoks);
    for (unsigned i = 0; i < ntoks; i++) {
        if (clang_getTokenKind(tokens[i]) != CXToken_Punctuation) {
            continue;
        }
        if (have_skip) {
            CXSourceLocation tok_loc =
                clang_getTokenLocation(tu, tokens[i]);
            unsigned tok_offset = 0;

            clang_getFileLocation(tok_loc, NULL, NULL, NULL, &tok_offset);
            if (tok_offset < skip_through_offset) {
                continue;
            }
        }
        out = pk_c_effects_string(clang_getTokenSpelling(tu, tokens[i]));
        break;
    }
    if (tokens != NULL) {
        clang_disposeTokens(tu, tokens, ntoks);
    }
    return out;
}

static int pk_c_effects_token_is_assign(const char *tok) {
    size_t n;

    if (tok == NULL) {
        return 0;
    }
    if (strcmp(tok, "=") == 0) {
        return 1;
    }
    if (strcmp(tok, "==") == 0 || strcmp(tok, "!=") == 0 ||
        strcmp(tok, "<=") == 0 || strcmp(tok, ">=") == 0) {
        return 0;
    }
    /* Compound assignments: end with '=' but length >= 2 and not a
     * comparison. Examples: +=, -=, *=, /=, %=, <<=, >>=, &=, |=, ^=. */
    n = strlen(tok);
    if (n >= 2 && tok[n - 1] == '=') {
        return 1;
    }
    return 0;
}

static int pk_c_effects_token_is_inc_dec(const char *tok) {
    if (tok == NULL) {
        return 0;
    }
    return strcmp(tok, "++") == 0 || strcmp(tok, "--") == 0;
}

static int pk_c_effects_token_is_deref(const char *tok) {
    return tok != NULL && strcmp(tok, "*") == 0;
}

typedef struct {
    char *target;
} pk_c_effects_lvalue_ctx;

static enum CXChildVisitResult pk_c_effects_find_lvalue_root(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    pk_c_effects_lvalue_ctx *ctx = (pk_c_effects_lvalue_ctx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    (void)parent;

    if (ctx->target != NULL) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_DeclRefExpr || kind == CXCursor_MemberRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);
        char *name = NULL;

        if (clang_Cursor_isNull(referenced) == 0) {
            name = pk_c_effects_string(clang_getCursorSpelling(referenced));
        }
        if (name == NULL || name[0] == '\0') {
            free(name);
            name = pk_c_effects_string(clang_getCursorSpelling(cursor));
        }
        if (name != NULL && name[0] != '\0') {
            ctx->target = name;
            return CXChildVisit_Break;
        }
        free(name);
        return CXChildVisit_Break;
    }
    /* For ArraySubscriptExpr / ParenExpr / CStyleCastExpr / UnaryOperator
     * (deref), the named root is reached by recursing into the first
     * child. */
    return CXChildVisit_Recurse;
}

static char *pk_c_effects_resolve_target(CXCursor cursor) {
    pk_c_effects_lvalue_ctx ctx = {NULL};
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    if (kind == CXCursor_DeclRefExpr || kind == CXCursor_MemberRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);
        char *name = NULL;

        if (clang_Cursor_isNull(referenced) == 0) {
            name = pk_c_effects_string(clang_getCursorSpelling(referenced));
        }
        if (name == NULL || name[0] == '\0') {
            free(name);
            name = pk_c_effects_string(clang_getCursorSpelling(cursor));
        }
        if (name != NULL && name[0] != '\0') {
            return name;
        }
        free(name);
    }
    (void)clang_visitChildren(cursor, pk_c_effects_find_lvalue_root, &ctx);
    return ctx.target;
}

static int pk_c_effects_tag_writes(pk_c_function_fact *fact, CXCursor lvalue) {
    char *target = pk_c_effects_resolve_target(lvalue);
    int rc = pk_c_function_fact_add_effect(
        fact, PK_C_EFFECT_WRITES, target == NULL ? "<unknown>" : target);

    free(target);
    return rc;
}

static int pk_c_effects_tag_reads(pk_c_function_fact *fact, CXCursor cursor) {
    char *target = pk_c_effects_resolve_target(cursor);
    int rc = pk_c_function_fact_add_effect(
        fact, PK_C_EFFECT_READS, target == NULL ? "<unknown>" : target);

    free(target);
    return rc;
}

/* Returns the call's identifier name. Tries direct referenced cursor
 * first, then falls back to spelling on the cursor itself. */
static char *pk_c_effects_call_name(CXCursor call_cursor) {
    CXCursor referenced = clang_getCursorReferenced(call_cursor);
    char *name = NULL;

    if (clang_Cursor_isNull(referenced) == 0) {
        name = pk_c_effects_string(clang_getCursorSpelling(referenced));
    }
    if (name != NULL && name[0] != '\0') {
        return name;
    }
    free(name);
    name = pk_c_effects_string(clang_getCursorSpelling(call_cursor));
    if (name != NULL && name[0] != '\0') {
        return name;
    }
    free(name);
    return NULL;
}

/* Detect whether the call dispatches through a struct-member function
 * pointer (`x->ops->method(...)`) or a local function-pointer variable.
 * Inspect the callee child of the call: a MemberRefExpr whose
 * referenced cursor is a FieldDecl, or a DeclRefExpr to a VarDecl /
 * ParmDecl, signals indirect dispatch. */
typedef struct {
    int is_indirect;
    int saw_callee;
} pk_c_effects_indirect_ctx;

static enum CXChildVisitResult pk_c_effects_check_indirect(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    pk_c_effects_indirect_ctx *ctx = (pk_c_effects_indirect_ctx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    (void)parent;

    if (ctx->saw_callee) {
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_MemberRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);
        enum CXCursorKind ref_kind;

        ctx->saw_callee = 1;
        if (clang_Cursor_isNull(referenced) == 0) {
            ref_kind = clang_getCursorKind(referenced);
            if (ref_kind == CXCursor_FieldDecl) {
                ctx->is_indirect = 1;
            }
        } else {
            /* Conservative: we couldn't resolve the member referenced
             * cursor, but a member access in callee position is almost
             * always an ops-table call. Tag indirect. */
            ctx->is_indirect = 1;
        }
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_DeclRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);

        ctx->saw_callee = 1;
        if (clang_Cursor_isNull(referenced) == 0) {
            enum CXCursorKind ref_kind = clang_getCursorKind(referenced);

            if (ref_kind == CXCursor_VarDecl ||
                ref_kind == CXCursor_ParmDecl) {
                /* Function-pointer variable / parameter dispatch. */
                ctx->is_indirect = 1;
            }
        }
        return CXChildVisit_Break;
    }
    if (kind == CXCursor_UnexposedExpr) {
        /* Step through ImplicitCastExpr / FunctionToPointerDecay. */
        return CXChildVisit_Recurse;
    }
    return CXChildVisit_Break;
}

static int pk_c_effects_call_is_indirect(CXCursor call_cursor) {
    pk_c_effects_indirect_ctx ctx = {0, 0};

    (void)clang_visitChildren(
        call_cursor, pk_c_effects_check_indirect, &ctx);
    return ctx.is_indirect;
}

static int pk_c_effects_handle_call(
    pk_c_function_fact *fact,
    CXCursor call_cursor
) {
    char *name = pk_c_effects_call_name(call_cursor);
    int indirect = pk_c_effects_call_is_indirect(call_cursor);
    int rc = 0;

    if (indirect) {
        rc = pk_c_function_fact_add_effect(
            fact,
            PK_C_EFFECT_UNRESOLVED_CALL,
            name == NULL ? "<unknown>" : name);
        if (rc != 0) {
            free(name);
            return rc;
        }
    }
    if (name != NULL) {
        if (pk_c_effects_name_in_set(
                name, kPkCEffectsIoNames, kPkCEffectsIoNamesLen) ||
            pk_c_effects_name_has_io_prefix(name)) {
            rc = pk_c_function_fact_add_effect(fact, PK_C_EFFECT_IO, NULL);
            if (rc != 0) {
                free(name);
                return rc;
            }
        }
        if (pk_c_effects_name_in_set(
                name, kPkCEffectsPanicNames, kPkCEffectsPanicNamesLen)) {
            rc = pk_c_function_fact_add_effect(fact, PK_C_EFFECT_PANICS, NULL);
            if (rc != 0) {
                free(name);
                return rc;
            }
        }
    }
    free(name);
    return rc;
}

/* Helper used by the main walker: grab the Nth child cursor. */
typedef struct {
    int target_index;
    int current;
    CXCursor cursor;
    int found;
} pk_c_effects_nth_ctx;

static enum CXChildVisitResult pk_c_effects_visit_nth(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    pk_c_effects_nth_ctx *ctx = (pk_c_effects_nth_ctx *)client_data;
    (void)parent;

    if (ctx->current == ctx->target_index) {
        ctx->cursor = cursor;
        ctx->found = 1;
        return CXChildVisit_Break;
    }
    ctx->current++;
    return CXChildVisit_Continue;
}

static int pk_c_effects_get_nth_child(CXCursor parent, int n, CXCursor *out) {
    pk_c_effects_nth_ctx ctx;

    ctx.target_index = n;
    ctx.current = 0;
    ctx.found = 0;
    (void)clang_visitChildren(parent, pk_c_effects_visit_nth, &ctx);
    if (ctx.found && out != NULL) {
        *out = ctx.cursor;
    }
    return ctx.found;
}

/* Walk callback: dispatch on cursor kind and tag the appropriate
 * effects. Returns CXChildVisit_Recurse / Continue / Break to control
 * traversal. The visit_ctx's fact is updated in place. */
static enum CXChildVisitResult pk_c_effects_walk(
    CXCursor cursor,
    CXCursor parent,
    CXClientData client_data
) {
    pk_c_effects_visit_ctx *ctx = (pk_c_effects_visit_ctx *)client_data;
    enum CXCursorKind kind = clang_getCursorKind(cursor);
    enum CXCursorKind parent_kind = clang_getCursorKind(parent);

    /* Don't recurse into nested function definitions; the outer walker
     * handles those as separate facts. */
    if (kind == CXCursor_FunctionDecl) {
        return CXChildVisit_Continue;
    }

    /* Inline asm: Unsafe. */
    if (kind == CXCursor_AsmStmt) {
        if (pk_c_function_fact_add_effect(
                ctx->fact, PK_C_EFFECT_UNSAFE, NULL) != 0) {
            return CXChildVisit_Break;
        }
        return CXChildVisit_Continue;
    }

    /* Cast to a non-void pointer type: type-punning, tag Unsafe. */
    if (kind == CXCursor_CStyleCastExpr) {
        CXType ty = clang_getCursorType(cursor);

        if (ty.kind == CXType_Pointer) {
            CXType pointee = clang_getPointeeType(ty);

            if (pointee.kind != CXType_Void &&
                pointee.kind != CXType_Invalid) {
                if (pk_c_function_fact_add_effect(
                        ctx->fact, PK_C_EFFECT_UNSAFE, NULL) != 0) {
                    return CXChildVisit_Break;
                }
            }
        }
        return CXChildVisit_Recurse;
    }

    /* Binary operators: detect assignment by tokenizing operator. */
    if (kind == CXCursor_BinaryOperator ||
        kind == CXCursor_CompoundAssignOperator) {
        char *op = pk_c_effects_operator_token(cursor, 1);
        int is_assign = pk_c_effects_token_is_assign(op);
        free(op);

        if (is_assign) {
            CXCursor lhs;

            if (pk_c_effects_get_nth_child(cursor, 0, &lhs)) {
                if (pk_c_effects_tag_writes(ctx->fact, lhs) != 0) {
                    return CXChildVisit_Break;
                }
            } else {
                if (pk_c_function_fact_add_effect(
                        ctx->fact, PK_C_EFFECT_WRITES, "<unknown>") != 0) {
                    return CXChildVisit_Break;
                }
            }
            /* Recurse into RHS so reads from the RHS subtree get
             * tagged. We cannot easily skip just the LHS, so recurse
             * into everything; double-tagging the LHS as a read is
             * acceptable conservatism. */
            return CXChildVisit_Recurse;
        }
        return CXChildVisit_Recurse;
    }

    /* Unary operators: ++ / -- are writes; * (deref) used as lvalue is
     * a write only if the parent is an assignment LHS, otherwise it's
     * a read. Conservative: any `*ptr` rooted at a named target gets a
     * Reads tag, plus `++` / `--` get Writes. */
    if (kind == CXCursor_UnaryOperator) {
        char *op = pk_c_effects_operator_token(cursor, 0);
        int is_inc_dec = pk_c_effects_token_is_inc_dec(op);
        int is_deref = pk_c_effects_token_is_deref(op);

        free(op);
        if (is_inc_dec) {
            CXCursor operand;

            if (pk_c_effects_get_nth_child(cursor, 0, &operand)) {
                if (pk_c_effects_tag_writes(ctx->fact, operand) != 0) {
                    return CXChildVisit_Break;
                }
                if (pk_c_effects_tag_reads(ctx->fact, operand) != 0) {
                    return CXChildVisit_Break;
                }
            } else {
                if (pk_c_function_fact_add_effect(
                        ctx->fact, PK_C_EFFECT_WRITES, "<unknown>") != 0) {
                    return CXChildVisit_Break;
                }
            }
            return CXChildVisit_Continue;
        }
        if (is_deref) {
            CXCursor operand;

            if (pk_c_effects_get_nth_child(cursor, 0, &operand)) {
                if (pk_c_effects_tag_reads(ctx->fact, operand) != 0) {
                    return CXChildVisit_Break;
                }
            }
            return CXChildVisit_Recurse;
        }
        return CXChildVisit_Recurse;
    }

    /* MemberExpr / ArraySubscriptExpr standalone: tag Reads. (When
     * they're an LHS the BinaryOperator branch already over-tags
     * with Writes.) */
    if (kind == CXCursor_MemberRefExpr ||
        kind == CXCursor_ArraySubscriptExpr) {
        if (pk_c_effects_tag_reads(ctx->fact, cursor) != 0) {
            return CXChildVisit_Break;
        }
        return CXChildVisit_Recurse;
    }

    /* Direct call expression. */
    if (kind == CXCursor_CallExpr) {
        if (pk_c_effects_handle_call(ctx->fact, cursor) != 0) {
            return CXChildVisit_Break;
        }
        return CXChildVisit_Recurse;
    }

    /* Macro instantiation: panic / WARN / BUG macros are reported as
     * MacroInstantiation when the kernel headers are not on the
     * include path. Detect by spelling. */
    if (kind == CXCursor_MacroInstantiation) {
        char *name = pk_c_effects_string(clang_getCursorSpelling(cursor));

        if (pk_c_effects_name_in_set(
                name, kPkCEffectsPanicNames, kPkCEffectsPanicNamesLen)) {
            if (pk_c_function_fact_add_effect(
                    ctx->fact, PK_C_EFFECT_PANICS, NULL) != 0) {
                free(name);
                return CXChildVisit_Break;
            }
        }
        if (name != NULL && (pk_c_effects_name_in_set(
                name, kPkCEffectsIoNames, kPkCEffectsIoNamesLen) ||
                pk_c_effects_name_has_io_prefix(name))) {
            if (pk_c_function_fact_add_effect(
                    ctx->fact, PK_C_EFFECT_IO, NULL) != 0) {
                free(name);
                return CXChildVisit_Break;
            }
        }
        free(name);
        return CXChildVisit_Continue;
    }

    /* RecoveryExpr-wrapped call (CXCursor_UnexposedExpr at a non-call
     * parent). Mirror the shape from clang_ast.c:595 (PR #510). The
     * first child of a RecoveryExpr-wrapped call is a DeclRefExpr to
     * the FunctionDecl. */
    if (kind == CXCursor_UnexposedExpr &&
        parent_kind != CXCursor_CallExpr &&
        parent_kind != CXCursor_UnexposedExpr) {
        CXCursor first;

        if (pk_c_effects_get_nth_child(cursor, 0, &first)) {
            enum CXCursorKind fk = clang_getCursorKind(first);

            if (fk == CXCursor_DeclRefExpr) {
                CXCursor referenced = clang_getCursorReferenced(first);

                if (clang_Cursor_isNull(referenced) == 0) {
                    enum CXCursorKind rk = clang_getCursorKind(referenced);

                    if (rk == CXCursor_FunctionDecl) {
                        char *name = pk_c_effects_string(
                            clang_getCursorSpelling(referenced));

                        if (name != NULL) {
                            if (pk_c_effects_name_in_set(
                                    name,
                                    kPkCEffectsPanicNames,
                                    kPkCEffectsPanicNamesLen)) {
                                if (pk_c_function_fact_add_effect(
                                        ctx->fact,
                                        PK_C_EFFECT_PANICS,
                                        NULL) != 0) {
                                    free(name);
                                    return CXChildVisit_Break;
                                }
                            }
                            if (pk_c_effects_name_in_set(
                                    name,
                                    kPkCEffectsIoNames,
                                    kPkCEffectsIoNamesLen) ||
                                pk_c_effects_name_has_io_prefix(name)) {
                                if (pk_c_function_fact_add_effect(
                                        ctx->fact,
                                        PK_C_EFFECT_IO,
                                        NULL) != 0) {
                                    free(name);
                                    return CXChildVisit_Break;
                                }
                            }
                            free(name);
                        }
                    }
                }
            }
        }
        return CXChildVisit_Recurse;
    }

    return CXChildVisit_Recurse;
}

void pk_c_extract_function_effects(
    pk_c_function_fact *fact,
    void *function_cursor_ptr
) {
    CXCursor *cursor = (CXCursor *)function_cursor_ptr;
    pk_c_effects_visit_ctx ctx;

    if (fact == NULL || cursor == NULL) {
        return;
    }
    ctx.fact = fact;
    (void)clang_visitChildren(*cursor, pk_c_effects_walk, &ctx);
}
