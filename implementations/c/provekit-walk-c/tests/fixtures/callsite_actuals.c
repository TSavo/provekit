/* SPDX-License-Identifier: Apache-2.0 */

#define BUG() do { return -1; } while (0)

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
