// H1 [A2] negative: wildcard static import of a completely non-framework namespace.
// `import static com.example.assertions.CustomAssert.*` is not org.junit.* or org.testng.*;
// it is not processed by the framework import scanner at all.
// The bare call to assertEquals must NOT be lifted (not framework-bound).
// This is the same silent-skip outcome as any user-scope method call without a
// framework static import: the import-edge guard rejects it without a refusal.
import org.junit.Test;
import static com.example.assertions.CustomAssert.*;

public class WildcardUnvendoredRefusal {
    @Test
    public void testG() {
        // assertEquals from com.example — NOT a JUnit/TestNG import.
        // Must NOT produce a contract (not framework-bound, silent skip).
        assertEquals(2, g(2));
    }

    private int g(int x) { return x; }
}
