package demo;

import org.junit.Test;
import static org.junit.Assert.assertTrue;

/**
 * G2b BAD suite — bound federation: two callers, same callsite, CONTRADICTING bounds.
 *
 * The vendor swears g(7) > 10.
 * The consumer swears g(7) < 5.
 *
 * Same #euf# callsite CID: g#euf#c:callresult_g_a1(i:7)::assertion.
 * The engine conjoins them: g(7) > 10 ∧ g(7) < 5.
 *
 * This is UNSATISFIABLE: no integer v satisfies v > 10 and v < 5 simultaneously.
 * z3: UNSAT → consistency unsatisfied.
 *
 * The refutation is purely from the two bounds — no implementation is consulted.
 */
public class BoundFederationBadTest {

    static int g(int v) { return 3; }

    @Test
    public void vendorClaimsGreaterThan10() {
        // Vendor assertion: g(7) returns something greater than 10.
        assertTrue(g(7) > 10);
    }

    @Test
    public void consumerClaimsLessThan5() {
        // Consumer assertion: g(7) returns something less than 5.
        // Contradicts vendorClaimsGreaterThan10 on the SAME #euf# CID.
        assertTrue(g(7) < 5);
    }
}
