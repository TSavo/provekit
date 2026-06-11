package demo;

import org.apache.commons.rng.core.source32.MersenneTwister;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: Matsumoto reference-vector point contracts.
 *
 * Seed: {0x123, 0x234, 0x345, 0x456} — the canonical Nishimura test seed.
 * Reference values from:
 *   http://www.math.sci.hiroshima-u.ac.jp/~m-mat/MT/MT2002/CODES/mt19937ar.out
 * as asserted verbatim in commons-rng-1.7:
 *   commons-rng-core/src/test/java/org/apache/commons/rng/core/source32/
 *   MersenneTwisterTest.java, method testMakotoNishimura
 *   expectedSequence[0..7] (first row of the vendor's reference sequence)
 *
 * Each test method is ONE draw at position N:
 *   - Freshly seeds MersenneTwister with the canonical seed
 *   - Calls nextInt() N times to advance to position N
 *   - Binds position N's output to an SSA local `d`
 *   - Asserts the vendor-sworn value via assertEquals(refValue, d)
 *
 * The kit substitutes `d` back to the nextInt() callsite and emits a
 * location-keyed point contract scoped to the test method. Each test method
 * has a DISTINCT location key (different method name), so the contracts are
 * independent and each is checked individually.
 *
 * All 8 contracts assert a value consistent with the vendor's reference
 * → all discharged.
 *
 * FLOOR scope: proves the per-draw value is a contract the vendor SWORE
 * (point equality, bin-1, deterministic theorem for the fixed seed).
 * Does NOT derive the output from the algorithm — derivation needs the
 * tempering universe + seed-state walk (rungs 2/3, NOT this showcase).
 *
 * Logo: "Commons RNG's own Mersenne Twister reference vectors, lifted and
 *        federated — the PRNG's per-draw contract, sworn by the vendor."
 */
public class MersenneTwisterReferenceTest {

    /** Canonical Nishimura test seed. */
    private static final int[] SEED = {0x123, 0x234, 0x345, 0x456};

    @Test
    public void testDraw0() {
        // Vendor-sworn: expectedSequence[0] = 0x3fa23623
        // Source: MersenneTwisterTest.java::testMakotoNishimura (commons-rng-1.7)
        MersenneTwister mt = new MersenneTwister(SEED);
        int d = mt.nextInt();
        assertEquals(0x3fa23623, d);
    }

    @Test
    public void testDraw1() {
        // Vendor-sworn: expectedSequence[1] = 0x38fa935f
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt(); // advance past draw[0]
        int d = mt.nextInt();
        assertEquals(0x38fa935f, d);
    }

    @Test
    public void testDraw2() {
        // Vendor-sworn: expectedSequence[2] = 0x1c72dc38
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt(); mt.nextInt(); // advance past draw[0..1]
        int d = mt.nextInt();
        assertEquals(0x1c72dc38, d);
    }

    @Test
    public void testDraw3() {
        // Vendor-sworn: expectedSequence[3] = 0xf4cf2f5f
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt(); mt.nextInt(); mt.nextInt(); // advance past draw[0..2]
        int d = mt.nextInt();
        assertEquals(0xf4cf2f5f, d);
    }

    @Test
    public void testDraw4() {
        // Vendor-sworn: expectedSequence[4] = 0xfc110f5c
        MersenneTwister mt = new MersenneTwister(SEED);
        mt.nextInt(); mt.nextInt(); mt.nextInt(); mt.nextInt(); // advance past draw[0..3]
        int d = mt.nextInt();
        assertEquals(0xfc110f5c, d);
    }

    @Test
    public void testDraw5() {
        // Vendor-sworn: expectedSequence[5] = 0xc75677aa
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 5; i++) mt.nextInt(); // advance past draw[0..4]
        int d = mt.nextInt();
        assertEquals(0xc75677aa, d);
    }

    @Test
    public void testDraw6() {
        // Vendor-sworn: expectedSequence[6] = 0xc802152f
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 6; i++) mt.nextInt(); // advance past draw[0..5]
        int d = mt.nextInt();
        assertEquals(0xc802152f, d);
    }

    @Test
    public void testDraw7() {
        // Vendor-sworn: expectedSequence[7] = 0x0d9155da
        MersenneTwister mt = new MersenneTwister(SEED);
        for (int i = 0; i < 7; i++) mt.nextInt(); // advance past draw[0..6]
        int d = mt.nextInt();
        assertEquals(0x0d9155da, d);
    }
}
