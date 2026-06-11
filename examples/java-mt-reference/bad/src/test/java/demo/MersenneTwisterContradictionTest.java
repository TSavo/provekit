package demo;

import org.apache.commons.rng.core.source32.MersenneTwister;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: within-test contradiction for draw[0].
 *
 * The SAME seeded draw is asserted to TWO contradictory values in one test:
 *   assertEquals(0x3fa23623, draw1)   // the vendor-sworn correct value
 *   assertEquals(0x12345678, draw1)   // a contradictory false claim
 *
 * The local `draw1` is an SSA alias for `mt.nextInt()` (position 0).
 * The kit emits TWO location-keyed point contracts for the SAME callsite
 * (same receiver `mt`, same SSA local `draw1`, same location key):
 *
 *   =(nextInt(mt), 0x3fa23623)   — from the first assertEquals
 *   =(nextInt(mt), 0x12345678)   — from the second assertEquals
 *
 * The location key conjoins both: a value cannot simultaneously equal two
 * different integers → UNSAT → unsatisfied (within-test contradiction).
 *
 * This is the SAME contradiction pattern as java-callbind-consistency/bad.
 * The refutation comes from the #euf# contract conjunction, not from
 * deriving the correct output — that is the floor limit.
 */
public class MersenneTwisterContradictionTest {

    @Test
    public void testContradiction() {
        // Seed: Nishimura canonical {0x123, 0x234, 0x345, 0x456}
        MersenneTwister mt = new MersenneTwister(new int[] {0x123, 0x234, 0x345, 0x456});

        // draw[0]: bound to SSA local `draw1`
        int draw1 = mt.nextInt();

        // Two contradictory assertions about the SAME draw via SSA local `draw1`.
        // The kit sees both, conjoins them, and z3 reports UNSAT.
        assertEquals(0x3fa23623, draw1);   // vendor-sworn correct value
        assertEquals(0x12345678, draw1);   // false claim — contradicts the line above
    }
}
