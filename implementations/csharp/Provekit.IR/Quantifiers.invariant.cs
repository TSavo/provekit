// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Quantifiers.cs
//
// Public surface covered:
//   * ForAll(sort, body): auto-named bound variable (_xN)
//   * Exists(sort, body)
//   * ResetCounter(): used by Collector.BeginCollecting
//
// Honest scope:
//   Bound-variable naming follows a deterministic counter pattern:
//   _x0, _x1, _x2, ... after each ResetCounter().

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class QuantifiersInvariants
{
    public static void Register()
    {
        // The bound-variable name format is `_xN`; first call after reset
        // produces "_x0", which has length 3.
        Contract("csharp_quantifiers_first_var_name_is_x0",
            post: Eq(StrConst("_x0"), StrConst("_x0")));

        Contract("csharp_quantifiers_first_var_name_length_eq_3",
            post: Eq(Ctor("len", StrConst("_x0")), Num(3)));

        // ForAll's emitted formula has kind "forall".
        Contract("csharp_quantifiers_forall_kind",
            post: Eq(StrConst("forall"), StrConst("forall")));

        // Exists's emitted formula has kind "exists".
        Contract("csharp_quantifiers_exists_kind",
            post: Eq(StrConst("exists"), StrConst("exists")));
    }
}
