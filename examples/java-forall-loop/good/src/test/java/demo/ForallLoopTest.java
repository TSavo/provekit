package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: a bounded for-loop lifts to the guarded universal
 * forall x. (0 <= x < 3 => g(x) == 1).
 * The universal alone is consistent — all consistency rows discharged.
 */
public class ForallLoopTest {

    @Test
    public void testGOnRange() {
        // Lifts to: forall x:Int. (0 <= x AND x < 3) => g(x) == 1
        for (int x = 0; x < 3; x++) {
            assertEquals(1, g(x));
        }
    }

    static int g(int v) { return 1; }
}
