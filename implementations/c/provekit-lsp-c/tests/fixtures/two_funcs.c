/* Fixture: two functions, one call site. Used by integration test. */

int add(int a, int b) {
    return a + b;
}

int compute(int x) {
    int result = add(x, 10);
    return result;
}
