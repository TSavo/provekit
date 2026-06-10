// Fixture (P4.5 test d): assertTrue classified TRUTH purely from the guard
// `if (!condition) failNotTrue(...)` in vendored AssertTrue.java — there is
// no name rule left that could have classified it.
import org.junit.Test;
import static org.junit.Assert.assertTrue;

public class TruthLift {
    @Test
    public void testP() {
        assertTrue(p(2));
    }

    private boolean p(int x) { return x > 0; }
}
