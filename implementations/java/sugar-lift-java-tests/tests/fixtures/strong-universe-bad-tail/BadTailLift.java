// Fixture: a 2-byte (mod-3 tail) input to a codec whose tail extraction index
// contains a METHOD CALL. The symbolic interpreter cannot walk the tail index,
// so the strong row is REFUSED BY NAME (modulus-2 tail). The weak charset row
// STILL emits (the table walk does not read the index).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class BadTailLift {
    @Test
    public void testBadTail() {
        // "ba" = 2-byte tail; the tail extraction wraps the work area in
        // scramble(...), which the index interpreter refuses.
        assertEquals("YmE=", encodeString("ba".getBytes()));
    }
}
