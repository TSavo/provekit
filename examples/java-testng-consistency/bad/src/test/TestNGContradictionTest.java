// BAD suite: TestNG assertion contradiction (Phase 4 showcase).
// THE PROOF OF THE MECHANISM: same callsite g(2), two contradicting expected values.
// Assert.assertEquals(g(2), 1): claim =(call:g(2), 1)
// Assert.assertEquals(g(2), 2): claim =(call:g(2), 2)
// Both share the contract name g#euf#c:callresult_g_a1(i:2)::assertion.
// The conjoin of =(g(2),1) ∧ =(g(2),2) is UNSAT → consistency row: unsatisfied.
import org.testng.annotations.Test;
import org.testng.Assert;

public class TestNGContradictionTest {

    @Test
    public void testGEqualOne() {
        // TestNG order: actual first. Claim: =(call:g(2), 1)
        Assert.assertEquals(g(2), 1);
    }

    @Test
    public void testGEqualTwo() {
        // TestNG order: actual first. Claim: =(call:g(2), 2)
        // Combined with testGEqualOne: =(g(2),1) ∧ =(g(2),2) → UNSAT.
        Assert.assertEquals(g(2), 2);
    }

    private int g(int x) { return x; }
}
