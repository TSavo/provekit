// Fixture framework (P4.5 test b): assertEquals's BODY under the name `check`.
// The throw-locus deriver must classify this EQUALITY purely from the guard
// `!objectsAreEqual(expected, actual)` reaching a throw — the name `check`
// carries no information and must not be consulted.
package org.junit.custom;

public final class CheckAssert {

    private CheckAssert() {}

    public static void check(Object expected, Object actual) {
        if (!objectsAreEqual(expected, actual)) {
            throw new AssertionError("expected: <" + expected + "> but was: <" + actual + ">");
        }
    }

    private static boolean objectsAreEqual(Object obj1, Object obj2) {
        if (obj1 == null) {
            return obj2 == null;
        }
        return obj1.equals(obj2);
    }
}
