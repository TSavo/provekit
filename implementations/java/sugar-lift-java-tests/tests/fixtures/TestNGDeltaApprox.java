// Fixture: TestNG delta-approximate assertion MUST be refused (Phase 4).
// assertEquals(double actual, double expected, double delta) has a delta param.
// The VocabDeriver learns from TestNG source: assertEquals(g_d(2), 1.0, 0.5)
// → delta param → APPROXIMATE → refused at lift time.
// Expected: 0 contracts, 1 refusal naming "approximate assertion (delta)".
import org.testng.annotations.Test;
import org.testng.Assert;

public class TestNGDeltaApprox {
    @Test
    public void testApproximate() {
        Assert.assertEquals(g_d(2), 1.0, 0.5);
    }

    private double g_d(int x) { return x; }
}
