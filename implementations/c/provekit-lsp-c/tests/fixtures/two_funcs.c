/* Fixture: two functions, one call site. Used by integration test. */

//provekit:contract
int add(int a, int b) {
    return a + b;
}

//provekit:contract
int compute(int x) {
    int result = add(x, 10);
    return result;
}
