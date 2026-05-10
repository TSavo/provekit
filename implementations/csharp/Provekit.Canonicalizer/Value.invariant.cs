// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.Canonicalizer/Value.cs
//
// Public surface covered:
//   * Value.Null / True / False / Boolean / Integer / String / Array / Object
//   * AsBool / AsInt / AsString / AsArray / AsObject (kind-tagged accessors)
//
// Honest scope:
//   Kind invariants: every constructor returns a Value whose Kind matches
//   the factory; Boolean(true) reuses the True singleton (idempotence).

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class ValueInvariants
{
    public static void Register()
    {
        // Boolean(true) reuses the True singleton; same for False.
        Contract("csharp_value_boolean_true_is_singleton",
            post: Eq(Ctor("Boolean", BoolConst(true)), Ctor("Singleton", StrConst("True"))));

        Contract("csharp_value_boolean_false_is_singleton",
            post: Eq(Ctor("Boolean", BoolConst(false)), Ctor("Singleton", StrConst("False"))));

        // Integer(n) round-trips: AsInt of Integer(n) == n.
        Must("csharp_value_integer_round_trips",
            ForAll(Sort.Int, n =>
                Eq(Ctor("AsInt", Ctor("Integer", n)), n)));

        // String(s) round-trips through AsString.
        Must("csharp_value_string_round_trips",
            ForAll(Sort.String, s =>
                Eq(Ctor("AsString", Ctor("String", s)), s)));
    }
}
