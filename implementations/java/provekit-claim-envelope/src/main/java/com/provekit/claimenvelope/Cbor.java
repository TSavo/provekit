// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR encoder. RFC 8949 §4.2.1 rules:
//   - shortest-form integer encoding (smallest of short / u8 / u16 / u32 / u64)
//   - definite-length items only
//   - map keys sorted in bytewise lex order of their CBOR-encoded form
//
// Only emits the major types needed by the proof-envelope shape:
// unsigned int, byte string, text string, array, map.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs and
// implementations/csharp/Provekit.ProofEnvelope/Cbor.cs 1:1.

package com.provekit.claimenvelope;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

public final class Cbor {
    public static final int MAJOR_UNSIGNED_INT = 0;
    public static final int MAJOR_BYTE_STRING = 2;
    public static final int MAJOR_TEXT_STRING = 3;
    public static final int MAJOR_ARRAY = 4;
    public static final int MAJOR_MAP = 5;

    private Cbor() {}

    /** Append a CBOR head: major-type tag + shortest-form length / value. */
    public static void appendHead(ByteArrayOutputStream out, int major, long arg) {
        if (arg < 0) {
            // We never emit negative integers from this kit; fail loudly.
            throw new IllegalArgumentException("Cbor.appendHead: arg must be non-negative");
        }
        int mt = (major & 0x07) << 5;
        if (arg < 24L) {
            out.write(mt | (int) arg);
            return;
        }
        if (arg <= 0xFFL) {
            out.write(mt | 24);
            out.write((int) arg);
            return;
        }
        if (arg <= 0xFFFFL) {
            out.write(mt | 25);
            out.write((int) ((arg >>> 8) & 0xFF));
            out.write((int) (arg & 0xFF));
            return;
        }
        if (arg <= 0xFFFFFFFFL) {
            out.write(mt | 26);
            out.write((int) ((arg >>> 24) & 0xFF));
            out.write((int) ((arg >>> 16) & 0xFF));
            out.write((int) ((arg >>> 8) & 0xFF));
            out.write((int) (arg & 0xFF));
            return;
        }
        out.write(mt | 27);
        for (int i = 7; i >= 0; i--) {
            out.write((int) ((arg >>> (i * 8)) & 0xFF));
        }
    }

    public static void encodeUint(ByteArrayOutputStream out, long value) {
        appendHead(out, MAJOR_UNSIGNED_INT, value);
    }

    public static void encodeBstr(ByteArrayOutputStream out, byte[] bytes) {
        appendHead(out, MAJOR_BYTE_STRING, bytes.length);
        out.write(bytes, 0, bytes.length);
    }

    public static void encodeTstr(ByteArrayOutputStream out, String utf8) {
        byte[] b = utf8.getBytes(StandardCharsets.UTF_8);
        appendHead(out, MAJOR_TEXT_STRING, b.length);
        out.write(b, 0, b.length);
    }

    public static void encodeArrayHead(ByteArrayOutputStream out, long count) {
        appendHead(out, MAJOR_ARRAY, count);
    }

    public static void encodeMapHead(ByteArrayOutputStream out, long count) {
        appendHead(out, MAJOR_MAP, count);
    }
}
