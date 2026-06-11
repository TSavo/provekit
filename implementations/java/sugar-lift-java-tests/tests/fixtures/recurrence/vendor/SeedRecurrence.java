package demo;

/**
 * SYNTHETIC FIXTURE — NOT A VENDOR LOGO.
 *
 * This is a controlled stand-in that carries the SHAPE of a loop-carried
 * recurrence over a mutable fixed-size buffer (the Mersenne-Twister seeding
 * recurrence shape), but with a LITERAL loop bound so the walker can unroll it
 * fully and prove the machinery end to end.
 *
 * It deliberately uses the SAME operator vocabulary as the real Commons RNG
 * MersenneTwister.initializeState seeding recurrence:
 *   mt = 1812433253 * (mt ^ (mt >> 30)) + i
 * plus a static-final 2-element gate array read under a low-bit conditional
 * (the twist's MAG01 shape) — so the walked FOL exercises bv32.mul, bv32.xor,
 * bv32.lshr, bv32.add, the symbolic mutable-array store at an induction index,
 * the literal-index base store, and the bv32.ite low-bit gate.
 *
 * The point of this fixture is to demonstrate the GENERALIZED machinery walks
 * a recurrence over a mutable array soundly. It is NOT a derivation of any
 * vendor reference vector and makes no claim sworn by any vendor test.
 */
final class SeedRecurrence {

    /** Twist gate array — 2 static-final literal entries, read at a low-bit index. */
    private static final int[] GATE = {0x0, 0x9908b0df};

    /** Buffer size — a static-final int literal (the resolvable loop bound). */
    private static final int LEN = 8;

    /**
     * Fill `state` with the seeding recurrence. The loop bound is the
     * static-final int LEN (resolvable → unrolls fully). Each step is a clean
     * scalar recurrence on `mt` plus an array store at the induction index,
     * with a MAG01-style low-bit gate folded in.
     */
    static void fill(int[] state) {
        int mt = 19650218;
        state[0] = mt;
        for (int i = 1; i < LEN; i++) {
            mt = 1812433253 * (mt ^ (mt >> 30)) + i;
            int gated = mt ^ ((mt & 1) == 1 ? GATE[1] : GATE[0]);
            state[i] = gated;
        }
    }
}
