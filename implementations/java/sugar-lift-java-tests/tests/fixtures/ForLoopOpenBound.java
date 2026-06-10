// Fixture: non-literal upper bound → refused (open forall).
// for (int x = 0; x < n; x++) — `n` is a variable, not an int literal.
// An open forall would be unsound; we refuse by name.
// Expected: 0 contracts, 1 refusal naming "open upper bound".
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ForLoopOpenBound {
    @Test
    public void testOpenBound() {
        int n = 3; // never used as a literal in the condition — still refused
        for (int x = 0; x < n; x++) {
            assertEquals(1, g(x));
        }
    }

    private int g(int v) { return 1; }
}
