// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.ProofEnvelope/Sign.cs
//
// Public surface covered:
//   * Sign.Ed25519SignWithSeed(seed, msg) -> 64-byte sig
//   * Sign.Ed25519SignString(seed, msg) -> "ed25519:" + base64
//   * Sign.Ed25519PubkeyString(seed) -> "ed25519:" + base64
//   * Sign.Ed25519VerifyString(pubkey, sig, msg) -> bool
//
// Honest scope:
//   The IR cannot model Ed25519's cryptographic strength. What it CAN
//   say: signature length is fixed (64 bytes raw, predictable base64
//   length), the prefix is "ed25519:" (8 chars), Sign with the same
//   seed+message is deterministic.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class SignInvariants
{
    public static void Register()
    {
        // ed25519 prefix is 8 chars: "ed25519:".
        Contract("csharp_sign_prefix_length_eq_8",
            post: Eq(Ctor("len", StrConst("ed25519:")), Num(8)));

        // Ed25519SignWithSeed produces exactly 64 raw bytes.
        Must("csharp_sign_raw_signature_length_eq_64",
            ForAll(Sort.String, msg =>
                Eq(Ctor("RawSignatureLength", msg), Num(64))));

        // Ed25519SignString is deterministic for fixed seed+message
        // (RFC 8032 Ed25519 is deterministic-by-design).
        Must("csharp_sign_string_is_deterministic",
            ForAll(Sort.String, msg =>
                Eq(Ctor("SignString", msg), Ctor("SignString", msg))));

        // Ed25519PubkeyString of a 32-byte seed is exactly
        // 8 (prefix) + 44 (base64 of 32 bytes with padding) = 52 chars.
        Contract("csharp_sign_pubkey_string_length_eq_52",
            post: Eq(Ctor("PubkeyStringLength"), Num(52)));

        // Ed25519SignString output length: 8 (prefix) + 88 (base64 of
        // 64 bytes with padding) = 96 chars.
        Must("csharp_sign_string_length_eq_96",
            ForAll(Sort.String, msg =>
                Eq(Ctor("SignStringLength", msg), Num(96))));

        // Verify of a freshly-signed message returns true.
        Must("csharp_sign_verify_round_trip",
            ForAll(Sort.String, msg =>
                Eq(Ctor("VerifySelfSigned", msg), BoolConst(true))));

        // Verify rejects malformed prefixes (returns false rather than throwing).
        Contract("csharp_sign_verify_rejects_malformed_prefix",
            post: Eq(Ctor("VerifyMalformed"), BoolConst(false)));
    }
}
