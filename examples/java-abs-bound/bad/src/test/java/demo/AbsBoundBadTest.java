package demo;

import org.junit.Test;
import static org.junit.Assert.assertTrue;
import static java.lang.Math.abs;

/**
 * G2b × G2 cross-feature: comparison bound meets walked numeric universe.
 *
 * THE SHOWCASE: no vendor test ever wrote `assertTrue(abs(MIN_VALUE) >= 0)`.
 * Yet Sugar can adjudicate it — because:
 *
 *   1. G2 walks Math.abs(int)'s body `(a < 0) ? -a : a` and emits:
 *        int32.eq-bv-expr(call:abs(MIN_VALUE), bv32.ite(bv32.slt(a,0), bv32.neg(a), a))
 *
 *   2. G2b lifts this bound to the SAME #euf# CID:
 *        >=(call:abs(MIN_VALUE), 0)
 *
 *   3. bv32 contagion promotes the `>=` atom to int32.gte-const (bvsge):
 *        bvsge(call:abs(MIN_VALUE)_bv, #x00000000)
 *
 *   4. Conjoined: the BV expression evaluates abs(MIN_VALUE) = MIN_VALUE = -2^31.
 *      bvsge(#x80000000, #x00000000) = false under signed comparison.
 *      UNSAT → consistency unsatisfied.
 *
 * No vendor ever tested this bound. The walked body adjudicates it.
 * The bound and the universe meet at the same callsite CID.
 */
public class AbsBoundBadTest {

    @Test
    public void industryBeliefAsBound() {
        // THE INDUSTRY BELIEF written as a bound, not an equality.
        // No JDK test contains this assertion. Sugar refutes it via the walked body.
        // Under two's complement: abs(Integer.MIN_VALUE) = -2147483648 < 0.
        // Therefore abs(MIN_VALUE) >= 0 is FALSE.
        assertTrue(abs(-2147483648) >= 0);
    }
}
