int guarded_store(int *p) {
    if (!p) return -1;
    *p = 0;
    return 0;
}

int call_guarded(int *p) {
    return guarded_store(p);
}
