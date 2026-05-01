// SPDX-License-Identifier: Apache-2.0
//
// IR-JSON v1.1.0 Term. Three kinds: VarTerm (no sort, named only),
// ConstTerm (carries sort + value), CtorTerm (no sort, named with args).
// Mirrors implementations/rust/provekit-ir-symbolic/src/lib.rs and
// implementations/cpp/provekit-ir-symbolic/include/provekit/ir.hpp.

namespace Provekit.IR;

public abstract record Term;

public sealed record VarTerm(string Name) : Term;

/// <summary>Const value variant — Int / String / Bool.</summary>
public abstract record ConstValue
{
    public sealed record Int(long Value) : ConstValue;
    public sealed record Str(string Value) : ConstValue;
    public sealed record Bool(bool Value) : ConstValue;
}

public sealed record ConstTerm(ConstValue Value, Sort Sort) : Term;

public sealed record CtorTerm(string Name, IReadOnlyList<Term> Args) : Term;

public sealed record LambdaTerm(string ParamName, Sort ParamSort, Term Body) : Term;

public sealed record LetBinding(string Name, Term BoundTerm);

public sealed record LetTerm(IReadOnlyList<LetBinding> Bindings, Term Body) : Term;

public static class Terms
{
    public static Term Var(string name) => new VarTerm(name);

    public static Term Num(long value) =>
        new ConstTerm(new ConstValue.Int(value), Sort.Int);

    public static Term StrConst(string value) =>
        new ConstTerm(new ConstValue.Str(value), Sort.String);

    public static Term BoolConst(bool value) =>
        new ConstTerm(new ConstValue.Bool(value), Sort.Bool);

    public static Term Ctor(string name, params Term[] args) =>
        new CtorTerm(name, args);

    public static Term Lambda(string paramName, Sort paramSort, Term body) =>
        new LambdaTerm(paramName, paramSort, body);

    public static LetBinding Binding(string name, Term boundTerm) =>
        new LetBinding(name, boundTerm);

    public static Term Let(IReadOnlyList<LetBinding> bindings, Term body) =>
        new LetTerm(bindings, body);

    /// <summary>
    /// References the return value within a post formula. Compiles to a
    /// VarTerm whose name matches the enclosing contract's outBinding
    /// (default "out"). Custom outBindings can use <see cref="Var"/>
    /// directly.
    /// </summary>
    public static Term Out() => new VarTerm("out");
}
