package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static java.lang.Math.abs;

/**
 * BAD suite: the industry belief, falsified.
 *
 * The claim below is what virtually every developer believes:
 * abs(Integer.MIN_VALUE) should return Integer.MAX_VALUE (2147483647),
 * i.e. the positive version of -2147483648.
 *
 * This is FALSE under two's complement arithmetic (JLS §4.2.1).
 * The JDK Math.abs(int) body is `(a < 0) ? -a : a`.
 * At a = -2147483648: -a overflows back to -2147483648.
 *
 * The kit lifts:
 *   1. the sworn equality    =(abs(-2147483648), 2147483647)   ← the false claim
 *   2. the universe row      int32.eq-bv-expr(abs(-2147483648),
 *                              bv32.ite(bv32.slt(a,0), bv32.neg(a), a))
 *
 * The conjunction is UNSATISFIED: z3's bitvector theory evaluates the walked
 * body at a=-2147483648 and confirms -2147483648 ≠ 2147483647.
 * The refutation comes from the BV expression walked from the vendor's own
 * source — no arithmetic is hand-authored in the kit.
 */
public class AbsUniverseBadTest {

    @Test
    public void testAbsMinValueFalseBelief() {
        // THE INDUSTRY BELIEF — false under two's complement.
        // The walked BV expression refutes this: abs(MIN_VALUE) == MIN_VALUE, not MAX_VALUE.
        assertEquals(2147483647, abs(-2147483648));
    }
}
