package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: two assertions about the same callsite g(2)==1 are consistent.
 * Both claims say g(2)==1, so the lifted #euf# contracts are satisfiable (same).
 */
public class ConsistencyTest {

    @Test
    public void testGFirst() {
        assertEquals(1, g(2));
    }

    @Test
    public void testGSecond() {
        // Same callsite, same expected value — consistent with testGFirst.
        assertEquals(1, g(2));
    }

    private int g(int x) { return x - 1; }
}
