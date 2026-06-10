// Fixture: assertNotEquals + assertNull + assertNotNull via learned vocabulary.
// All three must produce contracts (not diagnostics) when the vocab is configured.
// Used to verify the extended lift coverage in Phase 2.
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertNotNull;

public class NotEqualsAndNull {

    @Test
    public void testNotEquals() {
        // assertNotEquals(2, g(2)) -> ≠(g(2), 2)
        assertNotEquals(2, g(2));
    }

    @Test
    public void testNull() {
        // assertNull(getNull(5)) -> =(getNull(5), None)
        assertNull(getNull(5));
    }

    @Test
    public void testNotNull() {
        // assertNotNull(getNotNull(3)) -> ≠(getNotNull(3), None)
        assertNotNull(getNotNull(3));
    }

    private int g(int x) { return 1; }
    private Object getNull(int x) { return null; }
    private Object getNotNull(int x) { return x; }
}
