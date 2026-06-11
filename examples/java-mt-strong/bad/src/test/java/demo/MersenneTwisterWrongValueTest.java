package demo;

import org.apache.commons.rng.core.source32.MersenneTwister;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite (STRONG tier): a SINGLE wrong-but-plausible reference value,
 * refuted BY DERIVATION — not by within-test contradiction.
 *
 * This is the qualitative leap over the FLOOR rung. The FLOOR's BAD suite
 * (java-mt-reference) had to assert TWO contradictory values for the same draw
 * so the #euf# conjunction is UNSAT. Here there is NO contradiction: the test
 * makes ONE assertion, asserting a single value that is WRONG by one bit:
 *
 *   assertEquals(0x3fa23624, mt.nextInt());   // draw[0] is really 0x3fa23623
 *
 * The kit walks the vendor's seed→state→twist→temper recurrence for the literal
 * seed and pins `mt32.eq-seeded(0x3fa23624, <walked recurrence>)`. The walked
 * recurrence computes the GENUINE 0x3fa23623, so the equation
 *   0x3fa23624 == <walked draw[0]>
 * is FALSE → UNSATISFIED. The refutation is COMPUTATION-DRIVEN (the real
 * Mersenne Twister algorithm walked over the real seed), exactly like the CRC
 * value-pin — NOT a contradiction between two authored claims.
 *
 * A wrong reference value refuted UNSATISFIED by the walked recurrence is the
 * win: the FOL is the deliverable, the CHECK is the product.
 */
public class MersenneTwisterWrongValueTest {

    private static final int[] SEED = {0x123, 0x234, 0x345, 0x456};

    @Test
    public void testWrongDraw0() {
        MersenneTwister mt = new MersenneTwister(SEED);
        int d = mt.nextInt();
        // draw[0] is genuinely 0x3fa23623; this asserts a value wrong by one bit.
        // Refuted by the WALKED recurrence, with no second contradictory claim.
        assertEquals(0x3fa23624, d);
    }
}
