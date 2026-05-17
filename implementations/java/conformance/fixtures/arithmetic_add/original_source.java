final class ArithmeticAdd {
    // concept: concept:arithmetic-add
    public static int add(int x, int y) {
        return x + y;
    }

    public static void main(String[] args) {
        System.out.println(add(Integer.parseInt(args[0]), Integer.parseInt(args[1])));
    }
}
