// Fixture for java-witness-recompute GOOD suite.
// Both test methods pass: the witness package discharges.
public class WitnessTest {
    // Function under test: always returns 1.
    static int g(int x) { return 1; }

    public void testGReturnsOne() {
        if (g(0) != 1) throw new AssertionError("g(0) != 1");
        if (g(1) != 1) throw new AssertionError("g(1) != 1");
    }

    public void testGIsConstant() {
        for (int x = 0; x < 5; x++) {
            if (g(x) != 1) throw new AssertionError("g(" + x + ") != 1");
        }
    }
}
