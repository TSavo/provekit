package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: two assertions about the same callsite g(2) contradict.
 * One claims g(2)==1, the other g(2)==2.
 * The lifted #euf# contracts conjoin to: g(2)==1 AND g(2)==2, which is
 * unsatisfiable (EUF: a function returns one value for given arguments).
 */
public class ContradictionTest {

    @Test
    public void testGFirst() {
        assertEquals(1, g(2));
    }

    @Test
    public void testGSecond() {
        // CONTRADICTION: same callsite, different expected value.
        assertEquals(2, g(2));
    }

    private int g(int x) { return x; }
}
