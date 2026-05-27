// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class FormulaContracts
{
    internal static int csharp_atomic_formula_round_trips_name(string name)
    {
        if (AtomicName(name) != name) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_connective_and_kind()
    {
        if (ConnectiveKind(Predicates.And()) != "and") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_connective_or_kind()
    {
        if (ConnectiveKind(Predicates.Or()) != "or") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_connective_not_kind()
    {
        if (ConnectiveKind(Predicates.Not(Predicates.And())) != "not") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_connective_implies_kind()
    {
        if (ConnectiveKind(Predicates.Implies(Predicates.And(), Predicates.And())) != "implies")
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    private static string AtomicName(string name) => ((AtomicFormula)Predicates.Atomic(name)).Name;

    private static string ConnectiveKind(Formula formula) => ((ConnectiveFormula)formula).Kind;
}
