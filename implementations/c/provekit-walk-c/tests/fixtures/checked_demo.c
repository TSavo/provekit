#define BUG_ON(x) do { if (x) return -1; } while (0)
int checked(int x) {
    BUG_ON(x < 10);
    return x;
}
int composed_ok(void) {
    int y = 42;
    return checked(y);
}
