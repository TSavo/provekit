// H1 [B6] discrimination: entry point with lineLength=0 lifts; lineLength=76 refused.
// encodeNoChunk is the sound universe entry point (lineLength=0, no separator injection).
// encodeChunked propagates lineLength=76 → separator chars in output → MUST be refused.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class ChunkedLift {

    @Test
    public void testNoChunk() {
        // lineLength=0 path: no separators → str.chars-in-set(ABCD) is sound.
        assertEquals("ABCD", encodeNoChunk("x".getBytes()));
    }

    @Test
    public void testChunked() {
        // lineLength=76 path: output contains \r\n → universe contract WRONG.
        // VocabDeriver must refuse encodeChunked in the universe registry.
        assertEquals("ABCD", encodeChunked("x".getBytes()));
    }
}
