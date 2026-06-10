package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: the bounded loop lifts to forall x. (0<=x<3 => g(x)==1),
 * and a separate test asserts g(2)==2. The engine instantiates the universal
 * at x=2 → g(2)==1, which contradicts the point-claim g(2)==2.
 * The ambient-forall rule in the engine produces `unsatisfied`.
 *
 * This is the cross-contract federation proof: the forall from one test
 * refutes the point-claim from another test — no kit code needed.
 */
public class ForallLoopContradictionTest {

    @Test
    public void testGOnRange() {
        // Lifts to: forall x:Int. (0 <= x AND x < 3) => g(x) == 1
        for (int x = 0; x < 3; x++) {
            assertEquals(1, g(x));
        }
    }

    @Test
    public void testGAtTwoIsTwo() {
        // Contradicts the universal at x=2: the engine instantiates
        // forall x. g(x)==1 at x=2 → g(2)==1, but this says g(2)==2.
        assertEquals(2, g(2));
    }

    static int g(int v) { return 1; }
}
