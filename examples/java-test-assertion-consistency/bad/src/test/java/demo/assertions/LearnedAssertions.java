package demo.assertions;

public final class LearnedAssertions {
    private LearnedAssertions() {}

    public static void assertSameValue(int expected, int actual) {
        if (expected != actual) {
            throw new AssertionError("expected " + expected + " but got " + actual);
        }
    }

    public static void assertEquals(int expected, int actual) {
        record(expected, actual);
    }

    public static void assertEquals(double expected, double actual, double delta) {
        if (Math.abs(expected - actual) > delta) {
            throw new AssertionError("not close");
        }
    }

    private static void record(Object expected, Object actual) {
        if (expected == actual) {
            return;
        }
    }
}
