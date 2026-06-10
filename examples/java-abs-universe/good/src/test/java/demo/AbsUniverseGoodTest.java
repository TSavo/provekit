package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static java.lang.Math.abs;

/**
 * GOOD suite: the industry-confounding truth.
 *
 * The industry belief is abs(x) >= 0.  Under JLS §4.2.1, Java int is 32-bit
 * two's complement.  Integer.MIN_VALUE is -2147483648.  The JDK Math.abs(int)
 * body (jdk-21+35, vendor/jdk21/java/lang/Math.java) is letter-for-letter:
 *
 *     return (a < 0) ? -a : a;
 *
 * Under two's complement: -Integer.MIN_VALUE == Integer.MIN_VALUE.
 * Therefore: abs(-2147483648) == -2147483648.
 * Nobody believes it.  Sugar proves it.
 *
 * The kit lifts TWO contracts under the SAME #euf# name:
 *   1. the sworn equality    =(abs(-2147483648), -2147483648)
 *   2. the universe row      int32.eq-bv-expr(abs(-2147483648),
 *                              bv32.ite(bv32.slt(a,0), bv32.neg(a), a))
 *
 * The BV expression is walked letter-for-letter from the vendor's AST — no
 * arithmetic is hand-authored here.  The conjunction is discharged by z3's
 * bitvector theory: the walked body evaluates to MIN_VALUE at a=MIN_VALUE.
 */
public class AbsUniverseGoodTest {

    @Test
    public void testAbsMinValue() {
        // abs(MIN_VALUE) == MIN_VALUE under two's complement — JDK Math.abs body walked.
        // The industry belief (abs >= 0) is false for this input.
        assertEquals(-2147483648, abs(-2147483648));
    }

    @Test
    public void testAbsPositive() {
        // Normal case: abs(5) == 5 (the walked body confirms).
        assertEquals(5, abs(5));
    }

    @Test
    public void testAbsNegative() {
        // abs(-5) == 5 (the walked body: -5 < 0, so return -(-5) = 5).
        assertEquals(5, abs(-5));
    }
}
