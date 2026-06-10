// Fixture: accumulator discrimination — loop body mutates `acc`.
// The loop is NOT a universal: acc varies independently of x.
// Expected: loop REFUSED by name (accumulator pattern).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ForLoopAccumulator {
    @Test
    public void testAccumulator() {
        int acc = 0;
        for (int x = 0; x < 3; x++) {
            acc += g(x);
            assertEquals(1, g(x));
        }
    }

    private int g(int v) { return 1; }
}
