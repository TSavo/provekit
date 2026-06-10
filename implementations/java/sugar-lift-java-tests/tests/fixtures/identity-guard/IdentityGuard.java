// Fixture (P4.5 test 25): the ==-dispatch discrimination pair.
import org.junit.Test;
import static org.junit.identity.IdentityAssert.assertNotSame;
import static org.junit.identity.IdentityAssert.assertEqualsInt;

public class IdentityGuard {
    private int g(int x) { return x - 1; }
    private Object h(int x) { return "x"; }

    // assertNotSame's guard is reference ==: identity, not value inequality.
    // Lifting it as value-≠ would swear a claim the framework never made.
    @Test
    public void identityMustRefuse() {
        assertNotSame(h(1), h(2));
    }

    // Same guard SHAPE over primitive ints: value equality, lifts.
    @Test
    public void primitiveStillLifts() {
        assertEqualsInt(1, g(2));
    }
}
