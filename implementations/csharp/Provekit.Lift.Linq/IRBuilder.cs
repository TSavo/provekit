// SPDX-License-Identifier: Apache-2.0
//
// IR-JSON v1.1.0 builders + emitter. The lifter is a SEPARATE producer of
// canonical IR; we deliberately do not import Provekit.IR. The byte
// sequence emitted here MUST match the kit's MarshalDeclarations output
// for the same logical formula -- that is the cross-kit conformance
// guarantee. The conformance test in Provekit.Lift.Linq.Tests references
// Provekit.IR and verifies byte-equality on a pinned shape.
//
// Locked key orders per protocol/specs/2026-04-30-ir-formal-grammar.md
// (catalog v1.1.0):
//
//   var:        {kind, name}                       (no sort)
//   const:      {kind, value, sort}
//   ctor:       {kind, name, args}                 (no sort)
//   atomic:     {kind, name, args}
//   connective: {kind, operands}
//   quantifier: {kind, name, sort, body}
//   sort:       {kind: "primitive", name}
//   contract:   {kind: "contract", name, outBinding, pre?, post?, inv?}

using System.Text;

namespace Provekit.Lift.Linq;

// ----- IR data types --------------------------------------------------

public abstract record Sort
{
    public sealed record Primitive(string Name) : Sort;
    public sealed record Function(Sort[] Args, Sort Return) : Sort;
    public sealed record Dependent(string Name, string IndexVar, Sort IndexSort) : Sort;
    public sealed record RegionSort(string Name) : Sort;
}

public abstract record Term
{
    public sealed record Var(string Name) : Term;
    public sealed record Const(object Value, Sort Sort) : Term;
    public sealed record Ctor(string Name, IReadOnlyList<Term> Args) : Term;
}

public abstract record Formula
{
    public sealed record Atomic(string Name, IReadOnlyList<Term> Args) : Formula;
    // Kind in {"and", "or", "not", "implies"}.
    public sealed record Connective(string Kind, IReadOnlyList<Formula> Operands) : Formula;
    // Kind in {"forall", "exists"}.
    public sealed record Quantifier(string Kind, string Name, Sort Sort, Formula Body) : Formula;
}

public sealed record ContractDecl(
    string Name,
    string OutBinding,
    Formula? Pre,
    Formula? Post,
    Formula? Inv);

// ----- helpers --------------------------------------------------------

public static class IR
{
    public static Sort Int() => new Sort.Primitive("Int");
    public static Sort Bool() => new Sort.Primitive("Bool");
    public static Sort StringSort() => new Sort.Primitive("String");
    public static Sort Real() => new Sort.Primitive("Real");
    public static Sort Ref() => new Sort.Primitive("Ref");

    public static Term Var(string n) => new Term.Var(n);
    public static Term Num(long v) => new Term.Const(v, Int());
    public static Term Str(string v) => new Term.Const(v, StringSort());
    public static Term BoolConst(bool v) => new Term.Const(v, Bool());
    public static Term Null() => new Term.Const(null!, Ref());
    public static Term Ctor(string name, params Term[] args) => new Term.Ctor(name, args);

    public static Formula Atom(string name, params Term[] args) => new Formula.Atomic(name, args);
    public static Formula And(params Formula[] ops) => new Formula.Connective("and", ops);
    public static Formula Or(params Formula[] ops) => new Formula.Connective("or", ops);
    public static Formula Not(Formula f) => new Formula.Connective("not", new[] { f });
    public static Formula Implies(Formula a, Formula c) => new Formula.Connective("implies", new[] { a, c });
    public static Formula ForAll(string name, Sort sort, Formula body) =>
        new Formula.Quantifier("forall", name, sort, body);
    public static Formula Exists(string name, Sort sort, Formula body) =>
        new Formula.Quantifier("exists", name, sort, body);
}

// ----- byte emitter (mirrors Provekit.IR.Serialize.MarshalDeclarations)

public static class IREmit
{
    public static string Declarations(IReadOnlyList<ContractDecl> decls)
    {
        var sb = new StringBuilder();
        sb.Append('[');
        for (var i = 0; i < decls.Count; i++)
        {
            if (i > 0) sb.Append(',');
            WriteContract(sb, decls[i]);
        }
        sb.Append(']');
        return sb.ToString();
    }

    public static string Contract(ContractDecl d)
    {
        var sb = new StringBuilder();
        WriteContract(sb, d);
        return sb.ToString();
    }

    private static void WriteContract(StringBuilder sb, ContractDecl d)
    {
        sb.Append("{\"kind\":\"contract\",\"name\":");
        WriteString(sb, d.Name);
        sb.Append(",\"outBinding\":");
        WriteString(sb, d.OutBinding);
        if (d.Pre is not null)
        {
            sb.Append(",\"pre\":");
            WriteFormula(sb, d.Pre);
        }
        if (d.Post is not null)
        {
            sb.Append(",\"post\":");
            WriteFormula(sb, d.Post);
        }
        if (d.Inv is not null)
        {
            sb.Append(",\"inv\":");
            WriteFormula(sb, d.Inv);
        }
        sb.Append('}');
    }

    private static void WriteFormula(StringBuilder sb, Formula f)
    {
        switch (f)
        {
            case Formula.Atomic a:
                sb.Append("{\"kind\":\"atomic\",\"name\":");
                WriteString(sb, a.Name);
                sb.Append(",\"args\":[");
                for (var i = 0; i < a.Args.Count; i++)
                {
                    if (i > 0) sb.Append(',');
                    WriteTerm(sb, a.Args[i]);
                }
                sb.Append("]}");
                return;
            case Formula.Connective c:
                sb.Append("{\"kind\":");
                WriteString(sb, c.Kind);
                sb.Append(",\"operands\":[");
                for (var i = 0; i < c.Operands.Count; i++)
                {
                    if (i > 0) sb.Append(',');
                    WriteFormula(sb, c.Operands[i]);
                }
                sb.Append("]}");
                return;
            case Formula.Quantifier q:
                sb.Append("{\"kind\":");
                WriteString(sb, q.Kind);
                sb.Append(",\"name\":");
                WriteString(sb, q.Name);
                sb.Append(",\"sort\":");
                WriteSort(sb, q.Sort);
                sb.Append(",\"body\":");
                WriteFormula(sb, q.Body);
                sb.Append('}');
                return;
        }
        throw new InvalidOperationException($"unknown formula: {f.GetType()}");
    }

    private static void WriteTerm(StringBuilder sb, Term t)
    {
        switch (t)
        {
            case Term.Var v:
                sb.Append("{\"kind\":\"var\",\"name\":");
                WriteString(sb, v.Name);
                sb.Append('}');
                return;
            case Term.Const c:
                sb.Append("{\"kind\":\"const\",\"value\":");
                switch (c.Value)
                {
                    case long l: sb.Append(l); break;
                    case int i: sb.Append(i); break;
                    case bool b: sb.Append(b ? "true" : "false"); break;
                    case string s: WriteString(sb, s); break;
                    case null: sb.Append("null"); break;
                    default:
                        throw new InvalidOperationException($"unsupported const value: {c.Value?.GetType()}");
                }
                sb.Append(",\"sort\":");
                WriteSort(sb, c.Sort);
                sb.Append('}');
                return;
            case Term.Ctor c:
                sb.Append("{\"kind\":\"ctor\",\"name\":");
                WriteString(sb, c.Name);
                sb.Append(",\"args\":[");
                for (var i = 0; i < c.Args.Count; i++)
                {
                    if (i > 0) sb.Append(',');
                    WriteTerm(sb, c.Args[i]);
                }
                sb.Append("]}");
                return;
        }
        throw new InvalidOperationException($"unknown term: {t.GetType()}");
    }

    private static void WriteSort(StringBuilder sb, Sort s)
    {
        switch (s)
        {
            case Sort.Primitive p:
                // Locked key order: kind, name.
                sb.Append("{\"kind\":\"primitive\",\"name\":");
                WriteString(sb, p.Name);
                sb.Append('}');
                return;
            case Sort.Function f:
                // Locked key order: kind, args, return: JCS-alphabetical.
                sb.Append("{\"args\":[");
                for (int i = 0; i < f.Args.Length; i++)
                {
                    if (i > 0) sb.Append(',');
                    WriteSort(sb, f.Args[i]);
                }
                sb.Append("],\"kind\":\"function\",\"return\":");
                WriteSort(sb, f.Return);
                sb.Append('}');
                return;
            case Sort.Dependent d:
                // Locked key order: kind, name, indexVar, indexSort: JCS-alphabetical.
                sb.Append("{\"indexSort\":");
                WriteSort(sb, d.IndexSort);
                sb.Append(",\"indexVar\":");
                WriteString(sb, d.IndexVar);
                sb.Append(",\"kind\":\"dependent\",\"name\":");
                WriteString(sb, d.Name);
                sb.Append('}');
                return;
            case Sort.RegionSort r:
                sb.Append("{\"kind\":\"region\",\"name\":");
                WriteString(sb, r.Name);
                sb.Append('}');
                return;
        }
        throw new InvalidOperationException($"unknown sort: {s.GetType()}");
    }

    private static void WriteString(StringBuilder sb, string s)
    {
        sb.Append('"');
        foreach (var c in s)
        {
            if (c == '"') sb.Append("\\\"");
            else if (c == '\\') sb.Append("\\\\");
            else if (c < 0x20)
            {
                sb.Append("\\u00");
                sb.Append(HexLower((c >> 4) & 0xF));
                sb.Append(HexLower(c & 0xF));
            }
            else sb.Append(c);
        }
        sb.Append('"');
    }

    private static char HexLower(int n) => (char)(n < 10 ? '0' + n : 'a' + (n - 10));
}
