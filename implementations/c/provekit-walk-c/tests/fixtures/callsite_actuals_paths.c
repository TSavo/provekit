/* SPDX-License-Identifier: Apache-2.0 */

int path_sink(int lhs, int rhs) {
    return lhs + rhs;
}

int path_plain(int x) {
    path_sink(x, x + 1);
    return x;
}

int path_decl_initializer(int x) {
    int y = path_sink(x, x + 2);
    return y;
}

int path_assignment_rhs(int x) {
    int y = 0;
    y = path_sink(x, x + 3);
    return y;
}

int path_conditional_arm(int flag, int x) {
    if (flag) {
        path_sink(x, x + 4);
    } else {
        path_sink(x + 5, x + 6);
    }
    return x;
}
