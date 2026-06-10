// Fixture (P4.5 test c): calls the impostor assertEquals (body: `return;`).
// Must REFUSE with "no throw locus — not an assertion". A lift here would be
// the falsePass: the test would "pass" while asserting nothing.
import org.junit.Test;
import static org.junit.fake.FakeAssert.assertEquals;

public class NameImpostor {
    @Test
    public void testG() {
        assertEquals(1, g(2));
    }

    private int g(int x) { return x - 1; }
}
