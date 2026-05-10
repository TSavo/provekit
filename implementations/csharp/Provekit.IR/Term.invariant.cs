// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Term.cs
//
// Public surface covered:
//   * Terms.Var / Num / StrConst / BoolConst / Ctor / Out
//   * VarTerm / ConstTerm / CtorTerm record kinds (v1.1.0: var/ctor have NO sort)
//
// Honest scope:
//   Term constructors are total functions; they preserve their inputs
//   verbatim and tag with the right kind. We assert the structural
//   round-trips and Out()'s name pin.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class TermInvariants
{
    public static void Register()
    {
        // Out() is a VarTerm whose name is exactly "out" (the default
        // outBinding). This pin is protocol-load-bearing: `post`
        // formulas reference `out` symbolically.
        Contract("csharp_terms_out_name_is_out",
            post: Eq(Ctor("VarName", Ctor("Out")), StrConst("out")));

        // Var(name).Name == name: Var preserves its name.
        Must("csharp_terms_var_round_trips_name",
            ForAll(Sort.String, n =>
                Eq(Ctor("VarName", Ctor("Var", n)), n)));

        // Num(n) round-trips its integer payload.
        Must("csharp_terms_num_round_trips_value",
            ForAll(Sort.Int, n =>
                Eq(Ctor("ConstIntValue", Ctor("Num", n)), n)));

        // StrConst(s) round-trips its string payload.
        Must("csharp_terms_str_const_round_trips_value",
            ForAll(Sort.String, s =>
                Eq(Ctor("ConstStrValue", Ctor("StrConst", s)), s)));

        // Lambda(paramName, paramSort, body).ParamName == paramName
        Must("csharp_terms_lambda_round_trips_param_name",
            ForAll(Sort.String, pn =>
                ForAll(Sort.Int, body =>
                    Eq(Ctor("LambdaParamName", Ctor("Lambda", pn, StrConst("Int"), body)), pn))));

        // Let with bindings round-trips
        Must("csharp_terms_let_has_bindings",
            ForAll(Sort.Int, x =>
                Eq(Ctor("LetBindingCount", Ctor("Let", Ctor("Binding", StrConst("x"), x), x)), Num(1))));
    }
}
