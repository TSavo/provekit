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
// Backed by NSec.Cryptography (libsodium-backed Ed25519 with safe key
// material handling). Mirrors the Rust ed25519-dalek peer 1:1 at the
// signature-bytes level (both are RFC 8032 compliant).

using NSec.Cryptography;

namespace Provekit.ProofEnvelope;

public static class Sign
{
    public const string Ed25519SigPrefix = "ed25519:";
    public const string Ed25519KeyPrefix = "ed25519:";

    /// <summary>
    /// Sign <paramref name="message"/> with the Ed25519 private key
    /// derived from <paramref name="seed"/> (32 bytes). Returns the raw
    /// 64-byte signature.
    /// </summary>
    public static byte[] Ed25519SignWithSeed(ReadOnlySpan<byte> seed, ReadOnlySpan<byte> message)
    {
        if (seed.Length != 32)
        {
            throw new ArgumentException("seed must be 32 bytes", nameof(seed));
        }
        // NSec uses an Ed25519 "secret key" import format equal to the
        // 32-byte seed (RFC 8032 §5.1.5). KeyExportPolicies.None is fine
        // since we only sign and don't export.
        using var key = Key.Import(
            SignatureAlgorithm.Ed25519,
            seed,
            KeyBlobFormat.RawPrivateKey);
        return SignatureAlgorithm.Ed25519.Sign(key, message);
    }

    /// <summary>
    /// Sign and return the spec's self-identifying string form
    /// (<c>"ed25519:" + base64(sig)</c>).
    /// </summary>
    public static string Ed25519SignString(ReadOnlySpan<byte> seed, ReadOnlySpan<byte> message)
    {
        var sig = Ed25519SignWithSeed(seed, message);
        return Ed25519SigPrefix + Convert.ToBase64String(sig);
    }

    /// <summary>
    /// Derive the public key from <paramref name="seed"/> and return
    /// the self-identifying string form
    /// (<c>"ed25519:" + base64(pubkey)</c>).
    /// </summary>
    public static string Ed25519PubkeyString(ReadOnlySpan<byte> seed)
    {
        if (seed.Length != 32)
        {
            throw new ArgumentException("seed must be 32 bytes", nameof(seed));
        }
        using var key = Key.Import(
            SignatureAlgorithm.Ed25519,
            seed,
            KeyBlobFormat.RawPrivateKey);
        var pub = key.PublicKey.Export(KeyBlobFormat.RawPublicKey);
        return Ed25519KeyPrefix + Convert.ToBase64String(pub);
    }

    /// <summary>
    /// Verify <paramref name="message"/> against
    /// <paramref name="sigString"/> (spec form
    /// <c>"ed25519:" + base64(sig)</c>) using
    /// <paramref name="pubkeyString"/> (spec form
    /// <c>"ed25519:" + base64(pubkey)</c>). Returns false for any
    /// malformed input rather than throwing.
    /// </summary>
    public static bool Ed25519VerifyString(string pubkeyString, string sigString, ReadOnlySpan<byte> message)
    {
        if (!pubkeyString.StartsWith(Ed25519KeyPrefix, StringComparison.Ordinal)) return false;
        if (!sigString.StartsWith(Ed25519SigPrefix, StringComparison.Ordinal)) return false;
        byte[] pkBytes;
        byte[] sigBytes;
        try
        {
            pkBytes = Convert.FromBase64String(pubkeyString[Ed25519KeyPrefix.Length..]);
            sigBytes = Convert.FromBase64String(sigString[Ed25519SigPrefix.Length..]);
        }
        catch (FormatException)
        {
            return false;
        }
        if (pkBytes.Length != 32 || sigBytes.Length != 64) return false;
        try
        {
            var pk = PublicKey.Import(SignatureAlgorithm.Ed25519, pkBytes, KeyBlobFormat.RawPublicKey);
            return SignatureAlgorithm.Ed25519.Verify(pk, message, sigBytes);
        }
        catch
        {
            return false;
        }
    }
}
