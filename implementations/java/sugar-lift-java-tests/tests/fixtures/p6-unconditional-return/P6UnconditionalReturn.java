/*
 * @test
 * @summary P6 discrimination fixture: harness returns 1 unconditionally.
 *          No 'result != expected' guard is present.
 *          The kit MUST refuse with a named diagnostic — NOT classify.
 *          Soundness: classifying this would produce a false-pass on any input.
 */

import java.util.function.IntUnaryOperator;

public class P6UnconditionalReturn {

    /**
     * DISCRIMINATION: returns 1 unconditionally — no guard on result.
     * Body shape does NOT match error-sentinel pattern (no 'result != expected' if).
     * Kit must refuse this with: "no 'result != expected' guard found".
     */
    static int badHarness(IntUnaryOperator absFunc, int argument, int expected) {
        int result = absFunc.applyAsInt(argument);
        // No if (result != expected) — unconditional failure return.
        return 1;
    }

    public static void main(String[] args) {
        int errors = 0;
        errors += badHarness(Math::abs, Integer.MIN_VALUE, Integer.MIN_VALUE);
        if (errors > 0) {
            throw new RuntimeException(errors + " test(s) failed");
        }
    }
}
