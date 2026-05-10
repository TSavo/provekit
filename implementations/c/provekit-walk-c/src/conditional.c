/* SPDX-License-Identifier: Apache-2.0 */

#include "walk_c.h"

#ifdef PK_C_ENABLE_CLANG_AST

#include <stdlib.h>
#include <string.h>

typedef struct {
    CXCursor items[8];
    unsigned len;
} CursorList;

typedef struct {
    CXCursor target;
    int found;
} ContainsCtx;

typedef struct {
    CXCursor target;
    CXCursor child;
    int found;
} ChildCtx;

typedef struct {
    pk_c_walk_formula *cond;
    char *branch;
    size_t join_stmt_index;
    int line;
    int column;
} ConditionalFrame;

typedef struct {
    ConditionalFrame *items;
    size_t len;
    size_t cap;
} FrameVec;

typedef struct {
    char *name;
    pk_c_walk_term *term;
} Assignment;

static enum CXChildVisitResult collect_children(CXCursor cursor, CXCursor parent, CXClientData data) {
    CursorList *list = (CursorList *)data;

    (void)parent;
    if (list->len < sizeof(list->items) / sizeof(list->items[0])) {
        list->items[list->len++] = cursor;
    }
    return CXChildVisit_Continue;
}

static enum CXChildVisitResult contains_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    ContainsCtx *ctx = (ContainsCtx *)data;

    (void)parent;
    if (clang_equalCursors(cursor, ctx->target)) {
        ctx->found = 1;
        return CXChildVisit_Break;
    }
    return CXChildVisit_Recurse;
}

static int cursor_contains(CXCursor root, CXCursor target) {
    ContainsCtx ctx = {target, 0};

    if (clang_equalCursors(root, target)) {
        return 1;
    }
    (void)clang_visitChildren(root, contains_visitor, &ctx);
    return ctx.found;
}

static enum CXChildVisitResult containing_child_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    ChildCtx *ctx = (ChildCtx *)data;

    (void)parent;
    if (cursor_contains(cursor, ctx->target)) {
        ctx->child = cursor;
        ctx->found = 1;
        return CXChildVisit_Break;
    }
    return CXChildVisit_Continue;
}

static int find_containing_child(CXCursor root, CXCursor target, CXCursor *child_out) {
    ChildCtx ctx;

    memset(&ctx, 0, sizeof(ctx));
    ctx.target = target;
    (void)clang_visitChildren(root, containing_child_visitor, &ctx);
    if (!ctx.found) {
        return 0;
    }
    *child_out = ctx.child;
    return 1;
}

static void frame_vec_free(FrameVec *vec) {
    if (vec == NULL) {
        return;
    }
    for (size_t i = 0; i < vec->len; i++) {
        pk_c_walk_formula_free(vec->items[i].cond);
        free(vec->items[i].branch);
    }
    free(vec->items);
    memset(vec, 0, sizeof(*vec));
}

static int frame_vec_append(
    FrameVec *vec,
    pk_c_walk_formula *cond,
    const char *branch,
    size_t join_stmt_index,
    int line,
    int column
) {
    ConditionalFrame *items;
    size_t cap;

    if (cond == NULL) {
        return 0;
    }
    if (vec->len == vec->cap) {
        cap = vec->cap == 0 ? 4 : vec->cap * 2;
        if (cap < vec->cap) {
            pk_c_walk_formula_free(cond);
            return -1;
        }
        items = realloc(vec->items, cap * sizeof(*items));
        if (items == NULL) {
            pk_c_walk_formula_free(cond);
            return -1;
        }
        vec->items = items;
        vec->cap = cap;
    }
    vec->items[vec->len].cond = cond;
    vec->items[vec->len].branch = pk_c_walk_copy(branch);
    vec->items[vec->len].join_stmt_index = join_stmt_index;
    vec->items[vec->len].line = line;
    vec->items[vec->len].column = column;
    if (vec->items[vec->len].branch == NULL) {
        pk_c_walk_formula_free(cond);
        memset(&vec->items[vec->len], 0, sizeof(vec->items[vec->len]));
        return -1;
    }
    vec->len++;
    return 0;
}

static int append_if_frame(
    FrameVec *frames,
    CXCursor if_cursor,
    CXCursor cond_cursor,
    const char *branch,
    int negate,
    size_t stmt_index
) {
    pk_c_walk_formula *cond = pk_c_walk_lift_condition(cond_cursor);
    int line = 0;
    int column = 0;

    if (negate) {
        cond = pk_c_walk_formula_negate_take(cond);
    }
    pk_c_walk_locus(if_cursor, &line, &column);
    return frame_vec_append(frames, cond, branch, stmt_index, line, column);
}

static int collect_frames(CXCursor root, CXCursor target, FrameVec *frames, size_t stmt_index) {
    CursorList list = {{0}, 0};
    CXCursor child;

    if (!cursor_contains(root, target)) {
        return 0;
    }
    if (clang_getCursorKind(root) == CXCursor_IfStmt) {
        (void)clang_visitChildren(root, collect_children, &list);
        if (list.len >= 2 && cursor_contains(list.items[1], target)) {
            if (append_if_frame(frames, root, list.items[0], "then", 0, stmt_index) != 0) {
                return -1;
            }
            return collect_frames(list.items[1], target, frames, stmt_index);
        }
        if (list.len >= 3 && cursor_contains(list.items[2], target)) {
            if (append_if_frame(frames, root, list.items[0], "else", 1, stmt_index) != 0) {
                return -1;
            }
            return collect_frames(list.items[2], target, frames, stmt_index);
        }
    }
    if (find_containing_child(root, target, &child)) {
        return collect_frames(child, target, frames, stmt_index);
    }
    return 0;
}

int pk_c_walk_apply_call_context(
    CXCursor top_stmt,
    CXCursor call_cursor,
    pk_c_walk_formula **wp,
    pk_c_walk_chain *chain,
    size_t stmt_index
) {
    FrameVec frames = {0};
    int rc = 0;

    if (collect_frames(top_stmt, call_cursor, &frames, stmt_index) != 0) {
        frame_vec_free(&frames);
        return -1;
    }
    for (size_t i = frames.len; i > 0; i--) {
        ConditionalFrame *frame = &frames.items[i - 1];
        pk_c_walk_formula *cond = pk_c_walk_formula_clone(frame->cond);
        pk_c_walk_formula *next;

        if (cond == NULL) {
            rc = -1;
            break;
        }
        next = pk_c_walk_formula_implies_take(cond, *wp);
        *wp = NULL;
        if (next == NULL) {
            rc = -1;
            break;
        }
        *wp = next;
        if (pk_c_walk_chain_add_conditional_arm_arrival(
                chain,
                frame->branch,
                stmt_index,
                frame->join_stmt_index,
                frame->line,
                frame->column,
                frame->cond,
                *wp) != 0) {
            rc = -1;
            break;
        }
    }
    frame_vec_free(&frames);
    return rc;
}

static int single_stmt(CXCursor cursor, CXCursor *stmt_out) {
    CursorList list = {{0}, 0};

    if (clang_getCursorKind(cursor) != CXCursor_CompoundStmt) {
        *stmt_out = cursor;
        return 1;
    }
    (void)clang_visitChildren(cursor, collect_children, &list);
    if (list.len != 1) {
        return 0;
    }
    *stmt_out = list.items[0];
    return 1;
}

static int source_has_plain_assignment(CXCursor cursor) {
    char *source = pk_c_walk_cursor_source(cursor);
    int found = 0;

    if (source == NULL) {
        return 0;
    }
    for (char *p = source; *p != '\0'; p++) {
        char prev = p == source ? '\0' : p[-1];
        char next = p[1];

        if (*p == '=' && prev != '=' && prev != '!' && prev != '<' && prev != '>' && next != '=') {
            found = 1;
            break;
        }
    }
    free(source);
    return found;
}

static int lift_assignment(CXCursor cursor, Assignment *assignment) {
    CursorList list = {{0}, 0};

    memset(assignment, 0, sizeof(*assignment));
    if (clang_getCursorKind(cursor) != CXCursor_BinaryOperator || !source_has_plain_assignment(cursor)) {
        return 0;
    }
    (void)clang_visitChildren(cursor, collect_children, &list);
    if (list.len < 2) {
        return 0;
    }
    assignment->name = pk_c_walk_cursor_source(list.items[0]);
    assignment->term = pk_c_walk_lift_term(list.items[1]);
    if (assignment->name == NULL || assignment->term == NULL) {
        free(assignment->name);
        pk_c_walk_term_free(assignment->term);
        memset(assignment, 0, sizeof(*assignment));
        return -1;
    }
    return 1;
}

static void assignment_free(Assignment *assignment) {
    if (assignment == NULL) {
        return;
    }
    free(assignment->name);
    pk_c_walk_term_free(assignment->term);
    memset(assignment, 0, sizeof(*assignment));
}

static int apply_two_armed_assignments(
    CXCursor if_cursor,
    CXCursor cond_cursor,
    CXCursor then_cursor,
    CXCursor else_cursor,
    pk_c_walk_formula **wp,
    pk_c_walk_chain *chain,
    size_t stmt_index
) {
    CXCursor then_stmt;
    CXCursor else_stmt;
    Assignment then_assignment;
    Assignment else_assignment;
    pk_c_walk_formula *condition = NULL;
    pk_c_walk_formula *then_wp = NULL;
    pk_c_walk_formula *else_wp = NULL;
    pk_c_walk_formula *then_cond = NULL;
    pk_c_walk_formula *else_cond = NULL;
    pk_c_walk_formula *then_impl = NULL;
    pk_c_walk_formula *else_impl = NULL;
    pk_c_walk_formula *next = NULL;
    int line = 0;
    int column = 0;
    int rc = -1;
    int then_lifted;
    int else_lifted;

    memset(&then_assignment, 0, sizeof(then_assignment));
    memset(&else_assignment, 0, sizeof(else_assignment));
    if (!single_stmt(then_cursor, &then_stmt) || !single_stmt(else_cursor, &else_stmt)) {
        return 0;
    }
    then_lifted = lift_assignment(then_stmt, &then_assignment);
    else_lifted = lift_assignment(else_stmt, &else_assignment);
    if (then_lifted < 0 || else_lifted < 0) {
        goto done;
    }
    if (then_lifted == 0 || else_lifted == 0 ||
        strcmp(then_assignment.name, else_assignment.name) != 0) {
        rc = 0;
        goto done;
    }
    condition = pk_c_walk_lift_condition(cond_cursor);
    if (condition == NULL) {
        rc = 0;
        goto done;
    }
    then_wp = pk_c_walk_substitute_formula(*wp, then_assignment.name, then_assignment.term);
    else_wp = pk_c_walk_substitute_formula(*wp, else_assignment.name, else_assignment.term);
    then_cond = pk_c_walk_formula_clone(condition);
    else_cond = pk_c_walk_formula_negate_take(pk_c_walk_formula_clone(condition));
    if (then_wp == NULL || else_wp == NULL || then_cond == NULL || else_cond == NULL) {
        goto done;
    }
    pk_c_walk_locus(if_cursor, &line, &column);
    if (pk_c_walk_chain_add_conditional_arm_arrival(
            chain, "then", stmt_index, stmt_index, line, column, then_cond, then_wp) != 0 ||
        pk_c_walk_chain_add_conditional_arm_arrival(
            chain, "else", stmt_index, stmt_index, line, column, else_cond, else_wp) != 0) {
        goto done;
    }
    then_impl = pk_c_walk_formula_implies_take(then_cond, then_wp);
    then_cond = NULL;
    then_wp = NULL;
    if (then_impl == NULL) {
        goto done;
    }
    else_impl = pk_c_walk_formula_implies_take(else_cond, else_wp);
    else_cond = NULL;
    else_wp = NULL;
    if (else_impl == NULL) {
        goto done;
    }
    next = pk_c_walk_formula_and_take(then_impl, else_impl);
    then_impl = NULL;
    else_impl = NULL;
    if (next == NULL) {
        goto done;
    }
    pk_c_walk_formula_free(*wp);
    *wp = next;
    next = NULL;
    rc = 0;

done:
    assignment_free(&then_assignment);
    assignment_free(&else_assignment);
    pk_c_walk_formula_free(condition);
    pk_c_walk_formula_free(then_wp);
    pk_c_walk_formula_free(else_wp);
    pk_c_walk_formula_free(then_cond);
    pk_c_walk_formula_free(else_cond);
    pk_c_walk_formula_free(then_impl);
    pk_c_walk_formula_free(else_impl);
    pk_c_walk_formula_free(next);
    return rc;
}

int pk_c_walk_apply_two_armed_if_stmt(
    CXCursor stmt,
    pk_c_walk_formula **wp,
    pk_c_walk_chain *chain,
    size_t stmt_index
) {
    CursorList list = {{0}, 0};

    if (clang_getCursorKind(stmt) != CXCursor_IfStmt) {
        return 0;
    }
    (void)clang_visitChildren(stmt, collect_children, &list);
    if (list.len < 3) {
        return 0;
    }
    return apply_two_armed_assignments(stmt, list.items[0], list.items[1], list.items[2], wp, chain, stmt_index);
}

#endif
