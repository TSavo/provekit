// Fixture: delta-approximate assertion MUST be refused, never lifted.
// assertEquals(1.0f, f(2), 0.5f) has a delta/tolerance parameter;
// lifting it as exact = would be a false-pass.
// The kit must produce ZERO contracts and at least one diagnostic naming
// the "approximate assertion (delta)" refusal reason.
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.assertEquals;

public class DeltaApprox {

    @Test
    public void testApproximate() {
        // This is an approximate assertion — delta=0.5f means "|f(2) - 1.0| <= 0.5".
        // It must NOT be lifted as exact = (that would be a false-pass).
        assertEquals(1.0f, f(2), 0.5f);
    }

    private float f(int x) { return x; }
}
