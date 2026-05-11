int sum_to(int n) {
    int s = 0;
    int i = 0;
    while (i < n) {
        s = s + i;
        i = i + 1;
    }
    return s;
}
