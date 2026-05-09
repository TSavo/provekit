/* SPDX-License-Identifier: Apache-2.0 */

#define BUG_ON(x) do { if (x) return -1; } while (0)
#define ENOMEM 12
#define __user
#define __rcu
#define __must_hold(x)
#define __acquires(x)
#define __releases(x)
#define assert(x) do { } while (0)
typedef unsigned long size_t;
typedef unsigned int gfp_t;

int bug_on_nonnegative(int x) {
    BUG_ON(x < 0);
    return x;
}

int errno_guard(char *ptr) {
    if (!ptr) return -ENOMEM;
    return 0;
}

int user_buffer(__user char *buf) {
    return 0;
}

int held_lock(int x) __must_hold(lock) {
    return x;
}

int trailing_return(int x) {
    return x + 1;
}

int ret_guard(int ret) {
    if (ret < 0) return ret;
    return ret;
}

int goto_error(int x) {
    if (x == 0) goto error;
    return x;
error:
    return -1;
}

int assert_positive(int x) {
    assert(x > 0);
    return x;
}

int rcu_pointer(__rcu int *p) {
    return 0;
}

int sized_count(size_t n) {
    return 0;
}

int gfp_flags(gfp_t gfp) {
    return 0;
}

int acquire_lock(int x) __acquires(lock) {
    return x;
}

int release_lock(int x) __releases(lock) {
    return x;
}
