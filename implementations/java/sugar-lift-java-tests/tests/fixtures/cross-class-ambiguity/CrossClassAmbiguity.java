// H1 [A1] cross-class ambiguity — test source.
// checkEq delegates to helperChk which exists in BOTH AssertA and AssertB
// (same arity, opposite guard semantics). VocabDeriver must classify checkEq
// as UNLEARNED ("ambiguous delegation target"), never lift it as EQUALITY or
// INEQUALITY (either would be a falsePass).
import org.junit.Test;
import static com.provekit.fixture.Assertions.checkEq;

public class CrossClassAmbiguity {
    @Test
    public void testG() {
        checkEq(1, g(2));
    }

    private int g(int x) { return x - 1; }
}
