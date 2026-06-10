// Fixture: JUnit reference for IR byte-identity comparison with TestNG (Phase 4).
// assertEquals(1, g(2)) in JUnit order: expected=1 (arg[0]), actual=g(2) (arg[1]).
// This must produce IDENTICAL IR to TestNG: Assert.assertEquals(g(2), 1).
// Both claims: =(call:g(2), 1)
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class JUnitEqualityRef {
    @Test
    public void testGEqualOne() {
        assertEquals(1, g(2));
    }

    private int g(int x) { return x; }
}
