// Fixture framework (P4.5 test e): synthetic disagreement between the
// guard-derived order and the parameter names.
//   - guard `actual != other` puts the left operand (param 0) in the
//     expected slot → guard says expected-first
//   - but param[0] is named "actual" → names say actual-first
// The cross-check must fire: UNLEARNED + a disagreement diagnostic.
// Params are PRIMITIVE ints so the ==-dispatch gate (reference identity is
// not value equality) admits the guard and the disagreement check is reached.
package org.junit.disagree;

public final class DisagreeAssert {

    private DisagreeAssert() {}

    public static void assertEquals(int actual, int other) {
        if (actual != other) {
            throw new AssertionError("not equal");
        }
    }
}
