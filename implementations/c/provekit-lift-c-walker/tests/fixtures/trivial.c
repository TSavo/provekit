/* SPDX-License-Identifier: Apache-2.0 */
/* Minimal fixture for provekit-lift-c-walker smoke test. */

int add(int a, int b) {
    return a + b;
}

int identity(int x) {
    return x;
}

int negate(int x) {
    return -x;
}

int zero(void) {
    return 0;
}
