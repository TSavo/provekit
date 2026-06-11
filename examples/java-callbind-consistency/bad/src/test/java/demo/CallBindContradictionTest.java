package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: P5c call-binding contradiction.
 *
 * Two assertions about the SAME constructed receiver's same call:
 *   Codec codec = new Codec();
 *   int r = codec.encode(42);
 *   assertEquals(42, r);    // ← one test
 *   assertEquals(99, r);    // ← another test (different method)
 *
 * Both assertions reference the same local `codec` receiver and same
 * method `encode(42)` bound to `r`. Because both use the same local
 * receiver name `codec` in the SAME test method, they are within-scope
 * and the location-keyed contract captures BOTH: =(encode(codec,42),42)
 * AND =(encode(codec,42),99) → unsatisfied (within-test contradiction).
 */
public class CallBindContradictionTest {

    @Test
    public void testContradiction() {
        Codec codec = new Codec();
        int r = codec.encode(42);
        // Two contradictory assertions about the same call via SSA local `r`.
        assertEquals(42, r);
        assertEquals(99, r);
    }

    static class Codec {
        int encode(int x) { return x; }
    }
}
