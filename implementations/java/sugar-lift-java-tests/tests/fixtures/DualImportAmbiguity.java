// Fixture: dual-import ambiguity (Phase 4).
// Imports BOTH org.junit.Assert and org.testng.Assert.
// The vocabulary for assertEquals is ambiguous (JUnit says expected-first,
// TestNG says actual-first) → named refusal, no contract emitted.
import org.junit.Test;
import org.junit.Assert;
import org.testng.annotations.Test;
import org.testng.Assert;

public class DualImportAmbiguity {
    @Test
    public void testG() {
        assertEquals(g(2), 1);
    }

    private int g(int x) { return x; }
}
