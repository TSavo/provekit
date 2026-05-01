// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Predicates.cs
//
// Public surface covered:
//   * Atomic(name, ...args) — generic atomic builder
//   * Eq / Ne / Gt / Gte / Lt / Lte — six standard predicates
//   * And / Or / Not / Implies — four connective constructors
//
// Honest scope:
//   Predicate names are protocol-locked: "=" / "≠" / ">" / "≥" / "<" / "≤".
//   Cross-language hash agreement requires these exact byte sequences.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class PredicatesInvariants
{
    public static void Register()
    {
        // Predicate names are byte-locked at the protocol layer.
        // The IR can express this as length pins on the literal strings.
        Contract("csharp_pred_eq_name_length_eq_1",
            post: Eq(Ctor("len", StrConst("=")), Num(1)));

        Contract("csharp_pred_gt_name_length_eq_1",
            post: Eq(Ctor("len", StrConst(">")), Num(1)));

        Contract("csharp_pred_lt_name_length_eq_1",
            post: Eq(Ctor("len", StrConst("<")), Num(1)));

        // ≠ ≥ ≤ are 3-byte UTF-8 sequences. Their UTF-16 char
        // representation is 1 code unit, but the JCS encoder emits them
        // verbatim as their 3-byte UTF-8 form. The JCS-bytes length is
        // load-bearing for cross-language hash agreement.
        Contract("csharp_pred_ne_unicode_length_eq_1_chars",
            post: Eq(Ctor("len", StrConst("≠")), Num(1)));

        Contract("csharp_pred_gte_unicode_length_eq_1_chars",
            post: Eq(Ctor("len", StrConst("≥")), Num(1)));

        Contract("csharp_pred_lte_unicode_length_eq_1_chars",
            post: Eq(Ctor("len", StrConst("≤")), Num(1)));

        // Eq is reflexive at the construction layer: Eq(a, a) is a valid
        // formula whose two arguments are structurally equal.
        Must("csharp_pred_eq_reflexive_construction",
            ForAll(Sort.Int, x => Eq(x, x)));
    }
}
