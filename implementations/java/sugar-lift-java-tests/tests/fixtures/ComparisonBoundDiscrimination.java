// Fixture: G2b comparison-bound discrimination.
// All shapes that must be REFUSED by name (named diagnostic, not silent).
import org.junit.Test;
import static org.junit.Assert.assertTrue;

public class ComparisonBoundDiscrimination {

    static int g(int v) { return 3; }
    static int h(int v) { return 5; }

    // Non-literal bound: assertTrue(g(2) < n)
    // n is a local variable — open bound, refused by name.
    @Test
    public void testNonLiteralBound() {
        int n = 10;
        assertTrue(g(2) < n);
    }

    // Both operands are calls: assertTrue(g(2) < h(3))
    // Two callsites, not a bound — refused by name.
    @Test
    public void testBothOperandsCalls() {
        assertTrue(g(2) < h(3));
    }
}
