package demo;

/**
 * DISCRIMINATION FIXTURE — machinery #2: a field-array store that escapes the
 * bound state array.
 *
 * This is MT-shaped with a resolvable buffer length (N = 624), BUT its
 * `initializeState` writes a SECOND field array (`shadow`) that is NOT the
 * `state` parameter bound to the threaded buffer. A write to a field array
 * outside the resolvable chain is unsound to thread against the state store, so
 * the walker MUST REFUSE BY NAME at that write — never silently dropping it nor
 * folding it into the wrong buffer. No seeding pin is emitted.
 */
final class MtFieldEscape {
    private static final int N = 624;
    private final int[] mt = new int[N];
    /** A second buffer the seeding loop also writes — NOT the threaded state. */
    private static final int[] shadow = new int[N];

    MtFieldEscape(int[] seed) {
        setSeedInternal(seed);
    }

    private void setSeedInternal(int[] seed) {
        fillState(mt, seed);
    }

    private static void fillState(int[] state, int[] seed) {
        initializeState(state);
        int next = mixSeedAndState(state, seed);
        mixState(state, next);
    }

    private static void initializeState(int[] state) {
        int mt = 19650218;
        state[0] = mt;
        for (int i = 1; i < state.length; i++) {
            mt = 1812433253 * (mt ^ (mt >>> 30)) + i;
            // ESCAPE: a write to a DIFFERENT field array than the bound state.
            shadow[i] = mt;
        }
    }

    private static int mixSeedAndState(int[] state, int[] seed) {
        int i = 1, j = 0;
        for (int k = Math.max(state.length, seed.length); k > 0; k--) {
            int a = state[i], b = state[i - 1];
            state[i] = (a ^ ((b ^ (b >>> 30)) * 1664525)) + seed[j] + j;
            i++; j++;
            if (i >= state.length) { state[0] = state[state.length - 1]; i = 1; }
            if (j >= seed.length) j = 0;
        }
        return i;
    }

    private static void mixState(int[] state, int startIndex) {
        int i = startIndex;
        for (int k = state.length - 1; k > 0; k--) {
            int a = state[i], b = state[i - 1];
            state[i] = (a ^ ((b ^ (b >>> 30)) * 1566083941)) - i;
            i++;
            if (i >= state.length) { state[0] = state[state.length - 1]; i = 1; }
        }
    }
}
