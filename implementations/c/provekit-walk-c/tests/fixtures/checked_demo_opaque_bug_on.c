void BUG_ON(int);
int checked(int x) {
    BUG_ON(x < 10);
    return x;
}
int composed_ok(void) {
    int y = 42;
    return checked(y);
}
