// Fixture: effectively-final positive.
// `outer` is declared before the loop and NEVER reassigned — effectively final.
// The final-oracle MUST NOT treat it as a mutation target, so the loop body
// (which only uses int literals and the loop var) lifts cleanly.
// Expected: 1 forall contract. The effectively-final outer local is irrelevant.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ForLoopEffectivelyFinal {
    @Test
    public void testEffectivelyFinal() {
        int outer = 42; // never reassigned — effectively final (not the keyword)
        for (int x = 0; x < 3; x++) {
            assertEquals(1, g(x)); // body only uses loop var and int literals
        }
    }

    private int g(int v) { return 1; }
}
