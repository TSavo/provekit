// Fixture: G2b comparison-bound lifting.
// assertTrue(callExpr <op> intLiteral) → comparison atom over the #euf# callsite name.
// assertFalse(callExpr <op> intLiteral) → negated comparison atom.
// The #euf# name is IDENTICAL to the one assertEquals would produce for the same callsite —
// federation happens automatically at prove time.
import org.junit.Test;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.assertFalse;

public class ComparisonBoundLift {

    // Defined so the call lifts (bare function, int-literal arg).
    static int g(int v) { return 3; }

    @Test
    public void testLessThanCallLeft() {
        // assertTrue(g(7) < 10)  →  <(call:g(7), 10)
        assertTrue(g(7) < 10);
    }

    @Test
    public void testLessThanLitLeft() {
        // assertTrue(5 > g(2))  →  >(call:g(2), 5)  [lit on left, mirrored]
        assertTrue(5 > g(2));
    }

    @Test
    public void testAssertFalse() {
        // assertFalse(g(2) < 5)  →  ¬(g(2) < 5)  ≡  >=(call:g(2), 5)
        assertFalse(g(2) < 5);
    }

    @Test
    public void testGreaterThanEqual() {
        // assertTrue(g(3) >= 0)  →  >=(call:g(3), 0)
        assertTrue(g(3) >= 0);
    }
}
