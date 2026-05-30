using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;
using Provekit.Canonicalizer;
using V = Provekit.Canonicalizer.Value;

namespace Provekit.Lift.Csharp;

public static class CsharpAstTemplates
{
    public static JsonObject BodySource(MethodDeclarationSyntax method, string file)
    {
        var sourceText = method.SyntaxTree.GetText();
        var bodyText = MethodBodyText(method, sourceText);
        var paramNames = ParameterNames(method);
        var astTemplate = MethodBodyTemplate(method, paramNames);

        return new JsonObject
        {
            ["ast_template"] = astTemplate,
            ["body_text"] = bodyText,
            ["file"] = file,
            ["param_names"] = JsonSerializer.SerializeToNode(paramNames),
            ["source_cid"] = CidOfUtf8(bodyText),
            ["span"] = SpanFor(method),
            ["template_cid"] = TemplateCid(astTemplate),
        };
    }

    public static JsonObject MethodBodyTemplate(MethodDeclarationSyntax method) =>
        MethodBodyTemplate(method, ParameterNames(method));

    public static string TemplateCid(JsonNode template) =>
        CidOfUtf8(Jcs.Encode(ToCanonicalValue(template)));

    public static string CidOfUtf8(string text) => Hash.Blake3_512Utf8(text);

    private static JsonObject MethodBodyTemplate(MethodDeclarationSyntax method, IReadOnlyList<string> paramNames)
    {
        var statements = new JsonArray();
        if (method.Body is not null)
        {
            foreach (var statement in method.Body.Statements)
            {
                statements.Add(StatementTemplate(statement, paramNames));
            }
        }
        else if (method.ExpressionBody?.Expression is { } expression)
        {
            statements.Add(new JsonObject
            {
                ["kind"] = "expr_stmt",
                ["expr"] = ExpressionTemplate(expression, paramNames),
                ["trailing_semi"] = false,
            });
        }

        return new JsonObject
        {
            ["kind"] = "block",
            ["stmts"] = statements,
        };
    }

    private static JsonObject StatementTemplate(StatementSyntax statement, IReadOnlyList<string> paramNames)
    {
        return statement switch
        {
            ReturnStatementSyntax ret => new JsonObject
            {
                ["kind"] = "return",
                ["expr"] = ret.Expression is null ? null : ExpressionTemplate(ret.Expression, paramNames),
            },
            ExpressionStatementSyntax expr => new JsonObject
            {
                ["kind"] = "expr_stmt",
                ["expr"] = ExpressionTemplate(expr.Expression, paramNames),
                ["trailing_semi"] = true,
            },
            LocalDeclarationStatementSyntax local => LocalDeclarationTemplate(local, paramNames),
            IfStatementSyntax ifStmt => new JsonObject
            {
                ["kind"] = "if",
                ["cond"] = ExpressionTemplate(ifStmt.Condition, paramNames),
                ["then"] = StatementTemplate(ifStmt.Statement, paramNames),
                ["else"] = ifStmt.Else is null ? null : StatementTemplate(ifStmt.Else.Statement, paramNames),
            },
            BlockSyntax block => BlockTemplate(block, paramNames),
            _ => RawStatement(statement),
        };
    }

    private static JsonObject BlockTemplate(BlockSyntax block, IReadOnlyList<string> paramNames)
    {
        var statements = new JsonArray();
        foreach (var statement in block.Statements)
        {
            statements.Add(StatementTemplate(statement, paramNames));
        }

        return new JsonObject
        {
            ["kind"] = "block",
            ["stmts"] = statements,
        };
    }

    private static JsonObject LocalDeclarationTemplate(LocalDeclarationStatementSyntax local, IReadOnlyList<string> paramNames)
    {
        var declarations = new JsonArray();
        foreach (var variable in local.Declaration.Variables)
        {
            declarations.Add(new JsonObject
            {
                ["kind"] = "binding",
                ["name"] = variable.Identifier.Text,
                ["init"] = variable.Initializer?.Value is null
                    ? null
                    : ExpressionTemplate(variable.Initializer.Value, paramNames),
            });
        }

        return new JsonObject
        {
            ["kind"] = "let",
            ["declarations"] = declarations,
        };
    }

    private static JsonObject ExpressionTemplate(ExpressionSyntax expression, IReadOnlyList<string> paramNames)
    {
        expression = StripParens(expression);
        return expression switch
        {
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
            AssignmentExpressionSyntax assign => new JsonObject
            {
                ["kind"] = "assign",
                ["op"] = assign.OperatorToken.Text,
                ["left"] = ExpressionTemplate(assign.Left, paramNames),
                ["right"] = ExpressionTemplate(assign.Right, paramNames),
            },
            ObjectCreationExpressionSyntax obj => ObjectCreationTemplate(obj, paramNames),
            ElementAccessExpressionSyntax element => ElementAccessTemplate(element, paramNames),
            PrefixUnaryExpressionSyntax prefix => new JsonObject
            {
                ["kind"] = "prefix",
                ["op"] = prefix.OperatorToken.Text,
                ["expr"] = ExpressionTemplate(prefix.Operand, paramNames),
            },
            PostfixUnaryExpressionSyntax postfix => new JsonObject
            {
                ["kind"] = "postfix",
                ["op"] = postfix.OperatorToken.Text,
                ["expr"] = ExpressionTemplate(postfix.Operand, paramNames),
            },
            ConditionalExpressionSyntax cond => new JsonObject
            {
                ["kind"] = "conditional",
                ["cond"] = ExpressionTemplate(cond.Condition, paramNames),
                ["when_true"] = ExpressionTemplate(cond.WhenTrue, paramNames),
                ["when_false"] = ExpressionTemplate(cond.WhenFalse, paramNames),
            },
            CastExpressionSyntax cast => new JsonObject
            {
                ["kind"] = "cast",
                ["type"] = cast.Type.ToString(),
                ["expr"] = ExpressionTemplate(cast.Expression, paramNames),
            },
            AwaitExpressionSyntax awaitExpr => new JsonObject
            {
                ["kind"] = "await",
                ["expr"] = ExpressionTemplate(awaitExpr.Expression, paramNames),
            },
            _ => RawExpression(expression),
        };
    }

    private static JsonObject InvocationTemplate(InvocationExpressionSyntax invocation, IReadOnlyList<string> paramNames)
    {
        var args = new JsonArray();
        foreach (var argument in invocation.ArgumentList.Arguments)
        {
            args.Add(ExpressionTemplate(argument.Expression, paramNames));
        }

        if (invocation.Expression is MemberAccessExpressionSyntax member)
        {
            return new JsonObject
            {
                ["kind"] = "method_call",
                ["receiver"] = ExpressionTemplate(member.Expression, paramNames),
                ["method"] = member.Name.ToString(),
                ["args"] = args,
            };
        }

        return new JsonObject
        {
            ["kind"] = "call",
            ["func"] = ExpressionTemplate(invocation.Expression, paramNames),
            ["args"] = args,
        };
    }

    private static JsonObject ObjectCreationTemplate(ObjectCreationExpressionSyntax obj, IReadOnlyList<string> paramNames)
    {
        var args = new JsonArray();
        foreach (var argument in obj.ArgumentList?.Arguments ?? default(SeparatedSyntaxList<ArgumentSyntax>))
        {
            args.Add(ExpressionTemplate(argument.Expression, paramNames));
        }

        return new JsonObject
        {
            ["kind"] = "new",
            ["type"] = obj.Type.ToString(),
            ["args"] = args,
        };
    }

    private static JsonObject ElementAccessTemplate(ElementAccessExpressionSyntax element, IReadOnlyList<string> paramNames)
    {
        var args = new JsonArray();
        foreach (var argument in element.ArgumentList.Arguments)
        {
            args.Add(ExpressionTemplate(argument.Expression, paramNames));
        }

        return new JsonObject
        {
            ["kind"] = "index",
            ["receiver"] = ExpressionTemplate(element.Expression, paramNames),
            ["args"] = args,
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

    private static JsonObject RawStatement(StatementSyntax statement) => new()
    {
        ["kind"] = "raw_stmt",
        ["syntax_kind"] = statement.Kind().ToString(),
        ["text"] = statement.NormalizeWhitespace().ToFullString(),
    };

    private static JsonObject RawExpression(ExpressionSyntax expression) => new()
    {
        ["kind"] = "raw_expr",
        ["syntax_kind"] = expression.Kind().ToString(),
        ["text"] = expression.NormalizeWhitespace().ToFullString(),
    };

    private static ExpressionSyntax StripParens(ExpressionSyntax expression)
    {
        while (expression is ParenthesizedExpressionSyntax paren)
        {
            expression = paren.Expression;
        }

        return expression;
    }

    private static string MethodBodyText(MethodDeclarationSyntax method, SourceText sourceText)
    {
        if (method.Body is not null)
        {
            var start = method.Body.OpenBraceToken.Span.End;
            var end = method.Body.CloseBraceToken.Span.Start;
            if (start <= end)
            {
                return sourceText.ToString(TextSpan.FromBounds(start, end)).Trim();
            }
        }

        return method.ExpressionBody?.Expression is { } expression
            ? sourceText.ToString(expression.Span).Trim()
            : "";
    }

    private static IReadOnlyList<string> ParameterNames(MethodDeclarationSyntax method) =>
        method.ParameterList.Parameters.Select(parameter => parameter.Identifier.Text).ToArray();

    private static JsonObject SpanFor(SyntaxNode node)
    {
        var span = node.GetLocation().GetLineSpan();
        return new JsonObject
        {
            ["start_line"] = span.StartLinePosition.Line + 1,
            ["start_col"] = span.StartLinePosition.Character,
            ["end_line"] = span.EndLinePosition.Line + 1,
            ["end_col"] = span.EndLinePosition.Character,
        };
    }

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
