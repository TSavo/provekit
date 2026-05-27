// SPDX-License-Identifier: Apache-2.0

using Provekit.Canonicalizer;

namespace Provekit.IR;

internal static class SerializeContracts
{
    internal static int csharp_serialize_formula_to_value_is_deterministic(string name)
    {
        if (FormulaValueJcs(name) != FormulaValueJcs(name)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_serialize_term_to_value_is_deterministic(string name)
    {
        if (TermValueJcs(name) != TermValueJcs(name)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_serialize_marshal_empty_length_eq_2()
    {
        if (Serialize.MarshalDeclarations(Array.Empty<ContractDecl>()).Length != 2)
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    internal static int csharp_serialize_sort_to_value_length_gte_28(string name)
    {
        if (SortValueJcsLength(name) < 28) throw new InvalidOperationException("contract");
        return 1;
    }

    private static string FormulaValueJcs(string name) =>
        Jcs.Encode(Serialize.FormulaToValue(Predicates.Atomic(name, Terms.Var("x"))));

    private static string TermValueJcs(string name) =>
        Jcs.Encode(Serialize.TermToValue(Terms.Ctor(name, Terms.Var("x"))));

    private static int SortValueJcsLength(string name) =>
        Jcs.Encode(Serialize.SortToValue(new Sort.Primitive(name))).Length;
}
