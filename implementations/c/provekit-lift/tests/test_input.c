/* Test input for C kit lifter */
void foo(int *x) {
    if (!x) {
        BUG_ON(1);
    }
    WARN_ON(x == NULL);
}

void bar(int *y) {
    assert(y != NULL);
    __must_hold(lock);
}

int main(void) {
    int x = 0;
    foo(&x);
    bar(&x);
    return 0;
}