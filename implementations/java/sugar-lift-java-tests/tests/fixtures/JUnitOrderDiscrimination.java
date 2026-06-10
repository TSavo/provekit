// Fixture: ORDER DISCRIMINATION (Phase 4).
// The same source text "assertEquals(g(2), 1)" placed in a JUnit-importing file.
// JUnit order: expected=arg[0], actual=arg[1].
// arg[0] = g(2) is NOT an int literal → first arg is not an int literal → REFUSED.
// In TestNG file: actual=arg[0], expected=arg[1] → const=1 (arg[1]) → LIFTS.
// This asymmetry is the proof that vocab must be learned per-framework.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class JUnitOrderDiscrimination {
    @Test
    public void testG() {
        assertEquals(g(2), 1);
    }

    private int g(int x) { return x; }
}
