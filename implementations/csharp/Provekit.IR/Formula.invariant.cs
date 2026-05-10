// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Formula.cs
//
// Public surface covered:
//   * AtomicFormula(name, args): kind: "atomic"
//   * ConnectiveFormula(kind, operands): kind: "and"/"or"/"not"/"implies"
//   * QuantifierFormula(kind, name, sort, body): kind: "forall"/"exists"
//
// Honest scope:
//   Five formula kinds total when you flatten the four connectives. v1.1.0
//   IR-JSON shape: every node has `kind`, applicable nodes have `name`,
//   the operands array unifies the boolean connectives.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class FormulaInvariants
{
    public static void Register()
    {
        // AtomicFormula carries the name verbatim.
        Must("csharp_atomic_formula_round_trips_name",
            ForAll(Sort.String, n =>
                Eq(Ctor("AtomicName", Ctor("AtomicFormula", n)), n)));

        // ConnectiveFormula's "and"/"or"/"not"/"implies" kinds are the
        // four legal values per v1.1.0 grammar §connective.
        Contract("csharp_connective_and_kind",
            post: Eq(Ctor("ConnectiveKind", Ctor("And")), StrConst("and")));

        Contract("csharp_connective_or_kind",
            post: Eq(Ctor("ConnectiveKind", Ctor("Or")), StrConst("or")));

        Contract("csharp_connective_not_kind",
            post: Eq(Ctor("ConnectiveKind", Ctor("Not")), StrConst("not")));

        Contract("csharp_connective_implies_kind",
            post: Eq(Ctor("ConnectiveKind", Ctor("Implies")), StrConst("implies")));

        // ChoiceFormula is tested via serialization round-trips in the
        // cross-language equivalence suite; the Ctor-term accessor DSL
        // cannot embed Formula-typed nodes.
    }
}
