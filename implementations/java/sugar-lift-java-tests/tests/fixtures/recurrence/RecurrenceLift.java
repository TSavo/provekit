// Fixture consumer: drives the lift pass so the RecurrenceUniverseWalker runs
// over vendor_source_dirs. The walked recurrence FOL (and any named refusals)
// surface as diagnostics on the lift result — that is what the kit test asserts.
// This file's own assertions are incidental; the keystone is the vendor walk.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class RecurrenceLift {
    @Test
    public void testDrives() {
        // A trivial sworn equality so the file is a valid @Test target.
        assertEquals(1, ident(1));
    }
    private int ident(int x) { return x; }
}
