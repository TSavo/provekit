// SPDX-License-Identifier: Apache-2.0
//
// Pure-Java BLAKE3 implementation (reference algorithm).
//
// Source of truth: BLAKE3 specification (https://github.com/BLAKE3-team/
// BLAKE3-specs), Python reference impl, and the C portable code at
// tools/blake3-vendored/blake3_portable.c. Single-threaded, no SIMD,
// no parallelism. Adequate for the substrate's hash-the-bytes workload
// (envelopes are kilobytes, not gigabytes).
//
// Tested against:
//   - blake3-512(b"") == af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7
//                       cc9a93cae41f3262e00f03e7b69af26b7faaf09fcd333050
//                       338ddfe085b8cc869ca98b206c08243a
//     (pinned in test_canonicalizer.py from the rust kit)
//   - All cross-kit byte-equivalence pins flow through this hash.
//
// The protocol mandates 64-byte BLAKE3 output (XOF mode). This impl
// exposes both the streaming hasher and a one-shot `hash512` helper.

package com.provekit.canonicalizer;

public final class Blake3 {

    private Blake3() {}

    // --- Constants from the BLAKE3 spec ------------------------------------

    private static final int OUT_LEN = 32;
    private static final int KEY_LEN = 32;
    private static final int BLOCK_LEN = 64;
    private static final int CHUNK_LEN = 1024;

    // Domain-flag bit positions (spec §2.1).
    private static final int CHUNK_START = 1 << 0;
    private static final int CHUNK_END = 1 << 1;
    private static final int PARENT = 1 << 2;
    private static final int ROOT = 1 << 3;
    @SuppressWarnings("unused")
    private static final int KEYED_HASH = 1 << 4;
    @SuppressWarnings("unused")
    private static final int DERIVE_KEY_CONTEXT = 1 << 5;
    @SuppressWarnings("unused")
    private static final int DERIVE_KEY_MATERIAL = 1 << 6;

    // IV: first 8 words of SHA-256 IV (spec §2.1).
    static final int[] IV = {
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
        0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
    };

    // Message schedule permutation (spec §2.4).
    private static final int[] MSG_PERMUTATION = {
        2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8,
    };

    // --- Compression function ---------------------------------------------

    private static void g(int[] state, int a, int b, int c, int d, int mx, int my) {
        state[a] = state[a] + state[b] + mx;
        state[d] = Integer.rotateRight(state[d] ^ state[a], 16);
        state[c] = state[c] + state[d];
        state[b] = Integer.rotateRight(state[b] ^ state[c], 12);
        state[a] = state[a] + state[b] + my;
        state[d] = Integer.rotateRight(state[d] ^ state[a], 8);
        state[c] = state[c] + state[d];
        state[b] = Integer.rotateRight(state[b] ^ state[c], 7);
    }

    private static void round(int[] state, int[] m) {
        // Mix columns.
        g(state, 0, 4,  8, 12, m[ 0], m[ 1]);
        g(state, 1, 5,  9, 13, m[ 2], m[ 3]);
        g(state, 2, 6, 10, 14, m[ 4], m[ 5]);
        g(state, 3, 7, 11, 15, m[ 6], m[ 7]);
        // Mix diagonals.
        g(state, 0, 5, 10, 15, m[ 8], m[ 9]);
        g(state, 1, 6, 11, 12, m[10], m[11]);
        g(state, 2, 7,  8, 13, m[12], m[13]);
        g(state, 3, 4,  9, 14, m[14], m[15]);
    }

    private static void permute(int[] m) {
        int[] permuted = new int[16];
        for (int i = 0; i < 16; i++) {
            permuted[i] = m[MSG_PERMUTATION[i]];
        }
        System.arraycopy(permuted, 0, m, 0, 16);
    }

    /**
     * Compress one 64-byte block. Returns the 16-word output state.
     */
    static int[] compress(int[] chainingValue, int[] blockWords, long counter,
                          int blockLen, int flags) {
        int[] state = {
            chainingValue[0], chainingValue[1], chainingValue[2], chainingValue[3],
            chainingValue[4], chainingValue[5], chainingValue[6], chainingValue[7],
            IV[0], IV[1], IV[2], IV[3],
            (int) counter, (int) (counter >>> 32), blockLen, flags,
        };
        int[] block = new int[16];
        System.arraycopy(blockWords, 0, block, 0, 16);

        round(state, block); permute(block);
        round(state, block); permute(block);
        round(state, block); permute(block);
        round(state, block); permute(block);
        round(state, block); permute(block);
        round(state, block); permute(block);
        round(state, block);

        // Output: full 16-word state for XOF; first 8 words are also
        // the next chaining value (state[i] ^= state[i+8]).
        for (int i = 0; i < 8; i++) {
            state[i] ^= state[i + 8];
            state[i + 8] ^= chainingValue[i];
        }
        return state;
    }

    private static int[] firstEightWords(int[] compressionOutput) {
        int[] cv = new int[8];
        System.arraycopy(compressionOutput, 0, cv, 0, 8);
        return cv;
    }

    private static void wordsFromLittleEndian(byte[] bytes, int offset, int length, int[] out, int outOffset) {
        for (int i = 0; i < length / 4; i++) {
            int b0 = bytes[offset + 4 * i] & 0xFF;
            int b1 = bytes[offset + 4 * i + 1] & 0xFF;
            int b2 = bytes[offset + 4 * i + 2] & 0xFF;
            int b3 = bytes[offset + 4 * i + 3] & 0xFF;
            out[outOffset + i] = b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);
        }
    }

    // --- Chunk state -------------------------------------------------------

    private static final class ChunkState {
        int[] chainingValue;
        long chunkCounter;
        byte[] block = new byte[BLOCK_LEN];
        int blockLen;
        int blocksCompressed;
        int flags;

        ChunkState(int[] keyWords, long chunkCounter, int flags) {
            this.chainingValue = keyWords.clone();
            this.chunkCounter = chunkCounter;
            this.flags = flags;
        }

        int len() {
            return BLOCK_LEN * blocksCompressed + blockLen;
        }

        int startFlag() {
            return blocksCompressed == 0 ? CHUNK_START : 0;
        }

        void update(byte[] input, int offset, int length) {
            int pos = offset;
            int end = offset + length;
            while (pos < end) {
                if (blockLen == BLOCK_LEN) {
                    int[] blockWords = new int[16];
                    wordsFromLittleEndian(block, 0, BLOCK_LEN, blockWords, 0);
                    int[] out = compress(chainingValue, blockWords, chunkCounter,
                        BLOCK_LEN, flags | startFlag());
                    chainingValue = firstEightWords(out);
                    blocksCompressed += 1;
                    blockLen = 0;
                    java.util.Arrays.fill(block, (byte) 0);
                }
                int want = BLOCK_LEN - blockLen;
                int take = Math.min(want, end - pos);
                System.arraycopy(input, pos, block, blockLen, take);
                blockLen += take;
                pos += take;
            }
        }

        Output output() {
            int[] blockWords = new int[16];
            wordsFromLittleEndian(block, 0, BLOCK_LEN, blockWords, 0);
            return new Output(chainingValue, blockWords, chunkCounter, blockLen,
                flags | startFlag() | CHUNK_END);
        }
    }

    private static int[] parentOutputCv(int[] leftCv, int[] rightCv, int[] keyWords, int flags) {
        int[] blockWords = new int[16];
        System.arraycopy(leftCv, 0, blockWords, 0, 8);
        System.arraycopy(rightCv, 0, blockWords, 8, 8);
        int[] out = compress(keyWords, blockWords, 0, BLOCK_LEN, flags | PARENT);
        return firstEightWords(out);
    }

    /**
     * Output state used to produce extended XOF output at the root.
     */
    private static final class Output {
        final int[] inputChainingValue;
        final int[] blockWords;
        final long counter;
        final int blockLen;
        final int flags;

        Output(int[] cv, int[] blockWords, long counter, int blockLen, int flags) {
            this.inputChainingValue = cv;
            this.blockWords = blockWords;
            this.counter = counter;
            this.blockLen = blockLen;
            this.flags = flags;
        }

        int[] chainingValue() {
            int[] out = compress(inputChainingValue, blockWords, counter, blockLen, flags);
            return firstEightWords(out);
        }

        void rootOutputBytes(byte[] out, int offset, int length) {
            long outputBlockCounter = 0;
            int writeOffset = offset;
            int remaining = length;
            while (remaining > 0) {
                int[] words = compress(inputChainingValue, blockWords,
                    outputBlockCounter, blockLen, flags | ROOT);
                int wordCount = Math.min(16, (remaining + 3) / 4);
                for (int i = 0; i < wordCount; i++) {
                    int w = words[i];
                    int byteCount = Math.min(4, remaining);
                    for (int b = 0; b < byteCount; b++) {
                        out[writeOffset + b] = (byte) (w >>> (b * 8));
                    }
                    writeOffset += byteCount;
                    remaining -= byteCount;
                    if (remaining == 0) break;
                }
                outputBlockCounter += 1;
            }
        }
    }

    // --- Hasher ------------------------------------------------------------

    /**
     * Streaming BLAKE3 hasher in default (unkeyed) mode.
     */
    public static final class Hasher {
        private ChunkState chunkState;
        private final int[] keyWords = IV.clone();
        private final int[][] cvStack = new int[54][]; // max tree depth for 2^64 bytes
        private int cvStackLen;
        private final int flags = 0;

        public Hasher() {
            chunkState = new ChunkState(keyWords, 0, flags);
        }

        private void pushCv(int[] cv) {
            cvStack[cvStackLen++] = cv;
        }

        private int[] popCv() {
            return cvStack[--cvStackLen];
        }

        private void addChunkChainingValue(int[] newCv, long totalChunks) {
            // Whenever the total number of chunks is even, we merge.
            // The number of trailing 0-bits in `totalChunks` says how
            // many merges we must do.
            long chunks = totalChunks;
            while ((chunks & 1) == 0) {
                newCv = parentOutputCv(popCv(), newCv, keyWords, flags);
                chunks >>>= 1;
            }
            pushCv(newCv);
        }

        public Hasher update(byte[] input) {
            return update(input, 0, input.length);
        }

        public Hasher update(byte[] input, int offset, int length) {
            int pos = offset;
            int end = offset + length;
            while (pos < end) {
                if (chunkState.len() == CHUNK_LEN) {
                    int[] chunkCv = chunkState.output().chainingValue();
                    long totalChunks = chunkState.chunkCounter + 1;
                    addChunkChainingValue(chunkCv, totalChunks);
                    chunkState = new ChunkState(keyWords, totalChunks, flags);
                }
                int want = CHUNK_LEN - chunkState.len();
                int take = Math.min(want, end - pos);
                chunkState.update(input, pos, take);
                pos += take;
            }
            return this;
        }

        /**
         * Finalize and write {@code length} bytes of output to {@code out}
         * starting at {@code offset}. Multiple calls return the same XOF
         * stream prefix.
         */
        public void finalize(byte[] out, int offset, int length) {
            Output output = chunkState.output();
            int parentNodes = cvStackLen;
            while (parentNodes > 0) {
                parentNodes -= 1;
                int[] leftCv = cvStack[parentNodes];
                int[] rightCv = output.chainingValue();
                int[] blockWords = new int[16];
                System.arraycopy(leftCv, 0, blockWords, 0, 8);
                System.arraycopy(rightCv, 0, blockWords, 8, 8);
                output = new Output(keyWords, blockWords, 0, BLOCK_LEN, flags | PARENT);
            }
            output.rootOutputBytes(out, offset, length);
        }

        /** Convenience: finalize and return a fresh {@code length}-byte array. */
        public byte[] finalizeBytes(int length) {
            byte[] out = new byte[length];
            finalize(out, 0, length);
            return out;
        }
    }

    /**
     * Compute the BLAKE3-512 (64-byte XOF) hash of {@code input} and
     * return the spec's self-identifying string form
     * ({@code "blake3-512:" + lowercase-hex(64-byte digest)}).
     */
    public static String blake3_512Cid(byte[] input) {
        Hasher h = new Hasher();
        if (input.length > 0) {
            h.update(input);
        }
        byte[] digest = h.finalizeBytes(64);
        return "blake3-512:" + toHexLower(digest);
    }

    /**
     * Compute the raw 64-byte BLAKE3-512 hash of {@code input}. No
     * self-identifying prefix.
     */
    public static byte[] blake3_512(byte[] input) {
        Hasher h = new Hasher();
        if (input.length > 0) {
            h.update(input);
        }
        return h.finalizeBytes(64);
    }

    private static final char[] HEX_LOWER = "0123456789abcdef".toCharArray();

    static String toHexLower(byte[] bytes) {
        char[] out = new char[bytes.length * 2];
        for (int i = 0; i < bytes.length; i++) {
            int b = bytes[i] & 0xFF;
            out[i * 2] = HEX_LOWER[b >>> 4];
            out[i * 2 + 1] = HEX_LOWER[b & 0xF];
        }
        return new String(out);
    }
}
