// H1 [A1] cross-class ambiguity — CLASS B.
// helperChk(a,b): guard is `a == b` (throws when equal) → INEQUALITY semantics.
// OPPOSITE guard to AssertA.helperChk. The same name+arity in two classes means
// any unqualified delegation to helperChk is ambiguous. H1 [A1] fix: UNLEARNED.
package com.provekit.fixture;

public class AssertB {
    static void helperChk(Object a, Object b) {
        if (a == b) {
            throw new AssertionError("should not be equal");
        }
    }
}
