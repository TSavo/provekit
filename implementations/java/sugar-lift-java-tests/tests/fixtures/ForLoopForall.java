// Fixture: bounded for-loop → forall contract.
// for (int x = 0; x < 3; x++) { assertEquals(1, g(x)); }
// Expected: 1 forall contract with name containing "::loop::x".
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ForLoopForall {
    @Test
    public void testGOnRange() {
        for (int x = 0; x < 3; x++) {
            assertEquals(1, g(x));
        }
    }

    private int g(int v) { return 1; }
}
