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

    public static V SortToValue(Sort s) => s switch
    {
        // Locked key order: kind, name.
        Sort.Primitive p => V.Object(
            ("kind", V.String("primitive")),
            ("name", V.String(p.Name))
        ),
        // Locked key order: kind, args, return.
        Sort.Function f => V.Object(
            ("kind", V.String("function")),
            ("args", V.Array(f.Args.Select(SortToValue).ToArray())),
            ("return", SortToValue(f.Return))
        ),
        // Locked key order: kind, name, indexVar, indexSort.
        Sort.Dependent d => V.Object(
            ("kind", V.String("dependent")),
            ("name", V.String(d.Name)),
            ("indexVar", V.String(d.IndexVar)),
            ("indexSort", SortToValue(d.IndexSort))
        ),
        _ => throw new ArgumentException($"unknown Sort variant: {s.GetType().Name}")
    };

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
        LambdaTerm l => V.Object(
            ("kind", V.String("lambda")),
            ("paramName", V.String(l.ParamName)),
            ("paramSort", SortToValue(l.ParamSort)),
            ("body", TermToValue(l.Body))
        ),
        LetTerm l => V.Object(
            ("kind", V.String("let")),
            ("bindings", V.Array(l.Bindings.Select(b => V.Object(
                ("name", V.String(b.Name)),
                ("boundTerm", TermToValue(b.BoundTerm))
            )).ToArray())),
            ("body", TermToValue(l.Body))
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
        ChoiceFormula c => V.Object(
            ("kind", V.String("choice")),
            ("varName", V.String(c.VarName)),
            ("sort", SortToValue(c.Sort)),
            ("body", FormulaToValue(c.Body))
        ),
        _ => throw new InvalidOperationException($"unknown formula kind: {f.GetType()}"),
    };

    // ----- v1.1.0 BridgeDeclaration → canonicalizer Value ---------------
    //
    // Spec wire form (protocol/specs/2026-04-30-ir-formal-grammar.md
    // §BridgeDeclaration; pinned by `bridge_decl` in
    // conformance/fixtures.toml):
    //
    //   { "kind": "bridge", "name", "sourceSymbol", "sourceLayer",
    //     "sourceContractCid", "targetContractCid", "targetProofCid",
    //     "targetLayer", "notes"? }
    //
    // Insertion order here is irrelevant — `Jcs.Encode` re-sorts keys to
    // Unicode code-point order at emit time (same discipline as
    // `FormulaToValue`). `Notes` is omitted when null so JCS doesn't
    // emit `"notes":null`; this matches the Rust peer's
    // `skip_serializing_if = "Option::is_none"`.
    public static V BridgeDeclarationToValue(BridgeDeclaration b)
    {
        var entries = new List<KeyValuePair<string, V>>(9)
        {
            new("kind", V.String("bridge")),
            new("name", V.String(b.Name)),
            new("sourceSymbol", V.String(b.SourceSymbol)),
            new("sourceLayer", V.String(b.SourceLayer)),
            new("sourceContractCid", V.String(b.SourceContractCid)),
            new("targetContractCid", V.String(b.TargetContractCid)),
            new("targetProofCid", V.String(b.TargetProofCid)),
            new("targetLayer", V.String(b.TargetLayer)),
        };
        if (b.Notes is not null)
        {
            entries.Add(new("notes", V.String(b.Notes)));
        }
        return V.Object(entries);
    }

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
        switch (s)
        {
            case Sort.Primitive p:
                // Locked key order: kind, name.
                sb.Append("{\"kind\":\"primitive\",\"name\":");
                WriteString(sb, p.Name);
                sb.Append('}');
                break;
            case Sort.Function f:
                // Locked key order: kind, args, return.
                sb.Append("{\"kind\":\"function\",\"args\":[");
                for (int i = 0; i < f.Args.Length; i++)
                {
                    if (i > 0) sb.Append(',');
                    WriteSort(sb, f.Args[i]);
                }
                sb.Append("],\"return\":");
                WriteSort(sb, f.Return);
                sb.Append('}');
                break;
            case Sort.Dependent d:
                // Locked key order: kind, name, indexVar, indexSort.
                sb.Append("{\"kind\":\"dependent\",\"name\":");
                WriteString(sb, d.Name);
                sb.Append(",\"indexVar\":");
                WriteString(sb, d.IndexVar);
                sb.Append(",\"indexSort\":");
                WriteSort(sb, d.IndexSort);
                sb.Append('}');
                break;
            default:
                throw new ArgumentException($"unknown Sort variant: {s.GetType().Name}");
        }
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
            case LambdaTerm l:
                sb.Append("{\"kind\":\"lambda\",\"paramName\":");
                WriteString(sb, l.ParamName);
                sb.Append(",\"paramSort\":");
                WriteSort(sb, l.ParamSort);
                sb.Append(",\"body\":");
                WriteTerm(sb, l.Body);
                sb.Append('}');
                return;
            case LetTerm l:
                sb.Append("{\"kind\":\"let\",\"bindings\":[");
                for (var i = 0; i < l.Bindings.Count; i++)
                {
                    if (i > 0) sb.Append(',');
                    sb.Append("{\"name\":");
                    WriteString(sb, l.Bindings[i].Name);
                    sb.Append(",\"boundTerm\":");
                    WriteTerm(sb, l.Bindings[i].BoundTerm);
                    sb.Append('}');
                }
                sb.Append("],\"body\":");
                WriteTerm(sb, l.Body);
                sb.Append('}');
                return;
        }
        throw new InvalidOperationException($"unknown term: {t.GetType()}");
    }

    // ----- call-edge JSON serialization ------------------------------------
    //
    // Wire form per bridge-linkage-protocol.md §1. Keys in insertion order;
    // the downstream JCS canonicalizer re-sorts. Null targetContractCid is
    // emitted as JSON null (not omitted) so the linker can distinguish
    // "not yet resolved" from "absent field".

    public static string MarshalCallEdges(IReadOnlyList<CallEdgeDeclaration> edges)
    {
        var sb = new System.Text.StringBuilder();
        sb.Append('[');
        for (var i = 0; i < edges.Count; i++)
        {
            if (i > 0) sb.Append(',');
            var e = edges[i];
            sb.Append("{\"schemaVersion\":\"1\",\"kind\":\"call-edge\"");
            sb.Append(",\"sourceContractCid\":");
            WriteString(sb, e.SourceContractCid);
            sb.Append(",\"targetContractCid\":");
            if (e.TargetContractCid is null)
                sb.Append("null");
            else
                WriteString(sb, e.TargetContractCid);
            sb.Append(",\"callSiteLocus\":{\"file\":");
            WriteString(sb, e.CallSiteLocus.File);
            sb.Append(",\"line\":");
            sb.Append(e.CallSiteLocus.Line);
            sb.Append(",\"column\":");
            sb.Append(e.CallSiteLocus.Column);
            sb.Append('}');
            sb.Append(",\"targetSymbol\":");
            WriteString(sb, e.TargetSymbol);
            sb.Append(",\"evidenceTerm\":");
            WriteString(sb, e.EvidenceTerm);
            sb.Append('}');
        }
        sb.Append(']');
        return sb.ToString();
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
            case ChoiceFormula c:
                sb.Append("{\"kind\":\"choice\",\"varName\":");
                WriteString(sb, c.VarName);
                sb.Append(",\"sort\":");
                WriteSort(sb, c.Sort);
                sb.Append(",\"body\":");
                WriteFormula(sb, c.Body);
                sb.Append('}');
                return;
        }
        throw new InvalidOperationException($"unknown formula: {f.GetType()}");
    }
}
