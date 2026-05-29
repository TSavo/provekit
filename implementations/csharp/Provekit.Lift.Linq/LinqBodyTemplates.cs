using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Operations;
using Provekit.Canonicalizer;
using V = Provekit.Canonicalizer.Value;

namespace Provekit.Lift.Linq;

public sealed record LinqSourceSpan(int StartLine, int StartCol, int EndLine, int EndCol);

public sealed record LinqBodySource(
    string File,
    LinqSourceSpan Span,
    string SourceCid,
    string BodyText,
    JsonObject AstTemplate,
    string TemplateCid,
    IReadOnlyList<string> ParamNames);

internal static class LinqBodyTemplates
{
    public static LinqBodySource BodySource(IInvocationOperation operation, string outBinding)
    {
        var syntax = operation.Syntax;
        var sourceText = syntax.SyntaxTree.GetText();
        var invocationText = sourceText.ToString(syntax.Span).Trim();
        var bodyText = string.IsNullOrWhiteSpace(outBinding)
            ? invocationText
            : $"{outBinding} = {invocationText}";
        var paramNames = LambdaParamNames(syntax);
        var astTemplate = syntax is InvocationExpressionSyntax invocation
            ? InvocationTemplate(invocation, paramNames)
            : new JsonObject
            {
                ["kind"] = "raw_linq",
                ["syntax_kind"] = syntax.Kind().ToString(),
                ["text"] = syntax.NormalizeWhitespace().ToFullString(),
            };

        return new LinqBodySource(
            File: syntax.SyntaxTree.FilePath,
            Span: SpanFor(syntax),
            SourceCid: Hash.Blake3_512Utf8(bodyText),
            BodyText: bodyText,
            AstTemplate: astTemplate,
            TemplateCid: TemplateCid(astTemplate),
            ParamNames: paramNames);
    }

    private static JsonObject InvocationTemplate(InvocationExpressionSyntax invocation, IReadOnlyList<string> paramNames)
    {
        var args = new JsonArray();
        foreach (var argument in invocation.ArgumentList.Arguments)
        {
            args.Add(ExpressionTemplate(argument.Expression, paramNames));
        }

        var method = invocation.Expression is MemberAccessExpressionSyntax member
            ? member.Name.ToString()
            : invocation.Expression.ToString();
        var receiver = invocation.Expression is MemberAccessExpressionSyntax receiverMember
            ? ExpressionTemplate(receiverMember.Expression, paramNames)
            : null;

        var template = new JsonObject
        {
            ["kind"] = "linq_invocation",
            ["method"] = method,
            ["receiver"] = receiver,
            ["args"] = args,
        };

        if (FirstLambdaBody(invocation) is { } predicate)
        {
            template["predicate"] = ExpressionTemplate(predicate, paramNames);
        }

        return template;
    }

    private static JsonObject ExpressionTemplate(ExpressionSyntax expression, IReadOnlyList<string> paramNames)
    {
        expression = StripParens(expression);
        return expression switch
        {
            SimpleLambdaExpressionSyntax lambda when lambda.Body is ExpressionSyntax body => ExpressionTemplate(body, paramNames),
            ParenthesizedLambdaExpressionSyntax lambda when lambda.Body is ExpressionSyntax body => ExpressionTemplate(body, paramNames),
            IdentifierNameSyntax id => IdentifierTemplate(id.Identifier.Text, paramNames),
            LiteralExpressionSyntax literal => LiteralTemplate(literal),
            BinaryExpressionSyntax binary => new JsonObject
            {
                ["kind"] = "binary",
                ["op"] = binary.OperatorToken.Text,
                ["left"] = ExpressionTemplate(binary.Left, paramNames),
                ["right"] = ExpressionTemplate(binary.Right, paramNames),
            },
            InvocationExpressionSyntax invocation => InvocationTemplate(invocation, paramNames),
            MemberAccessExpressionSyntax member => new JsonObject
            {
                ["kind"] = "member",
                ["receiver"] = ExpressionTemplate(member.Expression, paramNames),
                ["member"] = member.Name.ToString(),
            },
            _ => new JsonObject
            {
                ["kind"] = "raw_expr",
                ["syntax_kind"] = expression.Kind().ToString(),
                ["text"] = expression.NormalizeWhitespace().ToFullString(),
            },
        };
    }

    private static JsonObject IdentifierTemplate(string name, IReadOnlyList<string> paramNames)
    {
        var index = IndexOf(paramNames, name);
        if (index >= 0)
        {
            return new JsonObject
            {
                ["kind"] = "param_ref",
                ["index"] = index + 1,
            };
        }

        return new JsonObject
        {
            ["kind"] = "ident",
            ["name"] = name,
        };
    }

    private static int IndexOf(IReadOnlyList<string> items, string value)
    {
        for (var i = 0; i < items.Count; i++)
        {
            if (items[i] == value) return i;
        }

        return -1;
    }

    private static JsonObject LiteralTemplate(LiteralExpressionSyntax literal)
    {
        var value = literal.Token.Value;
        return new JsonObject
        {
            ["kind"] = "literal",
            ["ty"] = value switch
            {
                int or long or short or byte => "int",
                bool => "bool",
                string => "string",
                null => "null",
                _ => literal.Kind().ToString(),
            },
            ["value"] = value switch
            {
                int i => i,
                long l => l,
                short s => s,
                byte b => b,
                bool b => b,
                string s => s,
                null => null,
                _ => literal.Token.Text,
            },
        };
    }

    private static IReadOnlyList<string> LambdaParamNames(SyntaxNode syntax)
    {
        var names = new List<string>();
        foreach (var lambda in syntax.DescendantNodesAndSelf().OfType<SimpleLambdaExpressionSyntax>())
        {
            names.Add(lambda.Parameter.Identifier.Text);
        }
        foreach (var lambda in syntax.DescendantNodesAndSelf().OfType<ParenthesizedLambdaExpressionSyntax>())
        {
            names.AddRange(lambda.ParameterList.Parameters.Select(parameter => parameter.Identifier.Text));
        }
        return names;
    }

    private static ExpressionSyntax? FirstLambdaBody(InvocationExpressionSyntax invocation)
    {
        foreach (var argument in invocation.ArgumentList.Arguments)
        {
            if (argument.Expression is SimpleLambdaExpressionSyntax { Body: ExpressionSyntax body })
            {
                return body;
            }
            if (argument.Expression is ParenthesizedLambdaExpressionSyntax { Body: ExpressionSyntax body2 })
            {
                return body2;
            }
        }
        return null;
    }

    private static ExpressionSyntax StripParens(ExpressionSyntax expression)
    {
        while (expression is ParenthesizedExpressionSyntax paren)
        {
            expression = paren.Expression;
        }

        return expression;
    }

    private static LinqSourceSpan SpanFor(SyntaxNode node)
    {
        var span = node.GetLocation().GetLineSpan();
        return new LinqSourceSpan(
            span.StartLinePosition.Line + 1,
            span.StartLinePosition.Character,
            span.EndLinePosition.Line + 1,
            span.EndLinePosition.Character);
    }

    private static string TemplateCid(JsonNode template) =>
        Hash.Blake3_512Utf8(Jcs.Encode(ToCanonicalValue(template)));

    private static V ToCanonicalValue(JsonNode node)
    {
        using var doc = JsonDocument.Parse(node.ToJsonString());
        return ElementToValue(doc.RootElement);
    }

    private static V ElementToValue(JsonElement element)
    {
        return element.ValueKind switch
        {
            JsonValueKind.Null => V.Null,
            JsonValueKind.True => V.True,
            JsonValueKind.False => V.False,
            JsonValueKind.String => V.String(element.GetString() ?? ""),
            JsonValueKind.Number => V.Integer(element.GetInt64()),
            JsonValueKind.Array => V.Array(element.EnumerateArray().Select(ElementToValue)),
            JsonValueKind.Object => V.Object(element.EnumerateObject()
                .Select(property => new KeyValuePair<string, V>(property.Name, ElementToValue(property.Value)))),
            _ => V.Null,
        };
    }
}
