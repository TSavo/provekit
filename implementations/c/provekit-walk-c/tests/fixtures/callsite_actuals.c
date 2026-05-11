/* SPDX-License-Identifier: Apache-2.0 */

#define BUG() do { return -1; } while (0)

struct foo {
    int x;
};

int actuals_callee(int literal, int ref, int expr) {
    if (literal < 0) {
        BUG();
    }
    if (ref < 0) {
        BUG();
    }
    if (expr < 0) {
        BUG();
    }
    return literal + ref + expr;
}

int actuals_caller(int input) {
    return actuals_callee(7, input, input + 1);
}

int actuals_single_callee(int value) {
    return value;
}

int actuals_mixed_callee(int ref, int expr) {
    return ref + expr;
}

struct foo actuals_struct_callee(struct foo value) {
    return value;
}

int actuals_ternary_caller(int cond, int x, int y) {
    return actuals_single_callee(cond ? x : y);
}

int actuals_comma_caller(int x, int y) {
    return actuals_single_callee((x, y));
}

struct foo actuals_compound_literal_caller(void) {
    return actuals_struct_callee((struct foo){.x = 1});
}

int actuals_mixed_caller(int cond, int x, int y, int z) {
    return actuals_mixed_callee(x, cond ? y : z);
}
