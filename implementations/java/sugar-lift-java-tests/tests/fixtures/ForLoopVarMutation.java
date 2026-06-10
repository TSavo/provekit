import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.assertEquals;

class ForLoopVarMutation {
    static int g(int v) { return 1; }

    // The body mutates the loop variable inside an assert arg: the iteration
    // space is not the stated range, so the universal would be a false claim.
    // Must be REFUSED by the loop-variable mutation gate (not merely by the
    // arg-shape gate).
    @Test
    void testLoopVarMutated() {
        for (int x = 0; x < 3; x++) {
            assertEquals(1, g(x++));
        }
    }
}
