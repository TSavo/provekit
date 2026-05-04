// SPDX-License-Identifier: Apache-2.0
//
// Ed25519 signing helper. v1.1.0 of the protocol mandates self-
// identifying signatures of the form:
//
//   "ed25519:" + base64-stdpad(64-byte-signature)
//
// And self-identifying public keys in the same form. The .proof file
// envelope itself stores its catalog signature as a RAW 64-byte CBOR
// byte string (not the prefixed string form): only the per-memento
// `producerSignature` field uses the prefixed string form, because
// memento envelopes are JCS-JSON.
//
// Backed by JDK 15+ {@code java.security.Signature.getInstance("Ed25519")}
// (RFC 8032 compliant). Byte-equivalent to the rust ed25519-dalek peer
// for the same (seed, message) pair.
//
// PKCS#8 raw seed wrap: the JDK accepts a 32-byte Ed25519 seed via
// {@link NamedParameterSpec#ED25519} + {@link EdECPrivateKeySpec} since
// JDK 15. We use that path (no third-party crypto provider needed).

package com.provekit.proofenvelope;

import java.security.GeneralSecurityException;
import java.security.KeyFactory;
import java.security.KeyPairGenerator;
import java.security.PrivateKey;
import java.security.PublicKey;
import java.security.SecureRandom;
import java.security.Signature;
import java.security.interfaces.EdECPublicKey;
import java.security.spec.EdECPoint;
import java.security.spec.EdECPrivateKeySpec;
import java.security.spec.EdECPublicKeySpec;
import java.security.spec.NamedParameterSpec;
import java.util.Base64;

public final class Sign {

    private Sign() {}

    public static final String ED25519_SIG_PREFIX = "ed25519:";
    public static final String ED25519_KEY_PREFIX = "ed25519:";

    /**
     * Sign {@code message} with the Ed25519 private key derived from
     * {@code seed} (32 bytes). Returns the raw 64-byte signature.
     * Byte-identical to the rust {@code ed25519_sign_with_seed} for the
     * same (seed, message) pair.
     */
    public static byte[] ed25519SignWithSeed(byte[] seed, byte[] message) {
        if (seed == null || seed.length != 32) {
            throw new IllegalArgumentException("seed must be exactly 32 bytes");
        }
        try {
            KeyFactory kf = KeyFactory.getInstance("Ed25519");
            EdECPrivateKeySpec privSpec = new EdECPrivateKeySpec(NamedParameterSpec.ED25519, seed.clone());
            PrivateKey privateKey = kf.generatePrivate(privSpec);
            Signature signer = Signature.getInstance("Ed25519");
            signer.initSign(privateKey);
            signer.update(message);
            return signer.sign();
        } catch (RuntimeException e) {
            throw e;
        } catch (Exception e) {
            throw new RuntimeException("Ed25519 sign failed: " + e.getMessage(), e);
        }
    }

    /**
     * Sign and return the spec's self-identifying string form
     * ({@code "ed25519:" + base64(sig)}).
     */
    public static String ed25519SignString(byte[] seed, byte[] message) {
        byte[] sig = ed25519SignWithSeed(seed, message);
        return ED25519_SIG_PREFIX + Base64.getEncoder().encodeToString(sig);
    }

    /**
     * Derive the public key from {@code seed} and return the raw 32-byte
     * Ed25519 public key bytes.
     */
    public static byte[] ed25519PubkeyBytes(byte[] seed) {
        if (seed == null || seed.length != 32) {
            throw new IllegalArgumentException("seed must be exactly 32 bytes");
        }
        // The JDK's Ed25519 KeyPairGenerator reads 32 bytes from its
        // SecureRandom and uses them as the RFC 8032 seed — exactly the
        // seed format we already have. Feed our seed in via a one-shot
        // SecureRandom subclass and the JDK returns the matching public
        // key with no curve arithmetic on our side.
        SecureRandom seedRng = new SeededOnceRandom(seed);
        try {
            KeyPairGenerator kpg = KeyPairGenerator.getInstance("Ed25519");
            kpg.initialize(NamedParameterSpec.ED25519, seedRng);
            EdECPublicKey pub = (EdECPublicKey) kpg.generateKeyPair().getPublic();
            return encodeEdECPoint(pub.getPoint());
        } catch (GeneralSecurityException e) {
            throw new RuntimeException("Ed25519 pubkey derive failed: " + e.getMessage(), e);
        }
    }

    /**
     * One-shot {@link SecureRandom} that returns the supplied 32-byte
     * seed for the first {@link SecureRandom#nextBytes(byte[])} call and
     * throws on any subsequent draw. The JDK's Ed25519
     * {@link KeyPairGenerator} draws exactly 32 bytes once during
     * generation; if it ever changes that, we want the loud failure.
     */
    private static final class SeededOnceRandom extends SecureRandom {
        private static final long serialVersionUID = 1L;
        private final byte[] seed;
        private boolean consumed;

        SeededOnceRandom(byte[] seed) {
            super(new java.security.SecureRandomSpi() {
                @Override protected void engineSetSeed(byte[] s) {}
                @Override protected void engineNextBytes(byte[] bytes) {
                    throw new IllegalStateException("SeededOnceRandom: provider engine should not be invoked");
                }
                @Override protected byte[] engineGenerateSeed(int numBytes) {
                    throw new IllegalStateException("SeededOnceRandom: engineGenerateSeed not supported");
                }
            }, null);
            this.seed = seed.clone();
        }

        @Override public void nextBytes(byte[] out) {
            if (consumed) {
                throw new IllegalStateException("SeededOnceRandom already consumed");
            }
            if (out.length != 32) {
                throw new IllegalStateException("unexpected RNG draw: len=" + out.length);
            }
            System.arraycopy(seed, 0, out, 0, 32);
            consumed = true;
        }
    }

    /**
     * Inverse of {@link #decodeEdECPoint}: encode the (x-odd-flag, y)
     * pair as a 32-byte little-endian Ed25519 public key per RFC 8032.
     */
    private static byte[] encodeEdECPoint(EdECPoint point) {
        byte[] big = point.getY().toByteArray(); // big-endian, two's-complement
        byte[] le = new byte[32];
        int len = big.length;
        for (int i = 0; i < 32 && i < len; i++) {
            le[i] = big[len - 1 - i];
        }
        if (point.isXOdd()) {
            le[31] |= (byte) 0x80;
        }
        return le;
    }

    /**
     * Derive the public key from {@code seed} and return the spec's
     * self-identifying string form ({@code "ed25519:" + base64(pubkey)}).
     */
    public static String ed25519PubkeyString(byte[] seed) {
        return ED25519_KEY_PREFIX + Base64.getEncoder().encodeToString(ed25519PubkeyBytes(seed));
    }

    /**
     * Verify {@code message} against {@code sigString} (spec form
     * {@code "ed25519:" + base64(sig)}) using {@code pubkeyString}
     * (spec form {@code "ed25519:" + base64(pubkey)}). Returns false
     * for any malformed input rather than throwing.
     */
    public static boolean ed25519VerifyString(String pubkeyString, String sigString, byte[] message) {
        if (pubkeyString == null || !pubkeyString.startsWith(ED25519_KEY_PREFIX)) return false;
        if (sigString == null || !sigString.startsWith(ED25519_SIG_PREFIX)) return false;
        byte[] pkBytes;
        byte[] sigBytes;
        try {
            pkBytes = Base64.getDecoder().decode(pubkeyString.substring(ED25519_KEY_PREFIX.length()));
            sigBytes = Base64.getDecoder().decode(sigString.substring(ED25519_SIG_PREFIX.length()));
        } catch (IllegalArgumentException e) {
            return false;
        }
        if (pkBytes.length != 32 || sigBytes.length != 64) return false;
        return ed25519VerifyRaw(pkBytes, sigBytes, message);
    }

    /**
     * Verify a raw 64-byte signature against {@code message} using the
     * raw 32-byte {@code pubkey}. Returns false on any failure.
     */
    public static boolean ed25519VerifyRaw(byte[] pubkey, byte[] signature, byte[] message) {
        if (pubkey == null || pubkey.length != 32) return false;
        if (signature == null || signature.length != 64) return false;
        try {
            EdECPoint point = decodeEdECPoint(pubkey);
            EdECPublicKeySpec pubSpec = new EdECPublicKeySpec(NamedParameterSpec.ED25519, point);
            KeyFactory kf = KeyFactory.getInstance("Ed25519");
            PublicKey publicKey = kf.generatePublic(pubSpec);
            Signature verifier = Signature.getInstance("Ed25519");
            verifier.initVerify(publicKey);
            verifier.update(message);
            return verifier.verify(signature);
        } catch (Exception e) {
            return false;
        }
    }

    /**
     * Decode a 32-byte little-endian Ed25519 public key into the
     * (x-odd-flag, y) form expected by {@link EdECPoint}. Per RFC 8032,
     * the encoded form is the 255-bit y-coordinate with the high bit of
     * the last byte holding the sign of x.
     */
    private static EdECPoint decodeEdECPoint(byte[] pubkey) {
        byte[] yBytes = pubkey.clone();
        boolean xOdd = (yBytes[31] & 0x80) != 0;
        yBytes[31] = (byte) (yBytes[31] & 0x7F);
        // EdECPoint expects big-endian BigInteger.
        byte[] yBigEndian = new byte[32];
        for (int i = 0; i < 32; i++) {
            yBigEndian[i] = yBytes[31 - i];
        }
        return new EdECPoint(xOdd, new java.math.BigInteger(1, yBigEndian));
    }
}
