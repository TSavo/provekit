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
//      `"blake3-512:<128 hex>"` IS the catalog CID.
//
// The `members` map key is the embedded envelope's own CID, and the
// value is its canonical bytes (JCS-JSON for memento envelopes per
// the memento envelope grammar) wrapped as a CBOR byte string.
//
// Java peer of implementations/rust/provekit-proof-envelope/src/proof.rs
// and implementations/csharp/Provekit.ProofEnvelope/Proof.cs. Output is
// byte-identical to the rust kit for the same canonical input.

package com.provekit.proofenvelope;

import com.provekit.canonicalizer.Hash;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

public final class ProofEnvelope {

    private ProofEnvelope() {}

    // -----------------------------------------------------------------------
    // Logical input/output
    // -----------------------------------------------------------------------

    /** Logical input shape for a .proof catalog memento. */
    public static final class Input {
        public final String name;
        public final String version;
        /**
         * Map from member CID (full self-identifying string form,
         * e.g. {@code "blake3-512:abc..."}) to that member's canonical
         * bytes (JCS-JSON bytes for memento envelopes).
         */
        public final Map<String, byte[]> members;
        /** CID of the signer's public-key memento. */
        public final String signerCid;
        /** Ed25519 seed bytes (32). Deterministic signing. */
        public final byte[] signerSeed;
        /** ISO-8601 string with millisecond precision and trailing 'Z'. */
        public final String declaredAt;
        /** Optional CID of the binary this proof attests, or null. */
        public final String binaryCid;
        /** Optional metadata; sorted at encode time. */
        public final Map<String, String> metadata;

        private Input(Builder b) {
            this.name = b.name;
            this.version = b.version;
            this.members = Collections.unmodifiableMap(new LinkedHashMap<>(b.members));
            this.signerCid = b.signerCid;
            this.signerSeed = b.signerSeed.clone();
            this.declaredAt = b.declaredAt;
            this.binaryCid = b.binaryCid;
            this.metadata = b.metadata == null
                ? null
                : Collections.unmodifiableMap(new LinkedHashMap<>(b.metadata));
        }

        public static Builder builder() { return new Builder(); }

        public static final class Builder {
            private String name = "";
            private String version = "";
            private Map<String, byte[]> members = new LinkedHashMap<>();
            private String signerCid = "";
            private byte[] signerSeed = new byte[32];
            private String declaredAt = "";
            private String binaryCid;
            private Map<String, String> metadata;

            public Builder name(String v) { this.name = v; return this; }
            public Builder version(String v) { this.version = v; return this; }
            public Builder members(Map<String, byte[]> v) {
                this.members = new LinkedHashMap<>(v);
                return this;
            }
            public Builder addMember(String cid, byte[] bytes) {
                this.members.put(cid, bytes.clone());
                return this;
            }
            public Builder signerCid(String v) { this.signerCid = v; return this; }
            public Builder signerSeed(byte[] v) {
                if (v == null || v.length != 32) {
                    throw new IllegalArgumentException("signer seed must be 32 bytes");
                }
                this.signerSeed = v.clone();
                return this;
            }
            public Builder declaredAt(String v) { this.declaredAt = v; return this; }
            public Builder binaryCid(String v) { this.binaryCid = v; return this; }
            public Builder metadata(Map<String, String> v) {
                this.metadata = v == null ? null : new LinkedHashMap<>(v);
                return this;
            }
            public Input build() { return new Input(this); }
        }
    }

    /** Result of {@link #build(Input)}. */
    public static final class Output {
        public final byte[] bytes;
        public final String cid;

        Output(byte[] bytes, String cid) {
            this.bytes = bytes;
            this.cid = cid;
        }
    }

    // -----------------------------------------------------------------------
    // Build
    // -----------------------------------------------------------------------

    /**
     * Build a .proof envelope from {@code input}. Output is byte-identical
     * to the rust kit's {@code build_proof_envelope} for the same canonical
     * input.
     */
    public static Output build(Input input) {
        // Step 1: encode unsigned body with sorted keys.
        List<CborPair> unsignedPairs = bodyPairsUnsigned(input);
        byte[] unsignedBytes = emitSortedMap(unsignedPairs);

        // Step 2: Ed25519-sign the unsigned bytes.
        byte[] sig = Sign.ed25519SignWithSeed(input.signerSeed, unsignedBytes);

        // Step 3: re-emit with signature added; keys re-sort automatically.
        List<CborPair> signedPairs = bodyPairsUnsigned(input);
        signedPairs.add(makeBytesPair("signature", sig));
        byte[] finalBytes = emitSortedMap(signedPairs);

        // Step 4: filename CID = full self-identifying BLAKE3-512.
        String cid = Hash.blake3_512(finalBytes);
        return new Output(finalBytes, cid);
    }

    // -----------------------------------------------------------------------
    // Verify
    // -----------------------------------------------------------------------

    /**
     * Verify a .proof envelope. Checks:
     *   1. CID matches: BLAKE3-512(proof_bytes) == expected_cid.
     *   2. CBOR decodes to a catalog map with the required keys.
     *   3. Ed25519 signature verifies: re-encode the unsigned body
     *      (all keys except {@code signature}) and verify the embedded
     *      {@code signature} bytes against {@code signerPubkey}.
     *
     * @param signerPubkey the raw 32-byte Ed25519 public key.
     */
    public static boolean verify(byte[] proofBytes, String expectedCid, byte[] signerPubkey) {
        if (proofBytes == null || expectedCid == null || signerPubkey == null) return false;
        if (signerPubkey.length != 32) return false;

        // Check 1: CID
        String actualCid = Hash.blake3_512(proofBytes);
        if (!actualCid.equals(expectedCid)) return false;

        // Check 2 + extract: decode the full signed body.
        DecodedCatalog dec;
        try {
            dec = decodeCatalog(proofBytes);
        } catch (RuntimeException e) {
            return false;
        }
        if (dec == null) return false;
        if (dec.signature == null || dec.signature.length != 64) return false;

        // Check 3: re-emit unsigned body and verify signature.
        byte[] unsignedBytes = emitSortedMap(dec.unsignedPairs);
        return Sign.ed25519VerifyRaw(signerPubkey, dec.signature, unsignedBytes);
    }

    // -----------------------------------------------------------------------
    // Body construction
    // -----------------------------------------------------------------------

    private static List<CborPair> bodyPairsUnsigned(Input input) {
        ArrayList<CborPair> pairs = new ArrayList<>();
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
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeTstr(out, key);
        return out.toByteArray();
    }

    private static CborPair makeStringPair(String key, String value) {
        ByteArrayOutputStream v = new ByteArrayOutputStream();
        Cbor.encodeTstr(v, value);
        return new CborPair(encodeKey(key), v.toByteArray());
    }

    private static CborPair makeBytesPair(String key, byte[] value) {
        ByteArrayOutputStream v = new ByteArrayOutputStream();
        Cbor.encodeBstr(v, value);
        return new CborPair(encodeKey(key), v.toByteArray());
    }

    private static CborPair makeMembersPair(String key, Map<String, byte[]> members) {
        // members value is { tstr(cid) => bstr(envelope-bytes) }, sort by
        // bytewise CBOR-encoded-key form.
        List<CborPair> pairs = new ArrayList<>();
        for (Map.Entry<String, byte[]> e : members.entrySet()) {
            pairs.add(makeBytesPair(e.getKey(), e.getValue()));
        }
        byte[] valueBytes = emitSortedMap(pairs);
        return new CborPair(encodeKey(key), valueBytes);
    }

    private static CborPair makeMetadataPair(String key, Map<String, String> metadata) {
        // metadata value is { tstr => tstr }; rust uses BTreeMap, we sort
        // by bytewise CBOR-encoded-key form to match.
        TreeMap<String, String> sorted = new TreeMap<>(metadata);
        List<CborPair> pairs = new ArrayList<>();
        for (Map.Entry<String, String> e : sorted.entrySet()) {
            pairs.add(makeStringPair(e.getKey(), e.getValue()));
        }
        byte[] valueBytes = emitSortedMap(pairs);
        return new CborPair(encodeKey(key), valueBytes);
    }

    // -----------------------------------------------------------------------
    // Sorted-map emission
    // -----------------------------------------------------------------------

    static byte[] emitSortedMap(List<CborPair> pairs) {
        // Sort by bytewise lex order of the CBOR-encoded key form
        // (RFC 8949 §4.2.1).
        ArrayList<CborPair> sorted = new ArrayList<>(pairs);
        sorted.sort((a, b) -> compareBytewise(a.keyCbor, b.keyCbor));
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeMapHead(out, sorted.size());
        for (CborPair p : sorted) {
            out.write(p.keyCbor, 0, p.keyCbor.length);
            out.write(p.valueCbor, 0, p.valueCbor.length);
        }
        return out.toByteArray();
    }

    static int compareBytewise(byte[] a, byte[] b) {
        int n = Math.min(a.length, b.length);
        for (int i = 0; i < n; i++) {
            int da = a[i] & 0xFF;
            int db = b[i] & 0xFF;
            if (da != db) return Integer.compare(da, db);
        }
        return Integer.compare(a.length, b.length);
    }

    // -----------------------------------------------------------------------
    // Minimal CBOR decoder for verify(). Only the subset we emit.
    // -----------------------------------------------------------------------

    private static final class DecodedCatalog {
        List<CborPair> unsignedPairs;
        byte[] signature;
    }

    private static DecodedCatalog decodeCatalog(byte[] bytes) {
        Reader r = new Reader(bytes);
        long count = r.readMapHead();
        if (count < 0) return null;
        DecodedCatalog dec = new DecodedCatalog();
        dec.unsignedPairs = new ArrayList<>();
        boolean kindIsCatalog = false;
        for (long i = 0; i < count; i++) {
            int keyStart = r.pos;
            String key = r.readTstr();
            int keyEnd = r.pos;
            byte[] keyCbor = java.util.Arrays.copyOfRange(bytes, keyStart, keyEnd);

            int valStart = r.pos;
            r.skipValue();
            int valEnd = r.pos;
            byte[] valCbor = java.util.Arrays.copyOfRange(bytes, valStart, valEnd);

            if ("signature".equals(key)) {
                // Decode the bytes value to extract the raw signature.
                Reader vr = new Reader(valCbor);
                dec.signature = vr.readBstr();
            } else {
                if ("kind".equals(key)) {
                    Reader vr = new Reader(valCbor);
                    String kindValue = vr.readTstr();
                    if ("catalog".equals(kindValue)) kindIsCatalog = true;
                }
                dec.unsignedPairs.add(new CborPair(keyCbor, valCbor));
            }
        }
        if (!kindIsCatalog) return null;
        return dec;
    }

    private static final class Reader {
        final byte[] buf;
        int pos;

        Reader(byte[] buf) {
            this.buf = buf;
            this.pos = 0;
        }

        long readHead(int expectedMajor) {
            int b = buf[pos++] & 0xFF;
            int major = (b >>> 5) & 0x07;
            if (major != expectedMajor) {
                throw new IllegalStateException(
                    "expected major " + expectedMajor + " at pos " + (pos - 1)
                    + ", got " + major);
            }
            int info = b & 0x1F;
            return readArg(info);
        }

        long readArg(int info) {
            if (info < 24) return info;
            if (info == 24) return buf[pos++] & 0xFFL;
            if (info == 25) {
                long v = ((buf[pos] & 0xFFL) << 8) | (buf[pos + 1] & 0xFFL);
                pos += 2;
                return v;
            }
            if (info == 26) {
                long v = ((buf[pos] & 0xFFL) << 24) | ((buf[pos + 1] & 0xFFL) << 16)
                       | ((buf[pos + 2] & 0xFFL) << 8) | (buf[pos + 3] & 0xFFL);
                pos += 4;
                return v;
            }
            if (info == 27) {
                long v = 0;
                for (int i = 0; i < 8; i++) {
                    v = (v << 8) | (buf[pos + i] & 0xFFL);
                }
                pos += 8;
                return v;
            }
            throw new IllegalStateException("unsupported additional info: " + info);
        }

        long readMapHead() { return readHead(Cbor.MAJOR_MAP); }

        String readTstr() {
            long len = readHead(Cbor.MAJOR_TEXT_STRING);
            byte[] bytes = java.util.Arrays.copyOfRange(buf, pos, (int) (pos + len));
            pos += (int) len;
            return new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
        }

        byte[] readBstr() {
            long len = readHead(Cbor.MAJOR_BYTE_STRING);
            byte[] bytes = java.util.Arrays.copyOfRange(buf, pos, (int) (pos + len));
            pos += (int) len;
            return bytes;
        }

        /** Skip any value (for re-encoding-equivalent unsigned-body extraction). */
        void skipValue() {
            int b = buf[pos++] & 0xFF;
            int major = (b >>> 5) & 0x07;
            int info = b & 0x1F;
            long arg = readArg(info);
            switch (major) {
                case Cbor.MAJOR_UNSIGNED_INT:
                    return;
                case Cbor.MAJOR_BYTE_STRING:
                case Cbor.MAJOR_TEXT_STRING:
                    pos += (int) arg;
                    return;
                case Cbor.MAJOR_ARRAY:
                    for (long i = 0; i < arg; i++) skipValue();
                    return;
                case Cbor.MAJOR_MAP:
                    for (long i = 0; i < arg; i++) {
                        skipValue();
                        skipValue();
                    }
                    return;
                default:
                    throw new IllegalStateException("unsupported major " + major + " at pos " + (pos - 1));
            }
        }
    }

    static final class CborPair {
        final byte[] keyCbor;
        final byte[] valueCbor;

        CborPair(byte[] keyCbor, byte[] valueCbor) {
            this.keyCbor = keyCbor;
            this.valueCbor = valueCbor;
        }
    }
}
