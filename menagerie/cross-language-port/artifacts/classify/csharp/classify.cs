public static class Transported {
    public static int classify(int x) {
        int y = 0;
        if (x > 0 && x < 10) {
            y = 1;
        } else {
            y = 2;
        }
        return y;
    }
}
