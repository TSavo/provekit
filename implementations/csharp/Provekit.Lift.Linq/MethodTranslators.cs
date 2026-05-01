// SPDX-License-Identifier: Apache-2.0
//
// One translator per recognised LINQ op. Each consumes:
//   - the call-site `IInvocationOperation` (so we can ask the semantic
//     model for the receiver, the lambda parameter, the call's variable
//     binding name on the LHS),
//   - a `LiftContext` carrying the bound variable name (`_x0`, `_x1`,
//     ...) and the receiver's element-sort assumption,
// and produces a `MintedMemento` with:
//   - `OutBinding`: the LHS variable name (or `"out"` for scalar ops),
//   - `InputBindings`: the simple identifier of the receiver, when it
//     is itself a previously-lifted variable; the mint pipeline turns
//     these into CID edges (chain DAG).
//
// Translation rules (v1; document deviations):
//
//   Where(P)        → forall _x:T. P(_x) ⇒ member(_x, result)
//                     [forward-only; the converse is the verifier's
//                      responsibility, by design]
//   Select(f)       → forall _x:T. member(f(_x), result)
//   All(P)          → forall _x:T. P(_x)            (implicit member)
//   Any(P)          → exists _x:T. P(_x)
//   Count(P)        → forall _x:T. P(_x) ⇒ counted(_x)   [TODO: cardinality]
//   First(P)        → exists _x:T. P(_x) ∧ _x = out
//   Single(P)       → exists _x:T. P(_x) ∧ _x = out      [TODO: uniqueness]
//   Sum()           → out = Σ_x:T over receiver         [encoded as ctor]
//   Sum(f)          → out = Σ_x:T f(_x) over receiver
//   GroupBy(k)      → forall _x:T. ∃g. k(_x) = g.Key ∧ member(_x, g) ∧ member(g, result)
//   OrderBy(k)      → forall _x:T. member(_x, receiver) ⇔ member(_x, result)
//                     [permutation; TODO: order-preservation predicate]

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Operations;

namespace Provekit.Lift.Linq;

internal static class MethodTranslators
{
    // Public entry: returns null if this invocation is NOT a recognised LINQ op.
    public static MintedMemento? TryTranslate(
        IInvocationOperation op,
        SemanticModel model,
        LiftContext ctx)
    {
        var method = op.TargetMethod;
        if (method.ContainingType is null) return null;
        var ns = method.ContainingType.ContainingNamespace?.ToDisplayString();
        if (ns != "System.Linq") return null;

        return method.Name switch
        {
            "Where" => TranslateWhere(op, model, ctx),
            "Select" => TranslateSelect(op, model, ctx),
            "All" => TranslateAll(op, model, ctx),
            "Any" => TranslateAny(op, model, ctx),
            "Count" => TranslateCount(op, model, ctx),
            "First" => TranslateFirst(op, model, ctx),
            "Single" => TranslateSingle(op, model, ctx),
            "Sum" => TranslateSum(op, model, ctx),
            "GroupBy" => TranslateGroupBy(op, model, ctx),
            "OrderBy" => TranslateOrderBy(op, model, ctx),
            _ => null,
        };
    }

    // --- helpers ------------------------------------------------------

    private static (string Name, string LambdaParam, ExpressionSyntax Body)? ExtractLambda(
        IInvocationOperation op,
        int argIndex,
        string boundVar)
    {
        if (argIndex >= op.Arguments.Length) return null;
        var arg = op.Arguments[argIndex];
        if (arg.Value is IDelegateCreationOperation { Target: IAnonymousFunctionOperation anon })
        {
            // Three layouts in the wild:
            //   (a) explicit method-syntax lambda: `x => x > 0`
            //       syntax = SimpleLambdaExpressionSyntax with expr body.
            //   (b) explicit method-syntax lambda with block body:
            //       `x => { return x > 0; }` -- still SimpleLambda, body is Block.
            //   (c) query-syntax synthetic lambda: no expression body in
            //       syntax; the operation is IAnonymousFunctionOperation
            //       wrapping IBlockOperation with one IReturnOperation.
            //
            // We resolve by walking the operation's Body for the first
            // IReturnOperation whose ReturnedValue.Syntax is an
            // ExpressionSyntax; that is the predicate / projection
            // expression, regardless of whether it came from (a), (b),
            // or (c).
            var paramName = anon.Symbol.Parameters.Length > 0
                ? anon.Symbol.Parameters[0].Name
                : "_arg";
            var bodyExpr = FirstReturnedExpression(anon.Body)
                ?? (anon.Symbol.DeclaringSyntaxReferences.Length > 0
                    ? anon.Symbol.DeclaringSyntaxReferences[0].GetSyntax() switch
                    {
                        SimpleLambdaExpressionSyntax slx when slx.Body is ExpressionSyntax e => e,
                        _ => null,
                    }
                    : null);
            if (bodyExpr is not null) return (boundVar, paramName, bodyExpr);
        }
        // Direct syntax match (Roslyn occasionally skips the delegate creation wrapper).
        if (arg.Syntax is SimpleLambdaExpressionSyntax sl2 && sl2.Body is ExpressionSyntax body2)
            return (boundVar, sl2.Parameter.Identifier.Text, body2);
        return null;
    }

    private static ExpressionSyntax? FirstReturnedExpression(IBlockOperation? block)
    {
        if (block is null) return null;
        foreach (var stmt in block.Operations)
        {
            if (stmt is IReturnOperation ret
                && ret.ReturnedValue?.Syntax is ExpressionSyntax e)
                return e;
            if (stmt is IBlockOperation inner)
            {
                var nested = FirstReturnedExpression(inner);
                if (nested is not null) return nested;
            }
        }
        return null;
    }

    private static string ReceiverBinding(IInvocationOperation op)
    {
        // System.Linq.Enumerable extension methods: receiver is the
        // first argument (the source).
        if (op.Arguments.Length == 0) return "_recv";
        var src = op.Arguments[0].Value;
        // Unwrap conversions.
        while (src is IConversionOperation conv) src = conv.Operand;
        if (src.Syntax is IdentifierNameSyntax id) return id.Identifier.Text;
        return "_recv";
    }

    // --- per-op translators -------------------------------------------

    private static MintedMemento? TranslateWhere(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var lambda = ExtractLambda(op, 1, ctx.BoundVar);
        if (lambda is null) return null;
        var (boundVar, paramName, body) = lambda.Value;
        var pred = new PredicateTranslator(model, boundVar, paramName).TranslateBody(body);
        var outName = ctx.LhsName ?? "result";
        var receiver = ReceiverBinding(op);
        var formula = IR.ForAll(boundVar, IR.Ref(),
            IR.Implies(pred, IR.Atom("member", IR.Var(boundVar), IR.Var(outName))));
        var contract = new ContractDecl($"{outName}_where", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateSelect(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var lambda = ExtractLambda(op, 1, ctx.BoundVar);
        if (lambda is null) return null;
        var (boundVar, paramName, body) = lambda.Value;
        var projTerm = new PredicateTranslator(model, boundVar, paramName).TranslateProjection(body);
        var outName = ctx.LhsName ?? "result";
        var receiver = ReceiverBinding(op);
        var formula = IR.ForAll(boundVar, IR.Ref(),
            IR.Atom("member", projTerm, IR.Var(outName)));
        var contract = new ContractDecl($"{outName}_select", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateAll(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var lambda = ExtractLambda(op, 1, ctx.BoundVar);
        if (lambda is null) return null;
        var (boundVar, paramName, body) = lambda.Value;
        var pred = new PredicateTranslator(model, boundVar, paramName).TranslateBody(body);
        var outName = ctx.LhsName ?? "out";
        var receiver = ReceiverBinding(op);
        var formula = IR.ForAll(boundVar, IR.Int(), pred);
        var contract = new ContractDecl(ctx.MementoName ?? $"{receiver}_all", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateAny(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var lambda = ExtractLambda(op, 1, ctx.BoundVar);
        if (lambda is null) return null;
        var (boundVar, paramName, body) = lambda.Value;
        var pred = new PredicateTranslator(model, boundVar, paramName).TranslateBody(body);
        var outName = ctx.LhsName ?? "out";
        var receiver = ReceiverBinding(op);
        var formula = IR.Exists(boundVar, IR.Int(), pred);
        var contract = new ContractDecl(ctx.MementoName ?? $"{receiver}_any", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateCount(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var outName = ctx.LhsName ?? "out";
        var receiver = ReceiverBinding(op);
        if (op.Arguments.Length > 1
            && ExtractLambda(op, 1, ctx.BoundVar) is { } lambda)
        {
            var (boundVar, paramName, body) = lambda;
            var pred = new PredicateTranslator(model, boundVar, paramName).TranslateBody(body);
            // Approximate cardinality predicate; the verifier's solver
            // tactics specialise this when the receiver's universe is
            // known. TODO: dedicated `card-eq` form.
            var formula = IR.ForAll(boundVar, IR.Ref(),
                IR.Implies(pred, IR.Atom("counted", IR.Var(boundVar), IR.Var(outName))));
            var contract = new ContractDecl($"{receiver}_count", outName, formula, null, null);
            return MakeMemento(contract, outName, new[] { receiver }, op);
        }
        else
        {
            var formula = IR.Atom("=", IR.Var(outName),
                IR.Ctor("Count", IR.Var(receiver)));
            var contract = new ContractDecl($"{receiver}_count", outName, formula, null, null);
            return MakeMemento(contract, outName, new[] { receiver }, op);
        }
    }

    private static MintedMemento? TranslateFirst(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var outName = ctx.LhsName ?? "out";
        var receiver = ReceiverBinding(op);
        var lambda = op.Arguments.Length > 1 ? ExtractLambda(op, 1, ctx.BoundVar) : null;
        Formula formula;
        if (lambda is { } lam)
        {
            var (boundVar, paramName, body) = lam;
            var pred = new PredicateTranslator(model, boundVar, paramName).TranslateBody(body);
            formula = IR.Exists(boundVar, IR.Ref(),
                IR.And(pred, IR.Atom("=", IR.Var(boundVar), IR.Var(outName))));
        }
        else
        {
            formula = IR.Atom("=", IR.Var(outName), IR.Ctor("First", IR.Var(receiver)));
        }
        var contract = new ContractDecl($"{receiver}_first", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateSingle(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        // Same shape as First for v1; the uniqueness clause is a TODO.
        return TranslateFirst(op, model, ctx);
    }

    private static MintedMemento? TranslateSum(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var outName = ctx.LhsName ?? "out";
        var receiver = ReceiverBinding(op);
        var lambda = op.Arguments.Length > 1 ? ExtractLambda(op, 1, ctx.BoundVar) : null;
        Formula formula;
        if (lambda is { } lam)
        {
            var (boundVar, paramName, body) = lam;
            var proj = new PredicateTranslator(model, boundVar, paramName).TranslateProjection(body);
            // out = Sum_over(receiver, λ_x. proj)
            formula = IR.Atom("=", IR.Var(outName),
                IR.Ctor("Sum", IR.Var(receiver),
                    IR.Ctor("λ", IR.Var(boundVar), proj)));
        }
        else
        {
            formula = IR.Atom("=", IR.Var(outName), IR.Ctor("Sum", IR.Var(receiver)));
        }
        var contract = new ContractDecl($"{receiver}_sum", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateGroupBy(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var lambda = ExtractLambda(op, 1, ctx.BoundVar);
        if (lambda is null) return null;
        var (boundVar, paramName, body) = lambda.Value;
        var keyTerm = new PredicateTranslator(model, boundVar, paramName).TranslateProjection(body);
        var outName = ctx.LhsName ?? "result";
        var receiver = ReceiverBinding(op);
        // forall _x:T. exists g. (Key(g) = k(_x)) ∧ member(_x, g) ∧ member(g, out)
        var formula = IR.ForAll(boundVar, IR.Ref(),
            IR.Exists("g", IR.Ref(),
                IR.And(
                    IR.Atom("=", IR.Ctor("Key", IR.Var("g")), keyTerm),
                    IR.Atom("member", IR.Var(boundVar), IR.Var("g")),
                    IR.Atom("member", IR.Var("g"), IR.Var(outName)))));
        var contract = new ContractDecl($"{outName}_groupby", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento? TranslateOrderBy(
        IInvocationOperation op, SemanticModel model, LiftContext ctx)
    {
        var lambda = ExtractLambda(op, 1, ctx.BoundVar);
        if (lambda is null) return null;
        var (boundVar, paramName, body) = lambda.Value;
        var outName = ctx.LhsName ?? "result";
        var receiver = ReceiverBinding(op);
        // Permutation: same set of elements. TODO: emit a sortedness
        // companion predicate over (k(_x), k(_y)) pairs.
        // v1.1.0 has no iff; encode bidirectional membership as a
        // conjunction of two implications.
        var membRecv = IR.Atom("member", IR.Var(boundVar), IR.Var(receiver));
        var membOut = IR.Atom("member", IR.Var(boundVar), IR.Var(outName));
        var formula = IR.ForAll(boundVar, IR.Ref(),
            IR.And(IR.Implies(membRecv, membOut), IR.Implies(membOut, membRecv)));
        var contract = new ContractDecl($"{outName}_orderby", outName, formula, null, null);
        return MakeMemento(contract, outName, new[] { receiver }, op);
    }

    private static MintedMemento MakeMemento(
        ContractDecl contract,
        string outBinding,
        IEnumerable<string> inputBindings,
        IInvocationOperation op)
    {
        var json = IREmit.Contract(contract);
        var span = op.Syntax.GetLocation().GetLineSpan().ToString();
        return new MintedMemento(
            contract.Name,
            outBinding,
            inputBindings.ToList(),
            contract,
            json,
            span);
    }
}

internal sealed record LiftContext(string BoundVar, string? LhsName, string? MementoName);
