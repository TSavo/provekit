// P5c fixture: REASSIGNED local → refused by name (not a stable SSA alias).
// `String e = encode("hello"); e = "other"; assertEquals("SGVsbG8=", e)` must refuse
// because `e` is reassigned — it is NOT a stable alias for its initializer call.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class P5cReassignedRefusal {

    @Test
    public void testReassignedLocal() {
        String e = encode("hello");
        e = "overwritten";  // reassignment → e is no longer a stable alias
        assertEquals("SGVsbG8=", e);
    }

    private String encode(String s) { return s; }
}
