int control(int x) {
    if (x < 0) {
        goto done;
    }
    x = x + 1;
done:
    return x;
}
