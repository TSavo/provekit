package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static account.UserHandle.accept;

/**
 * BAD suite: a consumer's FALSE validity claim about an input the @Pattern REJECTS.
 *
 * The claimed value "Alice!" is NOT a member of the walked @Pattern language
 * "^[a-z][a-z0-9_]{2,15}$": it leads with an uppercase 'A' (the language requires
 * a lowercase letter) and ends with '!' (not a member of the [a-z0-9_] body
 * class). There is no vendor vector to conjoin against — point-wise consistency
 * alone could never catch this claim. The refutation comes from the regular
 * LANGUAGE walked from the annotation, not from any sworn sample.
 *
 * The regex universe row
 *
 *     str.in-regex(accept("Alice!"), <walked @Pattern regex>)
 *
 * conjoins with the equality =(accept("Alice!"), "Alice!") under the same #euf#
 * contract name, and z3's string/regex theory refutes it: UNSAT. The refutation
 * is MEMBERSHIP-driven — "Alice!" ∉ L(@Pattern) — not a within-test
 * contradiction.
 */
public class PatternRegexBadTest {

    @Test
    public void testRejectedInputClaimedValid() {
        // FALSE: "Alice!" is not in the @Pattern language (uppercase lead, '!' body).
        // The walked regex refutes the validity claim by membership.
        assertEquals("Alice!", accept("Alice!"));
    }
}
