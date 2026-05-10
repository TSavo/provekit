#define BUG_ON(c) do { if (c) __builtin_trap(); } while (0)
int helper_a(int x) { BUG_ON(x < 0); return x; }
int helper_b(int x) { BUG_ON(x > 100); return x; }
int two_armed(int x) {
    int r;
    if (x > 50) {
        r = helper_a(x);
    } else {
        r = helper_b(x);
    }
    return r;
}
