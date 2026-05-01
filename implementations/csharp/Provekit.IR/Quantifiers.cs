// SPDX-License-Identifier: Apache-2.0
//
// Quantifier constructors with auto-named bound variables. Each call
// to ForAll/Exists pulls the next index from a thread-local counter
// (`_x0`, `_x1`, ...). Reset between contracts with
// <see cref="Collector.Reset"/>. Mirrors the Rust/C++ peers.

using System.Threading;

namespace Provekit.IR;

public static class Quantifiers
{
    private static int _counter;

    /// <summary>Reset the bound-variable name counter to <c>_x0</c>.</summary>
    public static void ResetCounter() => Interlocked.Exchange(ref _counter, 0);

    private static string FreshVarName()
    {
        var n = Interlocked.Increment(ref _counter) - 1;
        return $"_x{n}";
    }

    /// <summary>
    /// Build <c>forall x:sort. body(x)</c>. The bound variable name is
    /// auto-generated (<c>_xN</c>); the lambda receives a VarTerm
    /// reference to it.
    /// </summary>
    public static Formula ForAll(Sort sort, Func<Term, Formula> body)
    {
        var vname = FreshVarName();
        var v = Terms.Var(vname);
        var inner = body(v);
        return new QuantifierFormula("forall", vname, sort, inner);
    }

    /// <summary>
    /// Build <c>exists x:sort. body(x)</c>.
    /// </summary>
    public static Formula Exists(Sort sort, Func<Term, Formula> body)
    {
        var vname = FreshVarName();
        var v = Terms.Var(vname);
        var inner = body(v);
        return new QuantifierFormula("exists", vname, sort, inner);
    }

    /// <summary>
    /// Build <c>εx:sort. body(x)</c> — definite description (unique existence).
    /// </summary>
    public static Formula Choice(string varName, Sort sort, Func<Term, Formula> body)
    {
        var v = Terms.Var(varName);
        var inner = body(v);
        return new ChoiceFormula(varName, sort, inner);
    }
}
