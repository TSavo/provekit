// Fixture for java-witness-recompute BAD suite.
// testGReturnsOne passes; testGFails deliberately fails.
// The bundle still reproduces honestly (both outcomes recorded),
// but the verifier REFUSES because not all outcomes are "passed".
public class WitnessTest {
    static int g(int x) { return 1; }

    public void testGReturnsOne() {
        if (g(0) != 1) throw new AssertionError("g(0) != 1");
    }

    // This test deliberately fails to demonstrate witness refusal.
    public void testGFails() {
        throw new AssertionError("intentional failure: witness should be refused");
    }
}
