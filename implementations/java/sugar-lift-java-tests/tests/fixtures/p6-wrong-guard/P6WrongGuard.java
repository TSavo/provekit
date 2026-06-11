/*
 * @test
 * @summary P6 discrimination fixture: guard compares unrelated values.
 *          The if-guard uses a constant vs constant comparison, NOT result vs expected.
 *          The kit MUST refuse with a named diagnostic — NOT classify.
 *          Soundness: the result local is never compared to the expected param.
 */

import java.util.function.IntUnaryOperator;

public class P6WrongGuard {

    /**
     * DISCRIMINATION: guard checks an unrelated condition (0 != 1), NOT result vs expected.
     * The kit must refuse: "no 'result != expected' guard found".
     */
    static int badHarness(IntUnaryOperator absFunc, int argument, int expected) {
        int result = absFunc.applyAsInt(argument);
        // Guard does NOT compare result to expected — compares unrelated constants.
        if (0 != 1) {
            System.err.println("Wrong guard: this is never a failure sentinel");
            return 1;
        }
        return 0;
    }

    public static void main(String[] args) {
        int errors = 0;
        errors += badHarness(Math::abs, Integer.MIN_VALUE, Integer.MIN_VALUE);
        if (errors > 0) {
            throw new RuntimeException(errors + " test(s) failed");
        }
    }
}
