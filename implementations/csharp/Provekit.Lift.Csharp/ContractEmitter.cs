using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Blake3;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace Provekit.Lift.Csharp;

public class ContractEmitter
{
    private readonly MethodDeclarationSyntax _method;
    private readonly SemanticModel _model;
    private readonly string _path;
    private readonly List<JsonObject> _effects = new();
    private readonly HashSet<string> _seenEffects = new();
    public ContractEmitter(MethodDeclarationSyntax method, SemanticModel model, string path)
    {
        _method = method;
        _model = model;
        _path = path;
    }

    public JsonObject? Emit()
    {
        var body = _method.Body;
        if (body is null) return null;

        var stmtTerm = EmitStatement(body);
        if (stmtTerm is null) return null;

        var returnExpr = ExtractReturnExpression(body);
        var postValue = returnExpr is not null ? EmitExpression(returnExpr) : stmtTerm;

        var parameters = _method.ParameterList.Parameters;
        var formals = parameters.Select(p => p.Identifier.Text).ToList();
        var formalSorts = parameters.Select(_ => PrimSort("Int")).ToList();

        var line = _method.GetLocation().GetLineSpan().StartLinePosition.Line + 1;

        return new JsonObject
        {
            ["schemaVersion"] = "1",
            ["kind"] = "function-contract",
            ["fnName"] = _method.Identifier.Text,
            ["formals"] = JsonSerializer.SerializeToNode(formals),
            ["formalSorts"] = JsonSerializer.SerializeToNode(formalSorts),
            ["returnSort"] = PrimSort("Int"),
            ["pre"] = TrueFormula(),
            ["post"] = EqFormula(VarTerm("return_value"), postValue),
            ["bodyCid"] = null,
            ["effects"] = JsonSerializer.SerializeToNode(SortEffects(_effects)),
            ["locus"] = new JsonObject { ["file"] = _path, ["line"] = line, ["col"] = 1 },
            ["autoMintedMementos"] = new JsonArray(),
        };
    }

    private JsonObject? EmitStatement(StatementSyntax stmt)
    {
        switch (stmt)
        {
            case BlockSyntax block:
            {
                JsonObject? result = null;
                foreach (var s in block.Statements)
                {
                    var emitted = EmitStatement(s);
                    if (emitted is null) continue;
                    result = result is null ? emitted : Seq(result, emitted);
                }
                return result ?? Skip();
            }

            case ReturnStatementSyntax ret:
            {
                if (ret.Expression is null) return Ctor("csharp:return", Unit());
                var expr = EmitExpression(ret.Expression);
                return Ctor("csharp:return", expr);
            }

            case LocalDeclarationStatementSyntax local:
            {
                JsonObject? result = null;
                foreach (var v in local.Declaration.Variables)
                {
                    var name = v.Identifier.Text;
                    var init = v.Initializer is not null ? EmitExpression(v.Initializer.Value) : IntConst(0);
                    var decl = Ctor("csharp:decl", StrConst(name), init);
                    result = result is null ? decl : Seq(result, decl);
                }
                return result;
            }

            case ExpressionStatementSyntax exprStmt:
            {
                var expr = EmitExpression(exprStmt.Expression);
                return expr;
            }

            case IfStatementSyntax ifStmt:
            {
                var cond = EmitExpression(ifStmt.Condition);
                var thenBody = EmitStatement(ifStmt.Statement);
                var elseBody = ifStmt.Else is not null
                    ? EmitStatement(ifStmt.Else.Statement)
                    : Skip();
                return Ctor("csharp:if", cond, thenBody ?? Skip(), elseBody ?? Skip());
            }

            case WhileStatementSyntax whileStmt:
            {
                var cond = EmitExpression(whileStmt.Condition);
                var bodyTerm = EmitStatement(whileStmt.Statement);
                var loopTerm = Ctor("csharp:while", cond, bodyTerm ?? Skip());
                AddOpaqueLoopEffect(loopTerm);
                return loopTerm;
            }

            case ForStatementSyntax forStmt:
            {
                JsonObject? init = null;
                if (forStmt.Declaration != null)
                {
                    foreach (var v in forStmt.Declaration.Variables)
                    {
                        var name = v.Identifier.Text;
                        var val = v.Initializer is not null ? EmitExpression(v.Initializer.Value) : IntConst(0);
                        var decl = Ctor("csharp:decl", StrConst(name), val);
                        init = init is null ? decl : Seq(init, decl);
                    }
                }
                foreach (var initExpr in forStmt.Initializers)
                {
                    var emitted = EmitExpression(initExpr);
                    init = init is null ? emitted : Seq(init, emitted);
                }

                var cond = forStmt.Condition is not null
                    ? EmitExpression(forStmt.Condition)
                    : BoolConst(true);

                JsonObject? update = null;
                foreach (var inc in forStmt.Incrementors)
                {
                    var u = EmitExpression(inc);
                    update = update is null ? u : Seq(update, u);
                }

                var bodyTerm = EmitStatement(forStmt.Statement);
                var forTerm = Ctor("csharp:for",
                    init ?? Skip(),
                    cond,
                    update ?? Skip(),
                    bodyTerm ?? Skip());
                AddOpaqueLoopEffect(forTerm);
                return forTerm;
            }

            case ForEachStatementSyntax foreachStmt:
            {
                var foreachTerm = Ctor("csharp:foreach",
                    StrConst(foreachStmt.Identifier.Text),
                    EmitExpression(foreachStmt.Expression),
                    EmitStatement(foreachStmt.Statement) ?? Skip());
                AddOpaqueLoopEffect(foreachTerm);
                return foreachTerm;
            }

            case BreakStatementSyntax:
                return Ctor("csharp:break", Unit());

            case ContinueStatementSyntax:
                return Ctor("csharp:continue", Unit());

            default:
                throw new NotSupportedException($"unhandled statement kind: {stmt.Kind()}");
        }
    }

    private JsonObject EmitExpression(ExpressionSyntax expr)
    {
        switch (expr)
        {
            case LiteralExpressionSyntax lit:
            {
                return lit.Token.Value switch
                {
                    int i => IntConst(i),
                    long l => IntConst((int)l),
                    bool b => BoolConst(b),
                    string s => StrConst(s),
                    null => IntConst(0),
                    _ => IntConst(0),
                };
            }

            case IdentifierNameSyntax id:
                return VarTerm(id.Identifier.Text);

            case BinaryExpressionSyntax bin:
            {
                var lhs = EmitExpression(bin.Left);
                var rhs = EmitExpression(bin.Right);
                var op = bin.Kind() switch
                {
                    SyntaxKind.AddExpression => "csharp:add",
                    SyntaxKind.SubtractExpression => "csharp:sub",
                    SyntaxKind.MultiplyExpression => "csharp:mul",
                    SyntaxKind.DivideExpression => "csharp:div",
                    SyntaxKind.ModuloExpression => "csharp:mod",
                    SyntaxKind.EqualsExpression => "csharp:eq",
                    SyntaxKind.NotEqualsExpression => "csharp:ne",
                    SyntaxKind.LessThanExpression => "csharp:lt",
                    SyntaxKind.LessThanOrEqualExpression => "csharp:le",
                    SyntaxKind.GreaterThanExpression => "csharp:gt",
                    SyntaxKind.GreaterThanOrEqualExpression => "csharp:ge",
                    SyntaxKind.LogicalAndExpression => "csharp:and",
                    SyntaxKind.LogicalOrExpression => "csharp:or",
                    SyntaxKind.BitwiseAndExpression => "csharp:bitand",
                    SyntaxKind.BitwiseOrExpression => "csharp:bitor",
                    SyntaxKind.ExclusiveOrExpression => "csharp:bitxor",
                    SyntaxKind.LeftShiftExpression => "csharp:shl",
                    SyntaxKind.RightShiftExpression => "csharp:shr",
                    _ => throw new NotSupportedException($"unhandled binary operator kind: {bin.Kind()}"),
                };
                return Ctor(op, lhs, rhs);
            }

            case PrefixUnaryExpressionSyntax pre:
            {
                var operand = EmitExpression(pre.Operand);
                var op = pre.Kind() switch
                {
                    SyntaxKind.UnaryMinusExpression => "csharp:neg",
                    SyntaxKind.LogicalNotExpression => "csharp:not",
                    SyntaxKind.BitwiseNotExpression => "csharp:bitnot",
                    SyntaxKind.PreIncrementExpression => "csharp:preinc",
                    SyntaxKind.PreDecrementExpression => "csharp:predec",
                    _ => throw new NotSupportedException($"unhandled prefix unary operator kind: {pre.Kind()}"),
                };
                return Ctor(op, operand);
            }

            case PostfixUnaryExpressionSyntax post:
            {
                var operand = EmitExpression(post.Operand);
                var op = post.Kind() switch
                {
                    SyntaxKind.PostIncrementExpression => "csharp:postinc",
                    SyntaxKind.PostDecrementExpression => "csharp:postdec",
                    _ => throw new NotSupportedException($"unhandled postfix unary operator kind: {post.Kind()}"),
                };
                return Ctor(op, operand);
            }

            case InvocationExpressionSyntax inv:
            {
                var symbol = _model.GetSymbolInfo(inv).Symbol as IMethodSymbol;
                var methodName = symbol?.ToDisplayString() ?? "unknown";

                var args = inv.ArgumentList.Arguments
                    .Select(a => EmitExpression(a.Expression))
                    .ToList();

                AddUnresolvedCallEffect(methodName);

                var callArgs = new List<JsonObject> { StrConst(methodName) };
                callArgs.AddRange(args);
                return Ctor("csharp:call", callArgs.ToArray());
            }

            case ParenthesizedExpressionSyntax paren:
                return EmitExpression(paren.Expression);

            case MemberAccessExpressionSyntax ma:
                return Ctor("csharp:member",
                    EmitExpression(ma.Expression),
                    StrConst(ma.Name.Identifier.Text));

            case ConditionalExpressionSyntax cond:
                return Ctor("csharp:ite",
                    EmitExpression(cond.Condition),
                    EmitExpression(cond.WhenTrue),
                    EmitExpression(cond.WhenFalse));

            case AssignmentExpressionSyntax assign:
            {
                var rhs = EmitExpression(assign.Right);
                AddWriteEffect(assign.Left.ToString());
                return Ctor("csharp:assign", EmitExpression(assign.Left), rhs);
            }

            case ObjectCreationExpressionSyntax obj:
            {
                var typeName = obj.Type.ToString();
                var args = obj.ArgumentList?.Arguments
                    .Select(a => EmitExpression(a.Expression))
                    .ToList() ?? new();
                var allArgs = new List<JsonObject> { StrConst(typeName) };
                allArgs.AddRange(args);
                return Ctor("csharp:new", allArgs.ToArray());
            }

            case ElementAccessExpressionSyntax elem:
                return Ctor("csharp:index",
                    EmitExpression(elem.Expression),
                    EmitExpression(elem.ArgumentList.Arguments[0].Expression));

            case CastExpressionSyntax cast:
                return Ctor("csharp:cast",
                    StrConst(cast.Type.ToString()),
                    EmitExpression(cast.Expression));

            default:
                throw new NotSupportedException($"unhandled expression kind: {expr.Kind()}");
        }
    }

    private JsonObject Ctor(string name, params JsonObject[] args)
    {
        return new JsonObject
        {
            ["kind"] = "ctor",
            ["name"] = name,
            ["args"] = JsonSerializer.SerializeToNode(args.ToList()),
        };
    }

    private JsonObject VarTerm(string name) => new()
    {
        ["kind"] = "var", ["name"] = name
    };

    private JsonObject IntConst(long value) => new()
    {
        ["kind"] = "const",
        ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("Int"),
    };

    private JsonObject BoolConst(bool value) => new()
    {
        ["kind"] = "const",
        ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("Bool"),
    };

    private JsonObject StrConst(string value) => new()
    {
        ["kind"] = "const",
        ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("String"),
    };

    private JsonObject Unit() => IntConst(0);

    private static ExpressionSyntax? ExtractReturnExpression(StatementSyntax body)
    {
        if (body is BlockSyntax block && block.Statements.Count == 1 && block.Statements[0] is ReturnStatementSyntax ret)
            return ret.Expression;
        if (body is ReturnStatementSyntax rs)
            return rs.Expression;
        return null;
    }

    private JsonObject Skip() => Ctor("csharp:skip", Unit());

    private JsonObject Seq(JsonObject first, JsonObject second) =>
        Ctor("csharp:seq", first, second);

    private static JsonObject PrimSort(string name) => new()
    {
        ["kind"] = "primitive", ["name"] = name
    };

    private static JsonObject TrueFormula() => new()
    {
        ["kind"] = "atomic", ["name"] = "true", ["args"] = new JsonArray()
    };

    private static JsonObject EqFormula(JsonObject lhs, JsonObject rhs) => new()
    {
        ["kind"] = "atomic", ["name"] = "=",
        ["args"] = JsonSerializer.SerializeToNode(new[] { lhs, rhs })
    };

    private void AddWriteEffect(string target)
    {
        if (_seenEffects.Add($"writes:{target}"))
            _effects.Add(new JsonObject { ["kind"] = "writes", ["target"] = target });
    }

    private void AddUnresolvedCallEffect(string callee)
    {
        if (_seenEffects.Add($"call:{callee}"))
            _effects.Add(new JsonObject { ["kind"] = "unresolved_call", ["name"] = callee });
    }

    private void AddOpaqueLoopEffect(JsonObject loopTerm)
    {
        var json = JsonSerializer.Serialize(loopTerm, JcsOptions);
        var output = new byte[64];
        Hasher.Hash(Encoding.UTF8.GetBytes(json), output);
        var hex = Convert.ToHexString(output).ToLowerInvariant();
        var cid = $"blake3-512:{hex}";
        if (_seenEffects.Add($"loop:{cid}"))
            _effects.Add(new JsonObject { ["kind"] = "opaque_loop", ["loopCid"] = cid });
    }

    private static List<JsonObject> SortEffects(List<JsonObject> effects)
    {
        return [.. effects.OrderBy(e => EffectSortKey(e))];
    }

    private static int EffectSortKey(JsonObject e)
    {
        return e["kind"]?.GetValue<string>() switch
        {
            "reads" => 0,
            "writes" => 1,
            "io" => 2,
            "unsafe" => 3,
            "panics" => 4,
            "unresolved_call" => 5,
            "opaque_loop" => 6,
            _ => 99,
        };
    }

    private static readonly JsonSerializerOptions JcsOptions = new()
    {
        WriteIndented = false,
    };
}
