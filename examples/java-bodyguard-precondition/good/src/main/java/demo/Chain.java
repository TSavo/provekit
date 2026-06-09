package demo;

public final class Chain {
    private Chain() {}

    public static int digit(int radix) {
        if (radix < Character.MIN_RADIX || radix > Character.MAX_RADIX) {
            throw new IllegalArgumentException("radix");
        }
        return radix;
    }

    public static int edge() {
        return digit(16);
    }
}
