// Fixture: G2 numeric-universe-walk lifting.
// Each int-expected assertion over a numeric-universe-registered callee lifts TWO
// contracts under the SAME #euf# name: the sworn equality and the walked
// int32.eq-bv-expr universe row.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class NumericUniverseLift {

    @Test
    public void testAbsTruth() {
        // ABS TRUTH: abs(MIN_VALUE) == MIN_VALUE == -2147483648 (two's complement)
        // This is what the walked body says. Nobody believes it. Sugar proves it.
        assertEquals(-2147483648, abs(-2147483648));
    }

    @Test
    public void testAbsPositive() {
        // A normal case: abs(5) == 5 (the walked body confirms this)
        assertEquals(5, abs(5));
    }

    @Test
    public void testAbsNegative() {
        // abs(-5) == 5 (the walked body confirms: -5 < 0, so return -(-5) = 5)
        assertEquals(5, abs(-5));
    }
}
