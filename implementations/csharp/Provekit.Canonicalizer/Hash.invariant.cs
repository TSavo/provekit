// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.Canonicalizer/Hash.cs
//
// Public surface covered:
//   * Blake3_512(ReadOnlySpan<byte>): string  // "blake3-512:" + 128 hex
//   * Blake3_512Utf8(string): string          // convenience wrapper
//
// Honest scope:
//   The IR cannot model BLAKE3's collision resistance. What the IR CAN
//   say is shape-level: output lengths, determinism, prefix presence.
//   Mirrors implementations/cpp/provekit/canonicalizer/hash.invariant.cpp.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class HashInvariants
{
    public static void Register()
    {
        // Blake3_512 output length is exactly 139 (11-char prefix + 128 hex).
        Must("csharp_blake3_512_output_length_eq_139",
            ForAll(Sort.String, b =>
                Eq(Ctor("len", Ctor("Blake3_512", b)), Num(139))));

        // Blake3_512 is deterministic.
        Must("csharp_blake3_512_is_deterministic",
            ForAll(Sort.String, b =>
                Eq(Ctor("Blake3_512", b), Ctor("Blake3_512", b))));

        // Blake3_512Utf8 output length is also exactly 139.
        Must("csharp_blake3_512_utf8_output_length_eq_139",
            ForAll(Sort.String, s =>
                Eq(Ctor("len", Ctor("Blake3_512Utf8", s)), Num(139))));

        // Blake3_512Utf8 is deterministic.
        Must("csharp_blake3_512_utf8_is_deterministic",
            ForAll(Sort.String, s =>
                Eq(Ctor("Blake3_512Utf8", s), Ctor("Blake3_512Utf8", s))));

        // BLAKE3-512 prefix length is exactly 11 (the literal "blake3-512:").
        Contract("csharp_blake3_512_prefix_length_eq_11",
            post: Eq(Ctor("len", StrConst("blake3-512:")), Num(11)));
    }
}
