/* SPDX-License-Identifier: Apache-2.0 */

int unsupported_goto(int x) {
    if (x) goto out;
    return x;
out:
    return -1;
}

int unsupported_ternary(int x) {
    return x ? 1 : 0;
}
