final class RecursiveFactorial {
    // concept: concept:recursive-factorial
    public static int factorial(int n) {
        if (n == 0) {
            return 1;
        }
        return n * factorial(n - 1);
    }

    public static void main(String[] args) {
        System.out.println(factorial(Integer.parseInt(args[0])));
    }
}
