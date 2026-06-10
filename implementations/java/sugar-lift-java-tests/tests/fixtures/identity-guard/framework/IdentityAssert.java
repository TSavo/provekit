// Discrimination fixture for the ==-dispatch gate. Two methods, same guard
// SHAPE, different operand types:
//  - assertNotSame over Objects: `expected == actual` is REFERENCE IDENTITY
//    (two .equals() values can be distinct refs) -> must be UNLEARNED, never
//    lifted as value-inequality.
//  - assertEqualsInt over ints: `expected != actual` is VALUE equality ->
//    classifies EQUALITY.
// Every Java developer knows == vs .equals; so must the lifter.
package org.junit.identity;

public final class IdentityAssert {
    private IdentityAssert() {}

    public static void assertNotSame(Object expected, Object actual) {
        if (expected == actual) {
            throw new AssertionError("expected not same");
        }
    }

    public static void assertEqualsInt(int expected, int actual) {
        if (expected != actual) {
            throw new AssertionError("expected equal ints");
        }
    }
}
