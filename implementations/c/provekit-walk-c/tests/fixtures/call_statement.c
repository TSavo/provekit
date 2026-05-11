void external_call(int);

int call_statement(int x) {
    external_call(x);
    return x;
}
