// H1 [A1] cross-class ambiguity — CLASS A.
// helperChk(a,b): guard is `a != b` (throws when not equal) → EQUALITY semantics.
// Exists in BOTH AssertA and AssertB. Unqualified call from Assertions.checkEq
// would be ambiguous — must classify as UNLEARNED.
package com.provekit.fixture;

public class AssertA {
    static void helperChk(Object a, Object b) {
        if (a != b) {
            throw new AssertionError("not equal");
        }
    }
}
