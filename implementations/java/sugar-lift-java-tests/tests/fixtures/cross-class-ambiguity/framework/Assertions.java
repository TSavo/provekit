// H1 [A1] cross-class ambiguity fixture — ASSERTIONS (framework entry point).
// checkEq delegates to helperChk(a,b) which exists in BOTH AssertA and AssertB
// in the corpus with OPPOSITE guard semantics (inequality vs equality predicate).
// The unqualified call is ambiguous — VocabDeriver must classify checkEq as
// UNLEARNED (never first-match) with the H1 [A1] fix.
package com.provekit.fixture;

public class Assertions {
    public static void checkEq(Object expected, Object actual) {
        // helperChk exists in both AssertA and AssertB with same arity.
        // H1 [A1] fix: countMatchesInCorpus("helperChk", 2) > 1 → UNLEARNED.
        // Without fix: first-match would pick whichever class iterates first.
        helperChk(expected, actual);
    }
}
