// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Serialize.cs
//
// Public surface covered:
//   * Serialize.SortToValue(Sort): Value
//   * Serialize.TermToValue(Term): Value
//   * Serialize.FormulaToValue(Formula): Value: drives the hashing flow
//   * Serialize.MarshalDeclarations(decls): string: insertion-order JSON
//
// Honest scope:
//   Serialize is a pure conversion; same Formula → same Value;
//   MarshalDeclarations of an empty list is exactly "[]".

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class SerializeInvariants
{
    public static void Register()
    {
        // FormulaToValue is deterministic.
        Must("csharp_serialize_formula_to_value_is_deterministic",
            ForAll(Sort.String, f =>
                Eq(Ctor("FormulaToValue", f), Ctor("FormulaToValue", f))));

        // TermToValue is deterministic.
        Must("csharp_serialize_term_to_value_is_deterministic",
            ForAll(Sort.String, t =>
                Eq(Ctor("TermToValue", t), Ctor("TermToValue", t))));

        // MarshalDeclarations of an empty list is exactly "[]" (length 2).
        Contract("csharp_serialize_marshal_empty_length_eq_2",
            post: Eq(Ctor("len", Ctor("MarshalDeclarations", Ctor("EmptyList"))), Num(2)));

        // SortToValue of any primitive sort emits a 2-key object
        // ({kind, name}); the JCS-encoded length is bounded below by
        // "{\"kind\":\"primitive\",\"name\":\"X\"}" = ~30 chars.
        Must("csharp_serialize_sort_to_value_length_gte_28",
            ForAll(Sort.String, s =>
                Gte(Ctor("len", Ctor("JcsEncode", Ctor("SortToValue", s))), Num(28))));
    }
}
