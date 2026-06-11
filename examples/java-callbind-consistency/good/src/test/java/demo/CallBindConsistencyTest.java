package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: P5c call-binding consistency.
 *
 * Tests the dominant real-vendor shape:
 *   String encoded = encode(42);
 *   assertEquals("expected", encoded);
 *
 * The local `encoded` is effectively-final (single-assignment).
 * The kit substitutes it back to `encode(42)` and emits a location-keyed
 * ::assertion contract (instance-method on `codec`).
 *
 * Both tests assert consistent values about their respective receiver's
 * method → discharged.
 */
public class CallBindConsistencyTest {

    @Test
    public void testDefaultCodecEncode() {
        Codec codec = new Codec(0);
        int result = codec.encode(42);
        assertEquals(42, result);
    }

    @Test
    public void testStrictCodecEncode() {
        Codec codec = new Codec(1);
        int result = codec.encode(42);
        assertEquals(42, result);
    }

    static class Codec {
        private final int mode;
        Codec(int mode) { this.mode = mode; }
        int encode(int x) { return x; }
    }
}
