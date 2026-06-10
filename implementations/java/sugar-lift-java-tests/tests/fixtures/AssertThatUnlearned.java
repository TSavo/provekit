// Fixture: assertThat → unlearned named refusal (Phase 4).
// assertThat is Hamcrest/AssertJ style — NOT in either JUnit's or TestNG's Assert
// as a direct method. Must produce a named refusal.
// Expected: 0 contracts, 1 diagnostic naming "no learned vocabulary" or
// "assertion not in learned vocabulary; refused by name: assertThat"
import org.junit.Test;
import static org.junit.Assert.assertThat;

public class AssertThatUnlearned {
    @Test
    public void testThat() {
        assertThat(g(2), org.hamcrest.CoreMatchers.equalTo(1));
    }

    private int g(int x) { return x; }
}
