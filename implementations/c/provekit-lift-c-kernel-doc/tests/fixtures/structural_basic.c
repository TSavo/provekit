int helper_alloc(int *out);
int helper_release(int *p);
int helper_inplace(int *src, int *dst, int n);

int caller_safe(int n) {
    int buf[16];
    int *p = buf;
    helper_alloc(p);
    helper_inplace(p, p, n);
    helper_release(p);
    return 0;
}

int caller_unsafe(int *external, int n) {
    helper_inplace(external, external, n);
    return 0;
}
