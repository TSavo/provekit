// Fixture: discrimination case.
// assertEquals with a string literal argument must produce a lift-gap, NOT a contract.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class StringArgDiscrimination {
    @Test
    public void testStringEq() {
        // This must be REFUSED by name, not lifted.
        assertEquals("hello", compute("world"));
    }

    @Test
    public void testVarArg() {
        int x = 5;
        // x is not an int literal → lift-gap
        assertEquals(x, compute2(1));
    }

    private String compute(String s) { return s; }
    private int compute2(int n) { return n; }
}
