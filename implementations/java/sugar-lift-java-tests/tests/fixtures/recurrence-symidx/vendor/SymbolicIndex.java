package demo;

/**
 * DISCRIMINATION FIXTURE — symbolic array index.
 *
 * The loop bound is a static-final literal (LEN), so the unroll itself is
 * sound. BUT the array store target index `j` is a SCALAR computed from a
 * data-dependent expression (a wrapping index, as in the real MT
 * mixSeedAndState `state[i]` with `i` re-seeded by a runtime branch). `j` is
 * not statically resolvable to a concrete value at unroll time, so the store
 * is UNSOUND and the walker MUST refuse BY NAME, locating the break at the
 * symbolic index. (Sound stores require literal / induction-var arithmetic.)
 */
final class SymbolicIndex {
    private static final int LEN = 8;
    static void fill(int[] state, int start) {
        int mt = 19650218;
        int j = start;
        for (int i = 1; i < LEN; i++) {
            mt = 1812433253 * (mt ^ (mt >> 30)) + i;
            state[j] = mt;   // j is symbolic — not resolvable to a concrete index
        }
    }
}
