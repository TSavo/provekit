// Fixture: assertion over a callee whose table is MUTABLE.
// Expected: equality contract lifts; NO universe row; named refusal.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class MutableLift {

    @Test
    public void testUpper() {
        assertEquals("AB=", encodeUpper("x".getBytes()));
    }
}
