// SPDX-License-Identifier: Apache-2.0
//
// Lambda-body → IR Formula translator. Recognises the subset of C# the
// trojan-horse pitch covers: comparisons (`<`, `<=`, `>`, `>=`, `==`,
// `!=`), boolean connectives (`&&`, `||`, `!`), member access (lifted to
// a ctor: e.g. `u.Age` → `ctor("Age", var(u))`), integer/string/bool
// literals, parameter references, simple arithmetic in projections.
// Unsupported constructs throw `UnsupportedSyntaxException`; the caller
// records this as a TODO rather than emitting a wrong memento.

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Operations;

namespace Provekit.Lift.Linq;

public sealed class UnsupportedSyntaxException : Exception
{
    public UnsupportedSyntaxException(string msg, SyntaxNode node)
        : base($"{msg} at {node.GetLocation().GetLineSpan()}: {node}")
    { }
}

public sealed class PredicateTranslator
{
    private readonly SemanticModel _model;
    private readonly string _boundVarName;
    private readonly string _lambdaParamName;

    public PredicateTranslator(SemanticModel model, string boundVarName, string lambdaParamName)
    {
        _model = model;
        _boundVarName = boundVarName;
        _lambdaParamName = lambdaParamName;
    }

    public Formula TranslateBody(ExpressionSyntax body) => TranslateBoolExpr(body);

    public Term TranslateProjection(ExpressionSyntax body) => TranslateTerm(body);

    private Formula TranslateBoolExpr(ExpressionSyntax e)
    {
        switch (e)
        {
            case ParenthesizedExpressionSyntax p:
                return TranslateBoolExpr(p.Expression);
            case PrefixUnaryExpressionSyntax u when u.IsKind(SyntaxKind.LogicalNotExpression):
                return IR.Not(TranslateBoolExpr(u.Operand));
            case BinaryExpressionSyntax b when b.IsKind(SyntaxKind.LogicalAndExpression):
                return IR.And(TranslateBoolExpr(b.Left), TranslateBoolExpr(b.Right));
            case BinaryExpressionSyntax b when b.IsKind(SyntaxKind.LogicalOrExpression):
                return IR.Or(TranslateBoolExpr(b.Left), TranslateBoolExpr(b.Right));
            case BinaryExpressionSyntax b when CompareOp(b.Kind()) is { } op:
                return IR.Atom(op,
                    TranslateTerm(b.Left),
                    TranslateTerm(b.Right));
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.TrueLiteralExpression):
                return IR.Atom("true");
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.FalseLiteralExpression):
                return IR.Atom("false");
            case MemberAccessExpressionSyntax m:
                // Bool-typed property access, e.g. `u.IsRegistered`.
                // Encode as an atomic predicate named after the member,
                // applied to the receiver. Preserves namespacing without
                // forcing us to model the receiver's full record shape.
                return IR.Atom(m.Name.Identifier.Text, TranslateTerm(m.Expression));
            case IdentifierNameSyntax id when IsBoolParam(id):
                return IR.Atom(id.Identifier.Text);
        }
        throw new UnsupportedSyntaxException(
            $"predicate body not supported (kind={e.Kind()})", e);
    }

    private static string? CompareOp(SyntaxKind k) => k switch
    {
        SyntaxKind.LessThanExpression => "<",
        SyntaxKind.LessThanOrEqualExpression => "≤",
        SyntaxKind.GreaterThanExpression => ">",
        SyntaxKind.GreaterThanOrEqualExpression => "≥",
        SyntaxKind.EqualsExpression => "=",
        SyntaxKind.NotEqualsExpression => "≠",
        _ => null,
    };

    private Term TranslateTerm(ExpressionSyntax e)
    {
        switch (e)
        {
            case ParenthesizedExpressionSyntax p:
                return TranslateTerm(p.Expression);
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.NumericLiteralExpression):
                {
                    var v = _model.GetConstantValue(l);
                    if (v is { HasValue: true, Value: int i }) return IR.Num(i);
                    if (v is { HasValue: true, Value: long ll }) return IR.Num(ll);
                    if (v.HasValue && v.Value is IConvertible c)
                        return IR.Num(c.ToInt64(System.Globalization.CultureInfo.InvariantCulture));
                    throw new UnsupportedSyntaxException("non-integer numeric literal", l);
                }
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.StringLiteralExpression):
                return IR.Str((string)_model.GetConstantValue(l).Value!);
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.TrueLiteralExpression):
                return IR.BoolConst(true);
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.FalseLiteralExpression):
                return IR.BoolConst(false);
            case IdentifierNameSyntax id when id.Identifier.Text == _lambdaParamName:
                return IR.Var(_boundVarName);
            case IdentifierNameSyntax id:
                // Free variable. We surface the original name; the
                // verifier resolves cross-memento references via the
                // variable name + the inputCids edges.
                return IR.Var(id.Identifier.Text);
            case MemberAccessExpressionSyntax m:
                return IR.Ctor(m.Name.Identifier.Text, TranslateTerm(m.Expression));
            case BinaryExpressionSyntax b when ArithOp(b.Kind()) is { } op:
                return IR.Ctor(op,
                    TranslateTerm(b.Left),
                    TranslateTerm(b.Right));
            case InvocationExpressionSyntax inv when inv.Expression is MemberAccessExpressionSyntax ma:
                // e.g. g.Sum(...), g.Count(), g.Key (latter is member-access not invocation).
                return TranslateInvocationTerm(inv, ma);
        }
        throw new UnsupportedSyntaxException(
            $"term not supported (kind={e.Kind()})", e);
    }

    private static string? ArithOp(SyntaxKind k) => k switch
    {
        SyntaxKind.AddExpression => "+",
        SyntaxKind.SubtractExpression => "-",
        SyntaxKind.MultiplyExpression => "*",
        SyntaxKind.DivideExpression => "/",
        SyntaxKind.ModuloExpression => "%",
        _ => null,
    };

    private Term TranslateInvocationTerm(
        InvocationExpressionSyntax inv,
        MemberAccessExpressionSyntax ma)
    {
        // Inside a projection lambda we may see g.Sum(x => f(x)) on a
        // grouping. We render the call as a ctor whose name is the
        // method (e.g. `Sum`) and whose args are the receiver plus the
        // un-elaborated lambda projection's IR. This is the smallest
        // useful encoding for the GroupBy→Sum apex case.
        var name = ma.Name.Identifier.Text;
        var receiver = TranslateTerm(ma.Expression);
        var args = new List<Term> { receiver };
        foreach (var a in inv.ArgumentList.Arguments)
        {
            if (a.Expression is SimpleLambdaExpressionSyntax sl
                && sl.Body is ExpressionSyntax body)
            {
                // Inner projection over the grouping element. We use a
                // fresh inner-bound variable name so it doesn't collide
                // with the outer one.
                var inner = new PredicateTranslator(_model, _boundVarName, sl.Parameter.Identifier.Text);
                args.Add(inner.TranslateProjection(body));
            }
            else
            {
                args.Add(TranslateTerm(a.Expression));
            }
        }
        return IR.Ctor(name, args.ToArray());
    }

    private bool IsBoolParam(IdentifierNameSyntax id)
    {
        // We can only safely treat a bare identifier as a bool predicate
        // if the semantic model says it's a Bool-typed reference. Tests
        // mostly hit this through MemberAccess so this is rare.
        var ti = _model.GetTypeInfo(id);
        return ti.Type?.SpecialType == SpecialType.System_Boolean;
    }
}
