// H1 [A2] positive: wildcard static import expands to all vendored vocab methods.
// `import static org.junit.Assert.*` must bind assertEquals (and all other
// JUnit assertion names) so that the call lifts exactly as the named-import twin.
// Discrimination: WITHOUT the H1 [A2] fix, wildcard imports were not expanded;
// assertEquals would be "not in assertionBoundNames" and silently skipped.
import org.junit.Test;
import static org.junit.Assert.*;

public class WildcardImportLift {
    @Test
    public void testG() {
        assertEquals(2, g(2));
    }

    private int g(int x) { return x; }
}
