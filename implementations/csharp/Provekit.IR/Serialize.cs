// SPDX-License-Identifier: Apache-2.0
//
// IR-JSON serializer + canonicalizer Value bridge. Locked key orders
// per protocol/specs/2026-04-30-ir-formal-grammar.md (catalog v1.1.0):
//
//   var:        {kind, name}                       (no sort)
//   const:      {kind, value, sort}
//   ctor:       {kind, name, args}                 (no sort)
//   atomic:     {kind, name, args}
//   connective: {kind, operands}
//   quantifier: {kind, name, sort, body}
//   sort:       {kind: "primitive", name}
//   contract:   {kind: "contract", name, outBinding, pre?, post?, inv?}
//
// `ToValue` produces a Provekit.Canonicalizer.Value tree (used by the
// hashing path; the canonicalizer's JCS encoder re-sorts at emit time).
// `MarshalDeclarations` produces insertion-order JSON (mirrors the C++
// kit's behavior — the mint adapter receives that JSON, parses it,
// and re-canonicalizes through JCS for hashing).

using Provekit.Canonicalizer;
using V = Provekit.Canonicalizer.Value;

namespace Provekit.IR;

public static class Serialize
{
    // ----- to canonicalizer Value (used by the hashing flow) ------------

    public static V SortToValue(Sort s) => V.Object(
        ("kind", V.String("primitive")),
        ("name", V.String(s.Name))
    );

    public static V TermToValue(Term t) => t switch
    {
        VarTerm v => V.Object(
            ("kind", V.String("var")),
            ("name", V.String(v.Name))
        ),
        ConstTerm c => V.Object(
            ("kind", V.String("const")),
            ("value", ConstValueToValue(c.Value)),
            ("sort", SortToValue(c.Sort))
        ),
        CtorTerm c => V.Object(
            ("kind", V.String("ctor")),
            ("name", V.String(c.Name)),
            ("args", V.Array(c.Args.Select(TermToValue).ToArray()))
        ),
        _ => throw new InvalidOperationException($"unknown term kind: {t.GetType()}"),
    };

    private static V ConstValueToValue(ConstValue cv) => cv switch
    {
        ConstValue.Int i => V.Integer(i.Value),
        ConstValue.Str s => V.String(s.Value),
        ConstValue.Bool b => V.Boolean(b.Value),
        _ => throw new InvalidOperationException($"unknown const value: {cv.GetType()}"),
    };

    public static V FormulaToValue(Formula f) => f switch
    {
        AtomicFormula a => V.Object(
            ("kind", V.String("atomic")),
            ("name", V.String(a.Name)),
            ("args", V.Array(a.Args.Select(TermToValue).ToArray()))
        ),
        ConnectiveFormula c => V.Object(
            ("kind", V.String(c.Kind)),
            ("operands", V.Array(c.Operands.Select(FormulaToValue).ToArray()))
        ),
        QuantifierFormula q => V.Object(
            ("kind", V.String(q.Kind)),
            ("name", V.String(q.Name)),
            ("sort", SortToValue(q.Sort)),
            ("body", FormulaToValue(q.Body))
        ),
        _ => throw new InvalidOperationException($"unknown formula kind: {f.GetType()}"),
    };

    // ----- to insertion-order JSON (mirrors C++ marshal_declarations) ---

    public static string MarshalDeclarations(IReadOnlyList<ContractDecl> decls)
    {
        var sb = new System.Text.StringBuilder();
        sb.Append('[');
        for (var i = 0; i < decls.Count; i++)
        {
            if (i > 0) sb.Append(',');
            var d = decls[i];
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
        sb.Append(']');
        return sb.ToString();
    }

    private static void WriteString(System.Text.StringBuilder sb, string s)
    {
        // Same JCS-compatible escape rules as the canonicalizer (\u00XX
        // lowercase for control chars, verbatim non-ASCII).
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

    private static void WriteSort(System.Text.StringBuilder sb, Sort s)
    {
        sb.Append("{\"kind\":\"primitive\",\"name\":");
        WriteString(sb, s.Name);
        sb.Append('}');
    }

    private static void WriteTerm(System.Text.StringBuilder sb, Term t)
    {
        switch (t)
        {
            case VarTerm v:
                sb.Append("{\"kind\":\"var\",\"name\":");
                WriteString(sb, v.Name);
                sb.Append('}');
                return;
            case ConstTerm c:
                sb.Append("{\"kind\":\"const\",\"value\":");
                switch (c.Value)
                {
                    case ConstValue.Int i: sb.Append(i.Value); break;
                    case ConstValue.Bool b: sb.Append(b.Value ? "true" : "false"); break;
                    case ConstValue.Str s: WriteString(sb, s.Value); break;
                }
                sb.Append(",\"sort\":");
                WriteSort(sb, c.Sort);
                sb.Append('}');
                return;
            case CtorTerm c:
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

    private static void WriteFormula(System.Text.StringBuilder sb, Formula f)
    {
        switch (f)
        {
            case AtomicFormula a:
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
            case ConnectiveFormula c:
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
            case QuantifierFormula q:
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
}
