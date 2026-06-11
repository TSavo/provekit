/*
 * @test
 * @summary P6 fixture: minimal faithful jtreg error-sentinel harness.
 *          testIntAbs is classified by body shape — NOT by name.
 *          Math::abs resolved via MemberReferenceTree.
 *          Integer.MIN_VALUE resolved via platform-axioms.json (JLS §4.2.1).
 */

import java.util.function.IntUnaryOperator;
import static java.lang.Math.abs;

public class P6JtregPositive {

    /**
     * Error-sentinel harness. Body shape (NOT name) triggers P6 classification:
     *   result = funcParam.applyAsInt(argument)
     *   if (result != expected) { return 1; }
     *   else { return 0; }
     */
    static int testIntAbs(IntUnaryOperator absFunc, int argument, int expected) {
        int result = absFunc.applyAsInt(argument);
        if (result != expected) {
            System.err.println("FAIL: abs(" + argument + ")=" + result + " expected " + expected);
            return 1;
        }
        return 0;
    }

    public static void main(String[] args) {
        int errors = 0;
        // abs(MIN_VALUE) == MIN_VALUE: "// Strange but true" (JDK comment verbatim)
        errors += testIntAbs(Math::abs, Integer.MIN_VALUE, Integer.MIN_VALUE);
        if (errors > 0) {
            throw new RuntimeException(errors + " test(s) failed");
        }
    }
}
