// Fixture: G1 universe-walk lifting.
// Each string-expected assertion over a universe-registered callee lifts TWO
// contracts under the SAME #euf# name: the sworn equality and the walked
// str.chars-in-set universe row.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class UniverseLift {

    @Test
    public void testUpper() {
        // upper chain: encodeUpper -> encode(data, false) -> selector false
        // branch -> UPPER_TABLE; pad guard attributes '=' to UPPER_TABLE only.
        assertEquals("AB=", encodeUpper("x".getBytes()));
    }

    @Test
    public void testLower() {
        // lower chain: selector true branch -> LOWER_TABLE; NO pad.
        assertEquals("ab", encodeLower("y".getBytes()));
    }

    @Test
    public void testBytesUtf8Shape() {
        // the getBytesUtf8("lit") arg shape also lifts.
        assertEquals("AB", encodeUpper(getBytesUtf8("z")));
    }

    @Test
    public void testNonLiteralArgRefused() {
        byte[] data = "w".getBytes();
        // non-literal call arg: still refused by name, no contract.
        assertEquals("AB", encodeUpper(data));
    }
}
