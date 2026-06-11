/*
 * @test
 * @summary P6 discrimination fixture: functional-interface argument is not a method reference.
 *          The harness body shape IS correct, main accumulator+throw IS present.
 *          But the callsite passes a lambda (not a method reference) as the function param.
 *          The kit MUST refuse with a named diagnostic — method-ref resolution requires
 *          MemberReferenceTree; lambda expressions are not resolvable without execution.
 *          Soundness: we cannot determine which function is being called.
 */

import java.util.function.IntUnaryOperator;

public class P6MethodRefRefusal {

    /**
     * CORRECT SHAPE — would classify if callsite used a method reference.
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
        // DISCRIMINATION: lambda instead of method reference — cannot resolve callee statically.
        // Kit must refuse: "functional-interface argument is not a method reference".
        errors += testIntAbs(a -> a < 0 ? -a : a, Integer.MIN_VALUE, Integer.MIN_VALUE);
        if (errors > 0) {
            throw new RuntimeException(errors + " test(s) failed");
        }
    }
}
