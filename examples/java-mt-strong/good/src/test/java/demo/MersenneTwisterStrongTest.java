package demo;

import org.apache.commons.rng.core.source32.MersenneTwister;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite (STRONG tier): the Matsumoto reference vectors DERIVED.
 *
 * Seed: {0x123, 0x234, 0x345, 0x456} — the canonical Nishimura test seed.
 * Reference values from:
 *   http://www.math.sci.hiroshima-u.ac.jp/~m-mat/MT/MT2002/CODES/mt19937ar.out
 * as asserted verbatim in commons-rng-1.7:
 *   MersenneTwisterTest.java::testMakotoNishimura, expectedSequence[0..7].
 *
 * UNLIKE the FLOOR rung (java-mt-reference), these contracts are not merely
 * point equalities checked for within-test contradiction. The kit walks the
 * vendor's ENTIRE seed→state→draw pipeline INTER-PROCEDURALLY:
 *
 *   new MersenneTwister(seed)
 *     → setSeedInternal(seed)
 *       → fillStateMersenneTwister(mt, seed)
 *           → initializeState(state)        [forward i++, bound state.length=624]
 *           → mixSeedAndState(state, seed)  [countdown k--, cursors i,j, wrap]
 *           → mixState(state, nextIndex)    [countdown k--, cursor i,   wrap]
 *           → state[0] = UPPER_MASK
 *     → mti = N
 *   nextInt() → next() → the twist [three k++ sweeps] + tempering
 *
 * for the LITERAL seed, building one closed bv32 FOL (an SSA `let`-chain) per
 * draw position. The contract `mt32.eq-seeded(refValue, <walked recurrence>)`
 * pins the vendor's sworn value to the WALKED computation. The seed→state fold
 * is independently re-verified against a recompute; the twist+tempering is
 * walked, not faked.
 *
 * All 8 reference values are consistent with the walked recurrence → DISCHARGED
 * by DERIVATION (not by consistency). The universe does the work.
 *
 * Logo: "Commons RNG's own Mersenne Twister reference vectors, DERIVED — the
 *        whole seed→state→twist→temper pipeline walked and checked, no extraction."
 */
public class MersenneTwisterStrongTest {

    /** Canonical Nishimura test seed. */
    private static final int[] SEED = {0x123, 0x234, 0x345, 0x456};

    @Test
    public void testDraw0() {
        MersenneTwister mt = new MersenneTwister(SEED);
        int d = mt.nextInt();
        assertEquals(0x3fa23623, d);
    }

    @Test
    public void testDraw1() {
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0x38fa935f, d);
    }

    @Test
    public void testDraw2() {
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt(); mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0x1c72dc38, d);
    }

    @Test
    public void testDraw3() {
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt(); mt.nextInt(); mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0xf4cf2f5f, d);
    }

    @Test
    public void testDraw4() {
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 4; i++) mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0xfc110f5c, d);
    }

    @Test
    public void testDraw5() {
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 5; i++) mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0xc75677aa, d);
    }

    @Test
    public void testDraw6() {
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 6; i++) mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0xc802152f, d);
    }

    @Test
    public void testDraw7() {
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 7; i++) mt.nextInt();
        int d = mt.nextInt();
        assertEquals(0x0d9155da, d);
    }
}
