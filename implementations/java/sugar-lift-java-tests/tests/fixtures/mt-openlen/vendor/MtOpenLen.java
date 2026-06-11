package demo;

/**
 * DISCRIMINATION FIXTURE — machinery #1: inter-procedural param-array .length
 * not statically resolvable.
 *
 * This is MT-shaped (a `Cls(int[] seed)` constructor entering a seeding chain),
 * BUT its state buffer is `new int[size]` where `size` is a NON-static-final
 * instance field — the buffer length is NOT statically resolvable through the
 * call chain. The walker has no termination guarantee for the seeding loops
 * (whose bound is `state.length`) and MUST REFUSE BY NAME on the buffer length,
 * never guessing a dimension. No seeding pin may be emitted.
 */
final class MtOpenLen {
    /** State buffer allocated with a NON-resolvable dimension: a method-call
     *  dimension `new int[computeSize()]` does not fold to a static int, so the
     *  buffer length is not resolvable through the chain. */
    private final int[] mt = new int[computeSize()];

    private static int computeSize() { return 624; }

    MtOpenLen(int[] seed) {
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
            state[i] = mt;
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
