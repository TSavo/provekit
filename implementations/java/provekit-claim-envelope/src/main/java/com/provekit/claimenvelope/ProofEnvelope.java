// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder. Per RFC 8949 §4.2.1 + the .proof spec
// (protocol/specs/2026-04-30-proof-file-format.md):
//
//   1. Build the unsigned body as a CBOR map with keys sorted by
//      bytewise lex order of their CBOR-encoded form.
//   2. Ed25519-sign the unsigned-body bytes.
//   3. Re-emit the body with the signature added; keys re-sort
//      automatically (the new "signature" key slots in by lex order).
//   4. BLAKE3-512 the final bytes; the full self-identifying string
//      "blake3-512:<128 hex>" IS the catalog CID.
//
// The `members` map key is the embedded envelope's own CID, and the
// value is its canonical bytes (JCS-JSON for memento envelopes per the
// memento envelope grammar) wrapped as a CBOR byte string.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/proof.rs and
// implementations/csharp/Provekit.ProofEnvelope/Proof.cs 1:1, plus the
// optional `binaryCid` and `metadata` fields the rust peer carries.

package com.provekit.claimenvelope;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.TreeMap;

public final class ProofEnvelope {

    private ProofEnvelope() {}

    public static final class Input {
        public final String name;
        public final String version;
        /** Map from member CID to member canonical bytes. Insertion order ignored: encoder sorts. */
        public final Map<String, byte[]> members;
        public final String signerCid;
        public final byte[] signerSeed;
        /** ISO-8601 with millisecond precision and trailing 'Z'. */
        public final String declaredAt;
        /** Optional binaryCid (omitted when null). */
        public final String binaryCid;
        /** Optional metadata key-value map (omitted when null). */
        public final Map<String, String> metadata;

        public Input(String name,
                     String version,
                     Map<String, byte[]> members,
                     String signerCid,
                     byte[] signerSeed,
                     String declaredAt,
                     String binaryCid,
                     Map<String, String> metadata) {
            this.name = Objects.requireNonNull(name, "name");
            this.version = Objects.requireNonNull(version, "version");
            this.members = Objects.requireNonNull(members, "members");
            this.signerCid = Objects.requireNonNull(signerCid, "signerCid");
            this.signerSeed = Objects.requireNonNull(signerSeed, "signerSeed");
            this.declaredAt = Objects.requireNonNull(declaredAt, "declaredAt");
            this.binaryCid = binaryCid;
            this.metadata = metadata;
        }

        public Input(String name,
                     String version,
                     Map<String, byte[]> members,
                     String signerCid,
                     byte[] signerSeed,
                     String declaredAt) {
            this(name, version, members, signerCid, signerSeed, declaredAt, null, null);
        }
    }

    public static final class Output {
        /** CBOR bytes of the signed catalog. Hash of these bytes IS the CID. */
        public final byte[] bytes;
        /** Full self-identifying CID: "blake3-512:<128 hex>". */
        public final String cid;

        Output(byte[] bytes, String cid) {
            this.bytes = bytes;
            this.cid = cid;
        }
    }

    private static final class CborPair {
        final byte[] keyCbor;
        final byte[] valueCbor;
        CborPair(byte[] k, byte[] v) { this.keyCbor = k; this.valueCbor = v; }
    }

    public static Output build(Input input) {
        // Step 1: encode unsigned body with sorted keys.
        List<CborPair> unsignedPairs = bodyPairsUnsigned(input);
        byte[] unsignedBytes = emitSortedMapBytes(unsignedPairs);

        // Step 2: Ed25519-sign the unsigned bytes.
        byte[] sig = Ed25519.signWithSeed(input.signerSeed, unsignedBytes);

        // Step 3: re-emit with signature added; keys re-sort automatically.
        List<CborPair> signedPairs = bodyPairsUnsigned(input);
        signedPairs.add(makeBytesPair("signature", sig));
        byte[] finalBytes = emitSortedMapBytes(signedPairs);

        // Step 4: filename CID = full self-identifying BLAKE3-512.
        String cid = Blake3.blake3_512(finalBytes);
        return new Output(finalBytes, cid);
    }

    /**
     * Verify a .proof envelope.
     *
     * Checks: BLAKE3-512(bytes) matches expectedCid; CBOR shape contains
     * the catalog kind + required keys; the signature over the unsigned
     * body verifies against {@code signerPubkey} (raw 32-byte Ed25519
     * public key, not a seed).
     *
     * Returns true iff all checks pass; false (never throws) for malformed input.
     */
    public static boolean verify(byte[] proofBytes, String expectedCid, byte[] signerPubkey) {
        if (proofBytes == null || expectedCid == null || signerPubkey == null) return false;
        if (signerPubkey.length != Ed25519.PUBKEY_BYTES) return false;

        // Check 1: CID
        String actualCid = Blake3.blake3_512(proofBytes);
        if (!actualCid.equals(expectedCid)) return false;

        // Check 2 + 3: decode minimally, lift the signature out and re-encode the unsigned body.
        DecodedCatalog dec;
        try {
            dec = decodeCatalog(proofBytes);
        } catch (RuntimeException ex) {
            return false;
        }
        if (dec == null) return false;
        return Ed25519.verifyBytes(signerPubkey, dec.signature, dec.unsignedBytes);
    }

    // -------------------------------------------------------------------
    // Body construction
    // -------------------------------------------------------------------

    private static List<CborPair> bodyPairsUnsigned(Input input) {
        List<CborPair> pairs = new ArrayList<>(8);
        pairs.add(makeStringPair("kind", "catalog"));
        pairs.add(makeStringPair("name", input.name));
        pairs.add(makeStringPair("version", input.version));
        pairs.add(makeMembersPair("members", input.members));
        pairs.add(makeStringPair("signer", input.signerCid));
        pairs.add(makeStringPair("declaredAt", input.declaredAt));
        if (input.binaryCid != null) {
            pairs.add(makeStringPair("binaryCid", input.binaryCid));
        }
        if (input.metadata != null) {
            pairs.add(makeMetadataPair("metadata", input.metadata));
        }
        return pairs;
    }

    private static byte[] encodeKey(String key) {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        Cbor.encodeTstr(b, key);
        return b.toByteArray();
    }

    private static CborPair makeStringPair(String key, String value) {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        Cbor.encodeTstr(b, value);
        return new CborPair(encodeKey(key), b.toByteArray());
    }

    private static CborPair makeBytesPair(String key, byte[] value) {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        Cbor.encodeBstr(b, value);
        return new CborPair(encodeKey(key), b.toByteArray());
    }

    private static CborPair makeMembersPair(String key, Map<String, byte[]> members) {
        // Encode as { tstr(cid) => bstr(envelope-bytes) }, sort by
        // bytewise CBOR-encoded-key form.
        List<CborPair> pairs = new ArrayList<>(members.size());
        for (Map.Entry<String, byte[]> e : members.entrySet()) {
            pairs.add(makeBytesPair(e.getKey(), e.getValue()));
        }
        byte[] valueBytes = emitSortedMapBytes(pairs);
        return new CborPair(encodeKey(key), valueBytes);
    }

    private static CborPair makeMetadataPair(String key, Map<String, String> metadata) {
        List<CborPair> pairs = new ArrayList<>(metadata.size());
        for (Map.Entry<String, String> e : metadata.entrySet()) {
            pairs.add(makeStringPair(e.getKey(), e.getValue()));
        }
        byte[] valueBytes = emitSortedMapBytes(pairs);
        return new CborPair(encodeKey(key), valueBytes);
    }

    private static byte[] emitSortedMapBytes(List<CborPair> pairs) {
        pairs.sort((a, b) -> compareBytewise(a.keyCbor, b.keyCbor));
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeMapHead(out, pairs.size());
        for (CborPair p : pairs) {
            out.write(p.keyCbor, 0, p.keyCbor.length);
            out.write(p.valueCbor, 0, p.valueCbor.length);
        }
        return out.toByteArray();
    }

    private static int compareBytewise(byte[] a, byte[] b) {
        int n = Math.min(a.length, b.length);
        for (int i = 0; i < n; i++) {
            int d = (a[i] & 0xFF) - (b[i] & 0xFF);
            if (d != 0) return d;
        }
        return a.length - b.length;
    }

    // -------------------------------------------------------------------
    // Minimal verifier-side decoder
    // -------------------------------------------------------------------

    private static final class DecodedCatalog {
        final byte[] unsignedBytes;
        final byte[] signature;
        DecodedCatalog(byte[] unsignedBytes, byte[] signature) {
            this.unsignedBytes = unsignedBytes;
            this.signature = signature;
        }
    }

    /**
     * Walk the top-level CBOR map, lift out the {@code signature} field's
     * raw bytes, and reconstruct the unsigned-body bytes by re-emitting
     * the remaining keys in canonical order. Returns null if the shape
     * is not a catalog (kind != "catalog") or the signature key is
     * missing / not a 64-byte byte string.
     */
    private static DecodedCatalog decodeCatalog(byte[] bytes) {
        Cursor c = new Cursor(bytes);
        long n = readMapHead(c);
        if (n < 0) return null;

        // Capture all key/value pairs as raw byte slices, plus parse strings
        // for the kind sentinel and the signature value.
        List<byte[]> keyForms = new ArrayList<>((int) n);
        List<byte[]> valueForms = new ArrayList<>((int) n);
        String kind = null;
        byte[] signature = null;

        for (long i = 0; i < n; i++) {
            int keyStart = c.pos;
            String key = readTstr(c);
            if (key == null) return null;
            int keyEnd = c.pos;

            int valStart = c.pos;
            // Snapshot the current pos before consuming value.
            String stringValueOrNull = peekIsTstr(c) ? readTstr(c) : null;
            if (stringValueOrNull == null) {
                // Reset — peek consumed bytes only if it was a tstr; otherwise
                // we still need to consume the value as raw bytes. We use a
                // straight-line skip to advance past the value.
                c.pos = valStart;
                if (!skipValue(c)) return null;
            }
            int valEnd = c.pos;

            byte[] keyForm = slice(bytes, keyStart, keyEnd);
            byte[] valueForm = slice(bytes, valStart, valEnd);
            keyForms.add(keyForm);
            valueForms.add(valueForm);

            if ("kind".equals(key)) {
                kind = stringValueOrNull;
            } else if ("signature".equals(key)) {
                // Re-parse the value as a byte string.
                Cursor v = new Cursor(valueForm);
                signature = readBstr(v);
            }
        }

        if (!"catalog".equals(kind)) return null;
        if (signature == null || signature.length != Ed25519.SIGNATURE_BYTES) return null;

        // Re-emit unsigned body: drop the signature pair, keep the rest in
        // their original (already-canonical) order, fresh map-head with
        // count-1.
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeMapHead(out, n - 1);
        for (int i = 0; i < n; i++) {
            byte[] kf = keyForms.get(i);
            // Skip the signature pair.
            if (isTstrEqual(kf, "signature")) continue;
            out.write(kf, 0, kf.length);
            byte[] vf = valueForms.get(i);
            out.write(vf, 0, vf.length);
        }
        return new DecodedCatalog(out.toByteArray(), signature);
    }

    private static boolean isTstrEqual(byte[] tstrCbor, String s) {
        Cursor c = new Cursor(tstrCbor);
        String got = readTstr(c);
        return s.equals(got);
    }

    private static byte[] slice(byte[] src, int start, int end) {
        byte[] out = new byte[end - start];
        System.arraycopy(src, start, out, 0, out.length);
        return out;
    }

    // -------------------------------------------------------------------
    // Tiny CBOR reader (only the major types this kit emits)
    // -------------------------------------------------------------------

    private static final class Cursor {
        final byte[] buf;
        int pos;
        Cursor(byte[] buf) { this.buf = buf; this.pos = 0; }
        int peek() { return pos < buf.length ? (buf[pos] & 0xFF) : -1; }
    }

    private static long readHead(Cursor c, int expectedMajor) {
        if (c.pos >= c.buf.length) return -1;
        int b = c.buf[c.pos++] & 0xFF;
        int major = b >>> 5;
        if (major != expectedMajor) {
            // Caller decides whether to reset.
            c.pos--;
            return -1;
        }
        int low = b & 0x1F;
        if (low < 24) return low;
        long argLen = switch (low) {
            case 24 -> 1;
            case 25 -> 2;
            case 26 -> 4;
            case 27 -> 8;
            default -> -1;
        };
        if (argLen < 0) return -1;
        if (c.pos + argLen > c.buf.length) return -1;
        long v = 0;
        for (int i = 0; i < argLen; i++) {
            v = (v << 8) | (c.buf[c.pos++] & 0xFF);
        }
        return v;
    }

    private static long readMapHead(Cursor c) {
        return readHead(c, Cbor.MAJOR_MAP);
    }

    private static String readTstr(Cursor c) {
        long len = readHead(c, Cbor.MAJOR_TEXT_STRING);
        if (len < 0) return null;
        if (c.pos + len > c.buf.length) return null;
        String s = new String(c.buf, c.pos, (int) len, java.nio.charset.StandardCharsets.UTF_8);
        c.pos += (int) len;
        return s;
    }

    private static byte[] readBstr(Cursor c) {
        long len = readHead(c, Cbor.MAJOR_BYTE_STRING);
        if (len < 0) return null;
        if (c.pos + len > c.buf.length) return null;
        byte[] out = new byte[(int) len];
        System.arraycopy(c.buf, c.pos, out, 0, out.length);
        c.pos += (int) len;
        return out;
    }

    private static boolean peekIsTstr(Cursor c) {
        int b = c.peek();
        if (b < 0) return false;
        return (b >>> 5) == Cbor.MAJOR_TEXT_STRING;
    }

    /** Skip past the next CBOR value at {@code c.pos}. Returns false on malformed input. */
    private static boolean skipValue(Cursor c) {
        if (c.pos >= c.buf.length) return false;
        int b = c.buf[c.pos++] & 0xFF;
        int major = b >>> 5;
        int low = b & 0x1F;
        long arg;
        if (low < 24) {
            arg = low;
        } else {
            long argLen = switch (low) {
                case 24 -> 1;
                case 25 -> 2;
                case 26 -> 4;
                case 27 -> 8;
                default -> -1;
            };
            if (argLen < 0) return false;
            if (c.pos + argLen > c.buf.length) return false;
            long v = 0;
            for (int i = 0; i < argLen; i++) {
                v = (v << 8) | (c.buf[c.pos++] & 0xFF);
            }
            arg = v;
        }
        switch (major) {
            case Cbor.MAJOR_UNSIGNED_INT:
                return true;
            case Cbor.MAJOR_BYTE_STRING:
            case Cbor.MAJOR_TEXT_STRING:
                if (c.pos + arg > c.buf.length) return false;
                c.pos += (int) arg;
                return true;
            case Cbor.MAJOR_ARRAY:
                for (long i = 0; i < arg; i++) {
                    if (!skipValue(c)) return false;
                }
                return true;
            case Cbor.MAJOR_MAP:
                for (long i = 0; i < arg; i++) {
                    if (!skipValue(c)) return false; // key
                    if (!skipValue(c)) return false; // value
                }
                return true;
            default:
                return false;
        }
    }
}
