package demo;

/**
 * DISCRIMINATION FIXTURE — non-literal loop bound.
 *
 * This carries EXACTLY the real Commons RNG MersenneTwister.initializeState
 * shape, including its loop bound `i < state.length`. The bound is a runtime
 * array length, NOT a literal or static-final int — so the walker has no
 * termination guarantee and MUST refuse the unroll BY NAME, locating the break
 * at the `state.length` bound. No FOL may be emitted (no unbounded unroll).
 */
final class OpenBound {
    static void fill(int[] state) {
        int mt = 19650218;
        state[0] = mt;
        for (int i = 1; i < state.length; i++) {
            mt = 1812433253 * (mt ^ (mt >> 30)) + i;
            state[i] = mt;
        }
    }
}
