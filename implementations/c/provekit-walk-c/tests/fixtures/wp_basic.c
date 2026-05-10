/* SPDX-License-Identifier: Apache-2.0 */

#define BUG() do { return -1; } while (0)

int callee(int x) {
    if (x < 10) {
        BUG();
    }
    return x;
}

int caller(int input) {
    if (input < 10) {
        BUG();
    }
    int y = input;
    int result = callee(y);
    return result;
}
