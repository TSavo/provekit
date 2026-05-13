class C {
    static int Add(int x, int y) {
        return x + y;
    }

    static int Max(int a, int b) {
        if (a > b) {
            return a;
        } else {
            return b;
        }
    }

    static int Factorial(int n) {
        int result = 1;
        while (n > 0) {
            result = result * n;
            n = n - 1;
        }
        return result;
    }
}
