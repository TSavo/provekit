/*
 * @test
 * @summary P6 discrimination fixture: sentinel never reaches a throw in main.
 *          The harness body shape IS correct (result != expected guard, positive return).
 *          But main has no 'if (errors > 0) throw' — so the sentinel is unobservable.
 *          The kit MUST refuse with a named diagnostic — NOT emit contracts.
 *          Soundness: a harness whose failures are silently swallowed cannot prove anything.
 */

import java.util.function.IntUnaryOperator;

public class P6NoThrow {

    /**
     * BODY SHAPE IS CORRECT — this would normally be classified.
     * But main has no accumulator+throw, so the sentinel is unobservable.
     */
    static int testIntAbs(IntUnaryOperator absFunc, int argument, int expected) {
        int result = absFunc.applyAsInt(argument);
        if (result != expected) {
            return 1;
        }
        return 0;
    }

    public static void main(String[] args) {
        int errors = 0;
        errors += testIntAbs(Math::abs, Integer.MIN_VALUE, Integer.MIN_VALUE);
        // MISSING: no 'if (errors > 0) throw ...' — sentinel is silently swallowed.
        System.out.println("Errors: " + errors);
    }
}
