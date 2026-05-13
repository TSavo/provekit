int loop(int *arr, int n) {
    int i = 0;
    int sum = 0;
    while (i < n) {
        sum = sum + arr[i];
        i = i + 1;
    }
    for (i = 0; i < n; i = i + 1) {
        sum = sum + arr[i];
    }
    do {
        sum = sum - 1;
    } while (sum > n);
    return sum;
}
