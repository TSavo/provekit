// Fixture: assertion over a callee whose delegation chain ESCAPES vendored
// source. Expected: equality contract lifts; NO universe row; named refusal.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class EscapeLift {

    @Test
    public void testUpper() {
        assertEquals("AB=", encodeUpper("x".getBytes()));
    }
}
