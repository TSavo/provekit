package demo;

import org.junit.Test;
import static org.junit.Assert.assertTrue;

/**
 * G2b GOOD suite — bound federation: two callers, same callsite, compatible bounds.
 *
 * The vendor swears g(7) < 10.
 * The consumer swears g(7) < 5.
 *
 * To Sugar these are ONE contract: the identity is the #euf# callsite CID
 * g#euf#c:callresult_g_a1(i:7)::assertion, not the predicate.
 * The engine conjoins them: g(7) < 10 ∧ g(7) < 5.
 *
 * This is satisfiable: any value v < 5 satisfies both bounds.
 * z3: SAT → consistency discharged.
 *
 * g is a defined method so the call lifts.
 */
public class BoundFederationGoodTest {

    /**
     * A defined method with a known return value.
     * The actual implementation is irrelevant — only the lifted bounds matter.
     */
    static int g(int v) { return 3; }

    @Test
    public void vendorClaimsLessThan10() {
        // Vendor assertion: g(7) returns something less than 10.
        assertTrue(g(7) < 10);
    }

    @Test
    public void consumerClaimsLessThan5() {
        // Consumer assertion: g(7) returns something less than 5.
        // Same #euf# CID as vendorClaimsLessThan10 — conjoined automatically.
        assertTrue(g(7) < 5);
    }
}
