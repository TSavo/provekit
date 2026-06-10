// H1 [C8] discrimination: TestNG assertNotEquals(Object,Object) 2-arg form.
// The 2-arg form delegates to 3-arg, which calls assertNotEqualsImpl, which calls
// !areNotEqualImpl — that guard is INEQUALITY semantics (throw when equal).
// Before H1 [C8]: the delta overloads (3-arg float/double) put "assertNotEquals" in
// the APPROX set, which won the post-pass over INEQUALITY — so the 2-arg form was
// falsely refused as "approximate assertion". H1 [C8] fix: areNotEqualImpl is a
// NOT-EQUAL predicate sentinel; !notEqual (where notEqual=areNotEqualImpl) → INEQUALITY.
// This test verifies the 2-arg form lifts as INEQUALITY, not APPROX.
import org.testng.annotations.Test;
import org.testng.Assert;

public class TestNGAssertNotEquals {
    @Test
    public void testGNotEquals() {
        // g(2)=2, expected-to-be-different=3: 2 ≠ 3 holds → assertion passes.
        // Should lift as INEQUALITY contract (not equality, not approx).
        Assert.assertNotEquals(g(2), 3);
    }

    private int g(int x) { return x; }
}
