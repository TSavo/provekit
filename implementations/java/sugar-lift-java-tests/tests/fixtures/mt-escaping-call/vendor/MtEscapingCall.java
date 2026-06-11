package demo;

/**
 * DISCRIMINATION FIXTURE — machinery #3: a static-method call in the seeding
 * chain ESCAPES the walkable class.
 *
 * This is MT-shaped with a resolvable buffer length (N = 624), BUT the
 * `fillState` chain delegates one of its three seeding steps to an EXTERNAL
 * helper (`ExternalMixer.mix`, defined outside this class). The walker cannot
 * inline a body it cannot see, so it MUST REFUSE BY NAME (the chain escapes the
 * walkable set), never fabricating the missing step. No seeding pin is emitted.
 */
final class MtEscapingCall {
    private static final int N = 624;
    private final int[] mt = new int[N];

    MtEscapingCall(int[] seed) {
        setSeedInternal(seed);
    }

    private void setSeedInternal(int[] seed) {
        fillState(mt, seed);
    }

    private static void fillState(int[] state, int[] seed) {
        initializeState(state);
        // ESCAPE: mixSeedAndState is delegated to a class we cannot walk.
        int next = ExternalMixer.mix(state, seed);
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
