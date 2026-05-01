// SPDX-License-Identifier: Apache-2.0
//
// LinqLifter: parse C# source, walk every InvocationExpression, ask
// Roslyn's semantic model whether the call resolves to System.Linq, and
// dispatch to MethodTranslators. Recognised LINQ ops mint a contract
// memento; everything else is silently ignored.
//
// The lifter handles BOTH method-syntax (`xs.Where(p)`) AND query-syntax
// (`from x in xs where P(x) select x`). Roslyn's `IInvocationOperation`
// view normalises the latter into the same shape as the former, which is
// why we walk the OPERATION tree (not the syntax tree).
//
// Out of scope for v1 (documented as TODOs):
//   - Joins (Join, GroupJoin)
//   - async LINQ (System.Linq.AsyncEnumerable)
//   - Pre-resolved IQueryable expression trees from EF Core
//   - Multi-source SelectMany

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Operations;

namespace Provekit.Lift.Linq;

public sealed class LinqLifter
{
    private readonly List<MetadataReference> _references;
    private int _boundVarCounter;

    public LinqLifter()
    {
        _references = BuildReferences();
    }

    public IReadOnlyList<MintedMemento> Lift(string sourceCode, string fileName = "Source.cs")
    {
        var tree = CSharpSyntaxTree.ParseText(sourceCode, path: fileName);
        var compilation = CSharpCompilation.Create(
            "LiftCompilation",
            new[] { tree },
            _references,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
        var model = compilation.GetSemanticModel(tree);

        var results = new List<MintedMemento>();
        var root = tree.GetRoot();

        // We want to lift ROOT-level LINQ statements first, then chained
        // sub-expressions. Walking VariableDeclaratorSyntax + plain
        // ExpressionStatement nodes gives us the LHS binding name for
        // each top-level LINQ expression. For chained sub-expressions
        // (e.g. `xs.Where(...).Where(...)`), we recurse into the call
        // chain bottom-up so each inner call mints a memento referenced
        // by the outer one.

        foreach (var decl in root.DescendantNodes().OfType<VariableDeclaratorSyntax>())
        {
            var initVal = decl.Initializer?.Value;
            if (initVal is InvocationExpressionSyntax inv)
            {
                LiftExpressionRecursive(inv, decl.Identifier.Text, model, results);
            }
            else if (initVal is QueryExpressionSyntax q)
            {
                LiftQueryExpression(q, decl.Identifier.Text, model, results);
            }
        }

        // Standalone expression statements like `xs.All(p => p > 0);`.
        foreach (var stmt in root.DescendantNodes().OfType<ExpressionStatementSyntax>())
        {
            if (stmt.Expression is InvocationExpressionSyntax inv)
            {
                LiftExpressionRecursive(inv, lhsName: null, model, results);
            }
        }

        return results;
    }

    // Query syntax (`from x in xs where P(x) select x`) lowers to a
    // chain of IInvocationOperation calls under an
    // ITranslatedQueryOperation. We unwrap that wrapper and walk the
    // operation chain bottom-up, mirroring the method-syntax path. The
    // result is byte-identical to what method-syntax for the same
    // semantics would have produced.
    private void LiftQueryExpression(
        QueryExpressionSyntax q,
        string lhsName,
        SemanticModel model,
        List<MintedMemento> results)
    {
        var op = model.GetOperation(q);
        if (op is null) return;
        // Unwrap the synthetic translated-query wrapper.
        IOperation? cur = op;
        while (cur is ITranslatedQueryOperation tq) cur = tq.Operation;
        // Walk the nested IInvocationOperation chain.
        LiftOperationChain(cur, lhsName, model, results);
    }

    // Bottom-up walker over the IInvocationOperation tree, identical in
    // shape to LiftExpressionRecursive but operating directly on
    // operations (the syntactic-sugar form has no InvocationExpression
    // syntax for the inner Where/Select).
    private string? LiftOperationChain(
        IOperation? op,
        string? lhsName,
        SemanticModel model,
        List<MintedMemento> results)
    {
        if (op is not IInvocationOperation inv) return null;
        var method = inv.TargetMethod;
        if (method.ContainingType is null) return null;
        var ns = method.ContainingType.ContainingNamespace?.ToDisplayString();
        if (ns != "System.Linq") return null;

        // Receiver in extension method form is Arguments[0].
        string? innerOutBinding = null;
        if (inv.Arguments.Length > 0)
        {
            var src = inv.Arguments[0].Value;
            while (src is IConversionOperation conv) src = conv.Operand;
            if (src is IInvocationOperation innerInv)
            {
                innerOutBinding = LiftOperationChain(innerInv, lhsName: null, model, results);
            }
        }

        var resolvedLhs = lhsName ?? (innerOutBinding is not null
            ? SyntheticBinding(innerOutBinding)
            : null);

        var ctx = new LiftContext(
            BoundVar: $"_x{_boundVarCounter++}",
            LhsName: resolvedLhs,
            MementoName: null);

        var memento = MethodTranslators.TryTranslate(inv, model, ctx);
        if (memento is null) return null;

        if (innerOutBinding is not null)
        {
            var patched = ReplaceVarName(memento.Contract, "_recv", innerOutBinding);
            memento = memento with
            {
                Contract = patched,
                IrJson = IREmit.Contract(patched),
                InputBindings = new[] { innerOutBinding },
            };
        }

        results.Add(memento);
        return memento.OutBinding;
    }

    // Bottom-up: lift the inner-most invocation first (so its OutBinding
    // is the original receiver name), then climb outward, with each
    // outer call inheriting the LHS name only at the top.
    private string? LiftExpressionRecursive(
        InvocationExpressionSyntax inv,
        string? lhsName,
        SemanticModel model,
        List<MintedMemento> results)
    {
        // First, recurse into the receiver if it is itself a chained LINQ call.
        string? innerOutBinding = null;
        if (inv.Expression is MemberAccessExpressionSyntax ma
            && ma.Expression is InvocationExpressionSyntax innerInv)
        {
            // The inner call needs an intermediate name. We mint one if
            // the user didn't write `var foo = ...`. The intermediate
            // name is what the outer call's `inputBindings` references.
            innerOutBinding = LiftExpressionRecursive(
                innerInv,
                lhsName: null,  // forces synthetic name
                model,
                results);
        }

        var op = model.GetOperation(inv) as IInvocationOperation;
        if (op is null) return null;

        // Resolve the LHS name. Top-level: use lhsName from
        // VariableDeclarator. Intermediate: synthesise a name from the
        // inner call's binding so the chain-DAG `inputBindings` edge is
        // unambiguous.
        var resolvedLhs = lhsName ?? (innerOutBinding is not null
            ? SyntheticBinding(innerOutBinding)
            : null);

        var ctx = new LiftContext(
            BoundVar: $"_x{_boundVarCounter++}",
            LhsName: resolvedLhs,
            MementoName: null);

        var memento = MethodTranslators.TryTranslate(op, model, ctx);
        if (memento is null) return null;

        // Patch the receiver if this call consumed an inner chained
        // call (its receiver syntax is an InvocationExpression, not an
        // identifier, so MethodTranslators emitted the placeholder
        // "_recv").
        if (innerOutBinding is not null)
        {
            var patchedContract = ReplaceVarName(memento.Contract, "_recv", innerOutBinding);
            memento = memento with
            {
                Contract = patchedContract,
                IrJson = IREmit.Contract(patchedContract),
                InputBindings = new[] { innerOutBinding },
            };
        }

        results.Add(memento);
        return memento.OutBinding;
    }

    private string SyntheticBinding(string baseName) => $"{baseName}_step{_boundVarCounter}";

    private static ContractDecl ReplaceVarName(ContractDecl c, string from, string to) =>
        c with
        {
            Pre = c.Pre is null ? null : ReplaceInFormula(c.Pre, from, to),
            Post = c.Post is null ? null : ReplaceInFormula(c.Post, from, to),
            Inv = c.Inv is null ? null : ReplaceInFormula(c.Inv, from, to),
        };

    private static Formula ReplaceInFormula(Formula f, string from, string to) => f switch
    {
        Formula.Atomic a => new Formula.Atomic(a.Name, a.Args.Select(t => ReplaceInTerm(t, from, to)).ToList()),
        Formula.Connective c => new Formula.Connective(c.Kind, c.Operands.Select(o => ReplaceInFormula(o, from, to)).ToList()),
        Formula.Quantifier q => new Formula.Quantifier(q.Kind, q.Name, q.Sort, ReplaceInFormula(q.Body, from, to)),
        _ => f,
    };

    private static Term ReplaceInTerm(Term t, string from, string to) => t switch
    {
        Term.Var v when v.Name == from => new Term.Var(to),
        Term.Ctor c => new Term.Ctor(c.Name, c.Args.Select(a => ReplaceInTerm(a, from, to)).ToList()),
        _ => t,
    };

    // Build the metadata-reference set Roslyn needs for SemanticModel
    // resolution. Without these, calls like `xs.Where(...)` resolve to
    // a null symbol and the lifter sees nothing.
    private static List<MetadataReference> BuildReferences()
    {
        var refs = new List<MetadataReference>();
        var tpa = (string?)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES") ?? "";
        foreach (var path in tpa.Split(Path.PathSeparator))
        {
            if (string.IsNullOrEmpty(path)) continue;
            try { refs.Add(MetadataReference.CreateFromFile(path)); }
            catch { /* some entries may be unreadable on certain runtimes */ }
        }
        // Belt and suspenders: ensure System.Linq is present.
        refs.Add(MetadataReference.CreateFromFile(typeof(System.Linq.Enumerable).Assembly.Location));
        return refs;
    }
}
