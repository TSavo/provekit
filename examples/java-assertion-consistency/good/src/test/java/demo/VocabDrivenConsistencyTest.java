package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;

/**
 * GOOD suite (Phase 2 vocab-driven): assertEquals + assertNotEquals on same callsite.
 * assertEquals(1, g(2)) AND assertNotEquals(2, g(2)):
 *   → g(2)==1 ∧ g(2)≠2
 *   → These are consistent: if g(2)=1 then g(2)≠2 is trivially satisfied.
 *   → Discharged.
 */
public class VocabDrivenConsistencyTest {

    @Test
    public void testEquality() {
        // learned from source: assertEquals(expected, actual) → =(g(2), 1)
        assertEquals(1, g(2));
    }

    @Test
    public void testNotEquals() {
        // learned from source: assertNotEquals(unexpected, actual) → ≠(g(2), 2)
        // Consistent with testEquality: g(2)=1 satisfies both g(2)=1 and g(2)≠2.
        assertNotEquals(2, g(2));
    }

    private int g(int x) { return x - 1; }
}
