int g(int x) {
    int y = x + 1;
    y += 0;
    switch (y) {
    case 1:
        return y;
    case 2:
        break;
    default:
        return 0;
    }
    return y;
}
