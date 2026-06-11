package demo;

/**
 * DISCRIMINATION FIXTURE — uninterpretable in-loop conditional (branch-gated
 * store via a statement-level `if`).
 *
 * The bound is a static-final literal and the index is the induction var, so
 * those gates pass. BUT the body performs a STORE inside a statement-level
 * `if` (a control-flow branch, not the value-level `?:` MAG01 gate we walk).
 * The walker has not generalized statement-level branch-gated stores, so it
 * MUST refuse BY NAME, locating the break at the IF node — never silently
 * dropping one branch of the store.
 */
final class IfStore {
    private static final int LEN = 8;
    static void fill(int[] state) {
        int mt = 19650218;
        state[0] = mt;
        for (int i = 1; i < LEN; i++) {
            mt = 1812433253 * (mt ^ (mt >> 30)) + i;
            if ((mt & 1) == 1) {
                state[i] = mt ^ 0x9908b0df;   // branch-gated store via `if`
            } else {
                state[i] = mt;
            }
        }
    }
}
