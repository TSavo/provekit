// Fixture: exact-lift case.
// assertEquals(2, g(2)) inside @Test → should produce 1 contract for g#euf#...
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ExactLift {
    @Test
    public void testG() {
        assertEquals(2, g(2));
    }

    @Test
    public void testH() {
        assertEquals(0, h(-1, 3));
    }

    private int g(int x) { return x; }
    private int h(int a, int b) { return a + b + 1; }
}
