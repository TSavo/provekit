package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static java.lang.Math.abs;

/**
 * BAD suite: the industry belief, refuted.
 *
 * The industry universally believes abs(x) >= 0.  The specific instance
 * abs(Integer.MIN_VALUE) >= 0  is believed to be true: most developers
 * expect abs(-2147483648) to return 2147483647 (Integer.MAX_VALUE), the
 * positive counterpart.
 *
 * This is FALSE under JLS §4.2.1 (32-bit two's complement arithmetic).
 *
 * The JDK Math.abs(int) body (vendored in vendor/jdk21/java/lang/Math.java):
 *   return (a < 0) ? -a : a;
 *
 * Under two's complement: -Integer.MIN_VALUE == Integer.MIN_VALUE == -2147483648.
 * So abs(-2147483648) = -2147483648, NOT 2147483647.
 *
 * The assertion below encodes the INDUSTRY BELIEF (abs(MIN) = MAX_VALUE).
 * The int32.eq-bv-expr universe row (walked from Math.java) evaluates the
 * BV expression at a=-2147483648 and confirms -2147483648, not 2147483647.
 * Conjoined: UNSAT, consistency unsatisfied.
 *
 * The JDK's OWN test file (AbsTests.java, verbatim in the good suite) asserts
 * the TRUTH — abs(MIN_VALUE) == MIN_VALUE — and it discharges.
 *
 * The refutation comes from the walked body, not from hand-authored arithmetic.
 */
public class AbsFlagshipBadTest {

    @Test
    public void testAbsMinValueIndustryBelief() {
        // THE INDUSTRY BELIEF: abs(MIN_VALUE) should return MAX_VALUE (2147483647).
        // This is false under two's complement. The walked BV expression refutes it.
        assertEquals(2147483647, abs(-2147483648));
    }
}
