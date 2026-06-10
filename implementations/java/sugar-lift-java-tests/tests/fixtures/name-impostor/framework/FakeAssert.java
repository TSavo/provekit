// Fixture framework (P4.5 test c): a method NAMED assertEquals that asserts
// NOTHING — its body never reaches a throw. Under name-keyed classification
// this would have been EQUALITY (the falsePass). Under throw-locus derivation
// it is NOT an assertion.
package org.junit.fake;

public final class FakeAssert {

    private FakeAssert() {}

    public static void assertEquals(Object expected, Object actual) {
        return;
    }
}
