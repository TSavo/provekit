// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class TermContracts
{
    internal static int csharp_terms_out_name_is_out()
    {
        if (VarName(Terms.Out()) != "out") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_terms_var_round_trips_name(string name)
    {
        if (VarName(Terms.Var(name)) != name) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_terms_num_round_trips_value(long value)
    {
        if (ConstIntValue(Terms.Num(value)) != value) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_terms_str_const_round_trips_value(string value)
    {
        if (ConstStringValue(Terms.StrConst(value)) != value) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_terms_lambda_round_trips_param_name(string paramName, long body)
    {
        if (LambdaParamName(paramName, body) != paramName) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_terms_let_has_bindings(long value)
    {
        if (LetBindingCount(value) != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    private static string VarName(Term term) => ((VarTerm)term).Name;

    private static long ConstIntValue(Term term) => ((ConstValue.Int)((ConstTerm)term).Value).Value;

    private static string ConstStringValue(Term term) => ((ConstValue.Str)((ConstTerm)term).Value).Value;

    private static string LambdaParamName(string paramName, long body) =>
        ((LambdaTerm)Terms.Lambda(paramName, Sort.Int, Terms.Num(body))).ParamName;

    private static int LetBindingCount(long value) =>
        ((LetTerm)Terms.Let(new[] { Terms.Binding("x", Terms.Num(value)) }, Terms.Num(value))).Bindings.Count;
}
