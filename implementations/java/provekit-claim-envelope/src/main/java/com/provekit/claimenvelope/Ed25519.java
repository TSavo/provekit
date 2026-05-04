// SPDX-License-Identifier: Apache-2.0
//
// Ed25519 signing helper. v1.1.0 of the protocol mandates
// self-identifying signatures of the form:
//
//   "ed25519:" + base64-stdpad(64-byte-signature)
//
// And self-identifying public keys of the same form. The .proof file
// envelope itself stores its catalog signature as a RAW 64-byte CBOR
// byte string (not the prefixed string form): only the per-memento
// `producerSignature` field uses the prefixed string form, because
// memento envelopes are JCS-JSON.
//
// Backed by BouncyCastle (RFC 8032 Ed25519). Byte-equivalent to the
// rust ed25519-dalek peer for the same (seed, message) pair.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/sign.rs and
// implementations/csharp/Provekit.ProofEnvelope/Sign.cs 1:1.

package com.provekit.claimenvelope;

import java.util.Base64;

import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters;
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters;
import org.bouncycastle.crypto.signers.Ed25519Signer;

public final class Ed25519 {
    public static final String SIG_PREFIX = "ed25519:";
    public static final String KEY_PREFIX = "ed25519:";
    public static final int SEED_BYTES = 32;
    public static final int PUBKEY_BYTES = 32;
    public static final int SIGNATURE_BYTES = 64;

    /**
     * Foundation v0 publicly-known test seed: 32 bytes of 0x42.
     * Source: tools/foundation-keygen/src/lib.rs FOUNDATION_V0_SEED.
     * v1 is HSM-generated; do not use this for production signing.
     */
    public static final byte[] FOUNDATION_V0_SEED;
    static {
        FOUNDATION_V0_SEED = new byte[SEED_BYTES];
        for (int i = 0; i < SEED_BYTES; i++) {
            FOUNDATION_V0_SEED[i] = (byte) 0x42;
        }
    }

    private Ed25519() {}

    /** Sign {@code message} with the Ed25519 private key derived from {@code seed}. Returns 64 bytes. */
    public static byte[] signWithSeed(byte[] seed, byte[] message) {
        if (seed == null || seed.length != SEED_BYTES) {
            throw new IllegalArgumentException("seed must be exactly " + SEED_BYTES + " bytes");
        }
        Ed25519PrivateKeyParameters sk = new Ed25519PrivateKeyParameters(seed, 0);
        Ed25519Signer signer = new Ed25519Signer();
        signer.init(true, sk);
        signer.update(message, 0, message.length);
        return signer.generateSignature();
    }

    /** "ed25519:" + base64-stdpad of the 64-byte signature. */
    public static String signString(byte[] seed, byte[] message) {
        byte[] sig = signWithSeed(seed, message);
        return SIG_PREFIX + Base64.getEncoder().encodeToString(sig);
    }

    /** Derive the 32-byte public key from {@code seed}. */
    public static byte[] pubkeyBytes(byte[] seed) {
        if (seed == null || seed.length != SEED_BYTES) {
            throw new IllegalArgumentException("seed must be exactly " + SEED_BYTES + " bytes");
        }
        Ed25519PrivateKeyParameters sk = new Ed25519PrivateKeyParameters(seed, 0);
        Ed25519PublicKeyParameters vk = sk.generatePublicKey();
        return vk.getEncoded();
    }

    /** "ed25519:" + base64-stdpad of the 32-byte public key. */
    public static String pubkeyString(byte[] seed) {
        byte[] pk = pubkeyBytes(seed);
        return KEY_PREFIX + Base64.getEncoder().encodeToString(pk);
    }

    /**
     * Verify {@code message} against a self-identifying string signature using a
     * self-identifying string public key. Returns false (never throws) for any
     * malformed input or invalid signature.
     */
    public static boolean verifyString(String pubkeyString, String sigString, byte[] message) {
        if (pubkeyString == null || sigString == null || message == null) return false;
        if (!pubkeyString.startsWith(KEY_PREFIX)) return false;
        if (!sigString.startsWith(SIG_PREFIX)) return false;
        byte[] pkBytes;
        byte[] sigBytes;
        try {
            pkBytes = Base64.getDecoder().decode(pubkeyString.substring(KEY_PREFIX.length()));
            sigBytes = Base64.getDecoder().decode(sigString.substring(SIG_PREFIX.length()));
        } catch (IllegalArgumentException ex) {
            return false;
        }
        if (pkBytes.length != PUBKEY_BYTES || sigBytes.length != SIGNATURE_BYTES) return false;
        return verifyBytes(pkBytes, sigBytes, message);
    }

    /** Verify a raw 64-byte signature against a raw 32-byte public key. */
    public static boolean verifyBytes(byte[] pubkey, byte[] signature, byte[] message) {
        if (pubkey == null || pubkey.length != PUBKEY_BYTES) return false;
        if (signature == null || signature.length != SIGNATURE_BYTES) return false;
        if (message == null) return false;
        try {
            Ed25519PublicKeyParameters vk = new Ed25519PublicKeyParameters(pubkey, 0);
            Ed25519Signer verifier = new Ed25519Signer();
            verifier.init(false, vk);
            verifier.update(message, 0, message.length);
            return verifier.verifySignature(signature);
        } catch (RuntimeException ex) {
            return false;
        }
    }
}
