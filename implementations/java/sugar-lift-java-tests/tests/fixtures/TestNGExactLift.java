// Fixture: TestNG exact-lift case (Phase 4).
// Assert.assertEquals(g(2), 1) in a TestNG-importing file.
// TestNG parameter order: assertEquals(actual, expected) — actual is arg[0].
// The VocabDeriver learns this from vendored TestNG Assert.java.
// Expected: 1 contract for g#euf#... with inv =(call:g(2), 1)
// This IR must be byte-identical to the JUnit fixture for assertEquals(1, g(2)).
import org.testng.annotations.Test;
import org.testng.Assert;

public class TestNGExactLift {
    @Test
    public void testG() {
        Assert.assertEquals(g(2), 1);
    }

    private int g(int x) { return x; }
}
