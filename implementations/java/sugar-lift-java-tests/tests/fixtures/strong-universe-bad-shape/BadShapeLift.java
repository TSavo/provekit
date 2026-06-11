// Fixture: assertion over a codec whose encode index contains a METHOD CALL.
// The weak charset row lifts; the strong row is REFUSED BY NAME (the symbolic
// store cannot interpret scramble(ibitWorkArea) inside the index expression).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class BadShapeLift {
    @Test
    public void testBadShape() {
        // "bar".getBytes() is the accepted string-literal-bytes shape.
        assertEquals("YmFy", encodeString("bar".getBytes()));
    }
}
