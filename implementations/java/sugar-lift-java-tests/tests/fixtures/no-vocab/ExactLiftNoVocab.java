// Fixture: the hardcode is gone — without configured vocab, assertEquals is refused.
// This is the "no-vocab-configured discrimination" test (Phase 2 test c).
// With no .sugar/config.toml in this workspace, the kit has no learned vocabulary.
// assertEquals must be refused by name, NOT lifted. Zero contracts expected.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ExactLiftNoVocab {
    @Test
    public void testG() {
        assertEquals(2, g(2));
    }

    private int g(int x) { return x; }
}
