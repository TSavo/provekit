// GOOD suite: TestNG assertion consistency (Phase 4 showcase).
// Both tests assert Assert.assertEquals(g(2), 1).
// TestNG order: actual=arg[0], expected=arg[1].
// Learned from vendored Assert.java: param[0]="actual" → expectedArgIndex=1.
// Claim: =(call:g(2), 1) asserted twice → consistent → discharged.
import org.testng.annotations.Test;
import org.testng.Assert;

public class TestNGConsistencyTest {

    @Test
    public void testGEqualOne_first() {
        // TestNG order: assertEquals(actual, expected)
        Assert.assertEquals(g(2), 1);
    }

    @Test
    public void testGEqualOne_second() {
        // Same claim from a second test — cross-test consistency: discharged.
        Assert.assertEquals(g(2), 1);
    }

    private int g(int x) { return x; }
}
