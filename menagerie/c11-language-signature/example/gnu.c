int gnu(int x) {
    void *target = &&done;
    int y = ({ int t = x + 1; t; });
done:
    return _Generic(x, int: y, default: x);
}
