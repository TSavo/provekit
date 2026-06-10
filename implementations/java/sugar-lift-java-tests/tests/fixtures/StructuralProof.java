// Fixture: structural proof that the parser is real.
// A string literal containing assertEquals(1, g(1)) inside a NON-test method,
// and a comment: // assertEquals(3, g(3))
// A string-scanner would lift both. The com.sun.source tree-walker MUST emit NEITHER.
// The real @Test method has a valid assertion to confirm the kit still works.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class StructuralProof {

    // This is not a test method — its assertions must NOT be lifted
    public void notATest() {
        String s = "assertEquals(1, g(1))";  // inside a string literal, not real code
        // assertEquals(3, g(3))              // inside a comment, not real code
        System.out.println(s);
    }

    @Test
    public void testReal() {
        // This is the only liftable assertion: assertEquals(7, g(7))
        assertEquals(7, g(7));
    }

    private int g(int x) { return x; }
}
