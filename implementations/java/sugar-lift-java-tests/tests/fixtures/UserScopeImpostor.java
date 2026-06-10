// H1 [A3] discrimination: user-defined assertEquals with no framework static import.
// This file defines its OWN assertEquals method (empty body, no throw) and calls it.
// There is NO `import static org.junit.Assert.assertEquals` — so the bare call is
// NOT structurally bound to any framework. VocabDeriver must NOT lift this as EQUALITY.
// A lift here would be a falsePass: the method asserts nothing (empty body).
// Without H1 [A3], vocab.isKnown("assertEquals") was true → the call would lift.
// With H1 [A3], the import-edge guard (isBareFrameworkBound check) rejects it silently.
import org.junit.Test;

public class UserScopeImpostor {
    // User-scope override: same name as JUnit assertEquals but NOT from the framework.
    // Empty body — no throw locus. Calling this asserts nothing.
    private void assertEquals(int expected, int actual) {
        // intentionally empty — user-scope impostor
    }

    @Test
    public void testG() {
        // This call is NOT covered by a static import from org.junit.Assert.
        // The import-edge guard must block it: 0 contracts, 0 refusals (silent skip).
        assertEquals(2, g(2));
    }

    private int g(int x) { return x; }
}
