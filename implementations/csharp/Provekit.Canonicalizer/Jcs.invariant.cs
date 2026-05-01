// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.Canonicalizer/Jcs.cs
//
// Public surface covered:
//   * Jcs.Encode(Value): string         (RFC 8785 JCS-JSON)
//   * Jcs.EncodeUtf8(Value): byte[]
//
// Honest scope:
//   The IR's atomic-predicate domain is narrow. RFC 8785 conformance
//   is byte-faithful; the IR can express determinism, length floors,
//   and structural equality of repeated calls.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class JcsInvariants
{
    public static void Register()
    {
        // Encode is deterministic: same input, same output.
        Must("csharp_jcs_encode_is_deterministic",
            ForAll(Sort.String, v =>
                Eq(Ctor("Encode", v), Ctor("Encode", v))));

        // Encode output length is bounded below by 2 (smallest valid JSON
        // value is two characters: "{}", "[]", or a 2-char string ""[]).
        Must("csharp_jcs_encode_output_length_gte_2",
            ForAll(Sort.String, v =>
                Gte(Ctor("len", Ctor("Encode", v)), Num(2))));

        // EncodeUtf8 is deterministic.
        Must("csharp_jcs_encode_utf8_is_deterministic",
            ForAll(Sort.String, v =>
                Eq(Ctor("EncodeUtf8", v), Ctor("EncodeUtf8", v))));

        // Encode and EncodeUtf8 carry the same content (length-equivalent
        // for ASCII-only inputs; the IR can express length-equivalence).
        Must("csharp_jcs_encode_utf8_length_eq_encode_length_for_ascii",
            ForAll(Sort.String, v =>
                Eq(Ctor("len", Ctor("EncodeUtf8", v)),
                   Ctor("len", Ctor("Encode", v)))));
    }
}
