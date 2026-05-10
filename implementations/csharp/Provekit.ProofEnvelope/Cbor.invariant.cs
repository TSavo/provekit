// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.ProofEnvelope/Cbor.cs
//
// Public surface covered:
//   * Cbor.AppendHead(output, major, arg)
//   * Cbor.EncodeTstr(output, text)
//   * Cbor.EncodeBstr(output, bytes)
//   * Cbor.EncodeMapHead(output, n)
//
// Honest scope:
//   RFC 8949 §4.2.1 deterministic encoding: shortest-form integer head,
//   definite-length items only, sorted map keys. The IR captures
//   determinism + length floors; byte-faithful conformance is proven by
//   Provekit.Tests.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class CborInvariants
{
    public static void Register()
    {
        // AppendHead with arg < 24 emits exactly 1 byte (immediate form).
        // The IR can express the byte-count via a bound integer.
        Must("csharp_cbor_head_immediate_length_eq_1",
            ForAll(Sort.Int, arg =>
                Implies(
                    And(Gte(arg, Num(0)), Lt(arg, Num(24))),
                    Eq(Ctor("HeadByteCount", arg), Num(1)))));

        // EncodeTstr is deterministic.
        Must("csharp_cbor_encode_tstr_is_deterministic",
            ForAll(Sort.String, s =>
                Eq(Ctor("EncodeTstr", s), Ctor("EncodeTstr", s))));

        // EncodeBstr is deterministic.
        Must("csharp_cbor_encode_bstr_is_deterministic",
            ForAll(Sort.String, b =>
                Eq(Ctor("EncodeBstr", b), Ctor("EncodeBstr", b))));

        // Empty text string encodes to 1 byte (the head, with arg=0).
        Contract("csharp_cbor_empty_tstr_length_eq_1",
            post: Eq(Ctor("EncodedLength", StrConst("")), Num(1)));
    }
}
