struct item {
    int value;
};

int helper(int x) {
    return x + 1;
}

int call(struct item *ptr, struct item obj, int x) {
    int y = helper((int)x);
    y = y + obj.value;
    (void)ptr;
    return y;
}
