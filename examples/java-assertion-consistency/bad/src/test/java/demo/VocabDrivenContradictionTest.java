package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;

/**
 * BAD suite (Phase 2 vocab-driven): direct =/≠ contradiction through learned vocab.
 * assertEquals(1, g(2)) AND assertNotEquals(1, g(2)):
 *   → g(2)==1 ∧ g(2)≠1
 *   → Unsatisfiable (EUF: same callsite cannot equal and not-equal the same value).
 *   → Refused as contradictory.
 */
public class VocabDrivenContradictionTest {

    @Test
    public void testEquality() {
        // learned: assertEquals(expected, actual) → =(g(2), 1)
        assertEquals(1, g(2));
    }

    @Test
    public void testNotEquals() {
        // CONTRADICTION via learned vocab: assertNotEquals(1, g(2)) → ≠(g(2), 1)
        // Same callsite g(2), same value 1: =(g(2),1) ∧ ≠(g(2),1) is UNSAT.
        assertNotEquals(1, g(2));
    }

    private int g(int x) { return x - 1; }
}
