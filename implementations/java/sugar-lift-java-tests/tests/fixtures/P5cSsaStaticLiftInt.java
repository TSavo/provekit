// P5c fixture: SSA binding of a STATIC (bare) call → #euf#-federated.
// `int result = compute(7); assertEquals(14, result)` must produce the SAME
// contract name as the inline form `assertEquals(14, compute(7))`.
// The local is effectively-final (no reassignment) → SSA substitution applies.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class P5cSsaStaticLiftInt {

    // SSA form: local effectively-final → substituted to compute(7).
    @Test
    public void testViaLocal() {
        int result = compute(7);
        assertEquals(14, result);
    }

    // Inline form: must produce byte-identical contract name.
    @Test
    public void testInline() {
        assertEquals(14, compute(7));
    }

    private int compute(int x) { return x * 2; }
}
