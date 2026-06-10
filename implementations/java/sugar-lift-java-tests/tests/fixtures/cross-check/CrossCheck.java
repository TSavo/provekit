// Fixture (P4.5 test e): calls the disagreement assertEquals.
// Must REFUSE (unlearned) — the deriver could not reconcile guard positions
// with parameter names, so the order is not trustworthy.
import org.junit.Test;
import static org.junit.disagree.DisagreeAssert.assertEquals;

public class CrossCheck {
    @Test
    public void testG() {
        assertEquals(g(2), 1);
    }

    private int g(int x) { return x - 1; }
}
