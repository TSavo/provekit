using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Blake3;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Provekit.IR;

namespace Provekit.Lift.Csharp;

internal static class CsharpFunctionContractWalker
{
    internal sealed record MethodBinding(
        MethodDeclarationSyntax Method,
        IMethodSymbol Symbol,
        JsonObject Contract);

    private sealed record FunctionInfo(
        MethodDeclarationSyntax Method,
        IMethodSymbol Symbol,
        JsonObject Contract,
        string Name,
        IReadOnlyList<string> Formals,
        IReadOnlyList<JsonObject> FormalSorts,
        JsonObject OwnPre);

    private sealed record CallsiteHit(
        FunctionInfo Caller,
        FunctionInfo Callee,
        InvocationExpressionSyntax Invocation,
        IReadOnlyList<JsonObject> Conditions,
        IReadOnlyDictionary<string, JsonObject> LocalBindings);

    private sealed record PendingEdge(
        FunctionInfo Caller,
        FunctionInfo Callee,
        InvocationExpressionSyntax Invocation);

    public static void Apply(
        IReadOnlyList<MethodBinding> methods,
        SemanticModel model,
        string path,
        LiftResult result)
    {
        if (methods.Count == 0) return;

        var infos = methods
            .Select(binding =>
            {
                var name = binding.Symbol.GetDocumentationCommentId()
                           ?? binding.Contract["fnName"]?.GetValue<string>()
                           ?? binding.Method.Identifier.Text;
                var formals = binding.Method.ParameterList.Parameters
                    .Select(parameter => parameter.Identifier.Text)
                    .ToList();
                var formalSorts = binding.Contract["formalSorts"]?.AsArray()
                    .Select(node => node?.DeepClone().AsObject() ?? PrimSort("Int"))
                    .ToList() ?? formals.Select(_ => PrimSort("Int")).ToList();
                var ownPre = LiftMethodPrecondition(binding.Method);
                binding.Contract["pre"] = ownPre.DeepClone();
                return new FunctionInfo(
                    binding.Method,
                    binding.Symbol,
                    binding.Contract,
                    name,
                    formals,
                    formalSorts,
                    ownPre);
            })
            .ToList();

        var extraPreconditions = infos.ToDictionary(
            info => info,
            _ => new List<JsonObject>());
        var pendingEdges = new List<PendingEdge>();
        var callsiteContracts = new List<JsonObject>();
        var usedCallsiteNames = new HashSet<string>(StringComparer.Ordinal);

        foreach (var caller in infos)
        {
            foreach (var hit in FindCallsites(caller, infos, model))
            {
                var actuals = hit.Invocation.ArgumentList.Arguments
                    .Select(argument => LiftTerm(argument.Expression))
                    .ToList();
                if (actuals.Count != hit.Callee.Formals.Count) continue;

                var substitutions = new Dictionary<string, JsonObject>(StringComparer.Ordinal);
                for (var i = 0; i < hit.Callee.Formals.Count; i++)
                    substitutions[hit.Callee.Formals[i]] = actuals[i];

                var composed = SubstituteFormula(hit.Callee.OwnPre, substitutions);
                if (hit.LocalBindings.Count > 0)
                    composed = SubstituteFormula(composed, hit.LocalBindings);

                var callerPrecondition = GuardFormula(composed, hit.Conditions);
                extraPreconditions[caller].Add(callerPrecondition.DeepClone().AsObject());

                var universal = UniversalCallsiteFormula(
                    caller,
                    GuardedImplicationFormula(composed, hit.Conditions));
                callsiteContracts.Add(BuildCallsiteContract(
                    caller,
                    hit.Callee,
                    hit.Invocation,
                    path,
                    universal,
                    usedCallsiteNames));
                pendingEdges.Add(new PendingEdge(caller, hit.Callee, hit.Invocation));
            }
        }

        foreach (var info in infos)
        {
            var merged = MergePreconditions(info.OwnPre, extraPreconditions[info]);
            info.Contract["pre"] = merged;
        }

        foreach (var contract in callsiteContracts)
            result.Declarations.Add(contract);

        foreach (var edge in pendingEdges)
            result.CallEdges.Add(BuildCallEdge(edge, path));
    }

    private static JsonObject BuildCallsiteContract(
        FunctionInfo caller,
        FunctionInfo callee,
        InvocationExpressionSyntax invocation,
        string path,
        JsonObject pre,
        HashSet<string> usedNames)
    {
        var lineSpan = invocation.GetLocation().GetLineSpan();
        var line = lineSpan.StartLinePosition.Line + 1;
        var col = lineSpan.StartLinePosition.Character + 1;
        var baseName = $"{caller.Name}->{callee.Name}::callsite-pre@{path}:{line}:{col}";
        var name = UniqueName(baseName, usedNames);

        return new JsonObject
        {
            ["schemaVersion"] = "1",
            ["kind"] = "function-contract",
            ["fnName"] = name,
            ["formals"] = JsonSerializer.SerializeToNode(caller.Formals),
            ["formalSorts"] = JsonSerializer.SerializeToNode(caller.FormalSorts),
            ["returnSort"] = PrimSort("Bool"),
            ["pre"] = pre,
            ["post"] = TrueFormula(),
            ["bodyCid"] = null,
            ["effects"] = new JsonArray(),
            ["locus"] = new JsonObject { ["file"] = path, ["line"] = line, ["col"] = col },
            ["autoMintedMementos"] = new JsonArray(),
        };
    }

    private static JsonObject BuildCallEdge(PendingEdge edge, string path)
    {
        var lineSpan = edge.Invocation.GetLocation().GetLineSpan();
        var declaration = new CallEdgeDeclaration(
            SourceContractCid: ContractCid(edge.Caller.Contract),
            TargetContractCid: ContractCid(edge.Callee.Contract),
            TargetSymbol: edge.Callee.Name,
            CallSiteLocus: new Locus(
                File: path,
                Line: lineSpan.StartLinePosition.Line + 1,
                Column: lineSpan.StartLinePosition.Character + 1),
            EvidenceTerm: $"callsite-pre({edge.Caller.Name}->{edge.Callee.Name})");

        return JsonNode.Parse(Serialize.MarshalCallEdges(new[] { declaration }))!
            .AsArray()[0]!
            .AsObject();
    }

    private static string ContractCid(JsonObject contract)
    {
        var json = contract.ToJsonString(new JsonSerializerOptions { WriteIndented = false });
        var output = new byte[64];
        Hasher.Hash(Encoding.UTF8.GetBytes(json), output);
        return $"blake3-512:{Convert.ToHexString(output).ToLowerInvariant()}";
    }

    private static string UniqueName(string name, HashSet<string> usedNames)
    {
        if (usedNames.Add(name)) return name;
        var i = 1;
        while (!usedNames.Add($"{name}::{i}")) i++;
        return $"{name}::{i}";
    }

    private static JsonObject LiftMethodPrecondition(MethodDeclarationSyntax method)
    {
        if (method.Body is null) return TrueFormula();

        var contributions = new List<JsonObject>();
        foreach (var statement in method.Body.Statements)
        {
            var contribution = TryPreconditionContribution(statement);
            if (contribution is not null)
                contributions.Add(contribution);
        }

        return AndFormula(contributions);
    }

    private static JsonObject? TryPreconditionContribution(StatementSyntax statement)
    {
        if (statement is IfStatementSyntax ifStatement
            && ifStatement.Else is null
            && StatementOnlyThrows(ifStatement.Statement))
        {
            return LiftNegatedPredicate(ifStatement.Condition);
        }

        if (statement is ExpressionStatementSyntax { Expression: InvocationExpressionSyntax invocation })
            return TryAssertLikePrecondition(invocation);

        return null;
    }

    private static bool StatementOnlyThrows(StatementSyntax statement)
    {
        if (statement is ThrowStatementSyntax) return true;
        return statement is BlockSyntax block
               && block.Statements.Count == 1
               && block.Statements[0] is ThrowStatementSyntax;
    }

    private static JsonObject? TryAssertLikePrecondition(InvocationExpressionSyntax invocation)
    {
        var callee = SimpleCalleeName(invocation.Expression);
        if (callee is "Assert" or "Requires"
            && invocation.ArgumentList.Arguments.Count > 0)
        {
            return LiftPredicate(invocation.ArgumentList.Arguments[0].Expression);
        }

        if (callee is "ThrowIfNull"
            && invocation.ArgumentList.Arguments.Count > 0)
        {
            return AtomicFormula("≠",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                NullConst());
        }

        if (callee is "ThrowIfNegative"
            && invocation.ArgumentList.Arguments.Count > 0)
        {
            return AtomicFormula("≥",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                IntConst(0));
        }

        if (callee is "ThrowIfNegativeOrZero"
            && invocation.ArgumentList.Arguments.Count > 0)
        {
            return AtomicFormula(">",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                IntConst(0));
        }

        if (callee is "ThrowIfLessThan"
            && invocation.ArgumentList.Arguments.Count > 1)
        {
            return AtomicFormula("≥",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                LiftTerm(invocation.ArgumentList.Arguments[1].Expression));
        }

        if (callee is "ThrowIfLessThanOrEqual"
            && invocation.ArgumentList.Arguments.Count > 1)
        {
            return AtomicFormula(">",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                LiftTerm(invocation.ArgumentList.Arguments[1].Expression));
        }

        if (callee is "ThrowIfGreaterThan"
            && invocation.ArgumentList.Arguments.Count > 1)
        {
            return AtomicFormula("≤",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                LiftTerm(invocation.ArgumentList.Arguments[1].Expression));
        }

        if (callee is "ThrowIfGreaterThanOrEqual"
            && invocation.ArgumentList.Arguments.Count > 1)
        {
            return AtomicFormula("<",
                LiftTerm(invocation.ArgumentList.Arguments[0].Expression),
                LiftTerm(invocation.ArgumentList.Arguments[1].Expression));
        }

        return null;
    }

    private static IEnumerable<CallsiteHit> FindCallsites(
        FunctionInfo caller,
        IReadOnlyList<FunctionInfo> functions,
        SemanticModel model)
    {
        if (caller.Method.Body is null) yield break;

        var hits = new List<CallsiteHit>();
        WalkStatements(
            caller.Method.Body.Statements,
            caller,
            functions,
            model,
            new List<JsonObject>(),
            new Dictionary<string, JsonObject>(StringComparer.Ordinal),
            hits);

        foreach (var hit in hits)
            yield return hit;
    }

    private static void WalkStatements(
        SyntaxList<StatementSyntax> statements,
        FunctionInfo caller,
        IReadOnlyList<FunctionInfo> functions,
        SemanticModel model,
        List<JsonObject> conditions,
        Dictionary<string, JsonObject> bindings,
        List<CallsiteHit> hits)
    {
        foreach (var statement in statements)
        {
            WalkStatement(statement, caller, functions, model, conditions, bindings, hits);
            foreach (var binding in BindingsFromStatement(statement))
                bindings[binding.Key] = binding.Value;
        }
    }

    private static void WalkStatement(
        StatementSyntax statement,
        FunctionInfo caller,
        IReadOnlyList<FunctionInfo> functions,
        SemanticModel model,
        List<JsonObject> conditions,
        Dictionary<string, JsonObject> bindings,
        List<CallsiteHit> hits)
    {
        if (statement is BlockSyntax block)
        {
            WalkStatements(
                block.Statements,
                caller,
                functions,
                model,
                conditions.Select(c => c.DeepClone().AsObject()).ToList(),
                CloneBindings(bindings),
                hits);
            return;
        }

        if (statement is IfStatementSyntax ifStatement)
        {
            var condition = LiftPredicate(ifStatement.Condition);

            var thenConditions = conditions.Select(c => c.DeepClone().AsObject()).ToList();
            thenConditions.Add(condition.DeepClone().AsObject());
            WalkStatement(
                ifStatement.Statement,
                caller,
                functions,
                model,
                thenConditions,
                CloneBindings(bindings),
                hits);

            if (ifStatement.Else is not null)
            {
                var elseConditions = conditions.Select(c => c.DeepClone().AsObject()).ToList();
                elseConditions.Add(NegateFormula(condition).AsObject());
                WalkStatement(
                    ifStatement.Else.Statement,
                    caller,
                    functions,
                    model,
                    elseConditions,
                    CloneBindings(bindings),
                    hits);
            }
            return;
        }

        foreach (var invocation in statement.DescendantNodesAndSelf().OfType<InvocationExpressionSyntax>())
        {
            var callee = ResolveCallee(invocation, functions, model);
            if (callee is null) continue;

            hits.Add(new CallsiteHit(
                caller,
                callee,
                invocation,
                conditions.Select(c => c.DeepClone().AsObject()).ToList(),
                CloneBindings(bindings)));
        }
    }

    private static Dictionary<string, JsonObject> CloneBindings(
        IReadOnlyDictionary<string, JsonObject> bindings)
    {
        return bindings.ToDictionary(
            pair => pair.Key,
            pair => pair.Value.DeepClone().AsObject(),
            StringComparer.Ordinal);
    }

    private static IEnumerable<KeyValuePair<string, JsonObject>> BindingsFromStatement(StatementSyntax statement)
    {
        if (statement is LocalDeclarationStatementSyntax local)
        {
            foreach (var variable in local.Declaration.Variables)
            {
                if (variable.Initializer is null) continue;
                yield return new KeyValuePair<string, JsonObject>(
                    variable.Identifier.Text,
                    LiftTerm(variable.Initializer.Value));
            }
        }

        if (statement is ExpressionStatementSyntax
            {
                Expression: AssignmentExpressionSyntax assignment
            }
            && assignment.Left is IdentifierNameSyntax id)
        {
            yield return new KeyValuePair<string, JsonObject>(
                id.Identifier.Text,
                LiftTerm(assignment.Right));
        }
    }

    private static FunctionInfo? ResolveCallee(
        InvocationExpressionSyntax invocation,
        IReadOnlyList<FunctionInfo> functions,
        SemanticModel model)
    {
        var symbolInfo = model.GetSymbolInfo(invocation);
        var methodSymbol = symbolInfo.Symbol as IMethodSymbol
            ?? symbolInfo.CandidateSymbols.OfType<IMethodSymbol>().FirstOrDefault();
        if (methodSymbol is not null)
        {
            var resolved = functions.FirstOrDefault(function =>
                SymbolEqualityComparer.Default.Equals(function.Symbol, methodSymbol)
                || SymbolEqualityComparer.Default.Equals(function.Symbol.OriginalDefinition, methodSymbol.OriginalDefinition));
            if (resolved is not null) return resolved;
        }

        var calleeName = SimpleCalleeName(invocation.Expression);
        return calleeName is null
            ? null
            : functions.FirstOrDefault(function => function.Method.Identifier.Text == calleeName);
    }

    private static JsonObject GuardFormula(JsonObject consequent, IReadOnlyList<JsonObject> conditions)
    {
        if (conditions.Count == 0) return consequent.DeepClone().AsObject();
        return ImpliesFormula(
            AndFormula(conditions.Select(c => c.DeepClone().AsObject()).ToList()),
            consequent.DeepClone().AsObject());
    }

    private static JsonObject GuardedImplicationFormula(JsonObject consequent, IReadOnlyList<JsonObject> conditions)
    {
        var antecedent = conditions.Count == 0
            ? TrueFormula()
            : AndFormula(conditions.Select(c => c.DeepClone().AsObject()).ToList());
        return ImpliesFormula(antecedent, consequent.DeepClone().AsObject());
    }

    private static JsonObject UniversalCallsiteFormula(FunctionInfo caller, JsonObject guarded)
    {
        JsonObject body = guarded.DeepClone().AsObject();
        for (var i = caller.Formals.Count - 1; i >= 0; i--)
        {
            body = ForAllFormula(
                caller.Formals[i],
                caller.FormalSorts[i].DeepClone().AsObject(),
                body);
        }
        return body;
    }

    private static JsonObject MergePreconditions(JsonObject ownPre, IReadOnlyList<JsonObject> obligations)
    {
        var operands = new List<JsonObject>();
        if (!IsTrueFormula(ownPre))
            operands.Add(ownPre.DeepClone().AsObject());
        operands.AddRange(obligations.Select(o => o.DeepClone().AsObject()).Where(o => !IsTrueFormula(o)));
        return AndFormula(operands);
    }

    private static JsonObject LiftPredicate(ExpressionSyntax expression)
    {
        expression = Unwrap(expression);

        if (expression is BinaryExpressionSyntax binary)
        {
            var comparison = binary.Kind() switch
            {
                SyntaxKind.EqualsExpression => "=",
                SyntaxKind.NotEqualsExpression => "≠",
                SyntaxKind.GreaterThanExpression => ">",
                SyntaxKind.GreaterThanOrEqualExpression => "≥",
                SyntaxKind.LessThanExpression => "<",
                SyntaxKind.LessThanOrEqualExpression => "≤",
                _ => null,
            };
            if (comparison is not null)
                return AtomicFormula(comparison, LiftTerm(binary.Left), LiftTerm(binary.Right));

            if (binary.IsKind(SyntaxKind.LogicalAndExpression))
                return AndFormula(new[] { LiftPredicate(binary.Left), LiftPredicate(binary.Right) });

            if (binary.IsKind(SyntaxKind.LogicalOrExpression))
                return OrFormula(new[] { LiftPredicate(binary.Left), LiftPredicate(binary.Right) });
        }

        if (expression is PrefixUnaryExpressionSyntax prefix
            && prefix.IsKind(SyntaxKind.LogicalNotExpression))
        {
            return NegateFormula(LiftPredicate(prefix.Operand));
        }

        return AtomicFormula("=", LiftTerm(expression), BoolConst(true));
    }

    private static JsonObject LiftNegatedPredicate(ExpressionSyntax expression)
    {
        expression = Unwrap(expression);
        if (expression is BinaryExpressionSyntax binary)
        {
            var inverse = binary.Kind() switch
            {
                SyntaxKind.EqualsExpression => "≠",
                SyntaxKind.NotEqualsExpression => "=",
                SyntaxKind.GreaterThanExpression => "≤",
                SyntaxKind.GreaterThanOrEqualExpression => "<",
                SyntaxKind.LessThanExpression => "≥",
                SyntaxKind.LessThanOrEqualExpression => ">",
                _ => null,
            };
            if (inverse is not null)
                return AtomicFormula(inverse, LiftTerm(binary.Left), LiftTerm(binary.Right));
        }

        if (expression is PrefixUnaryExpressionSyntax prefix
            && prefix.IsKind(SyntaxKind.LogicalNotExpression))
        {
            return LiftPredicate(prefix.Operand);
        }

        return NegateFormula(LiftPredicate(expression));
    }

    private static JsonObject LiftTerm(ExpressionSyntax expression)
    {
        expression = Unwrap(expression);
        switch (expression)
        {
            case LiteralExpressionSyntax literal:
                return literal.Token.Value switch
                {
                    int i => IntConst(i),
                    long l => IntConst(l),
                    uint u => IntConst(u),
                    ulong u => IntConst(unchecked((long)u)),
                    bool b => BoolConst(b),
                    string s => StrConst(s),
                    null => NullConst(),
                    _ => StrConst(literal.Token.ValueText),
                };

            case IdentifierNameSyntax id:
                return VarTerm(id.Identifier.Text);

            case ThisExpressionSyntax:
                return VarTerm("this");

            case BinaryExpressionSyntax binary:
            {
                var op = binary.Kind() switch
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
                    _ => "csharp:expr",
                };
                return Ctor(op, LiftTerm(binary.Left), LiftTerm(binary.Right));
            }

            case PrefixUnaryExpressionSyntax prefix:
            {
                if (prefix.IsKind(SyntaxKind.UnaryMinusExpression)
                    && LiftTerm(prefix.Operand) is { } operand
                    && operand["kind"]?.GetValue<string>() == "const"
                    && operand["value"] is JsonValue value
                    && value.TryGetValue<long>(out var longValue))
                {
                    return IntConst(-longValue);
                }

                var op = prefix.Kind() switch
                {
                    SyntaxKind.UnaryMinusExpression => "csharp:neg",
                    SyntaxKind.LogicalNotExpression => "csharp:not",
                    SyntaxKind.BitwiseNotExpression => "csharp:bitnot",
                    _ => "csharp:prefix",
                };
                return Ctor(op, LiftTerm(prefix.Operand));
            }

            case MemberAccessExpressionSyntax member:
                return Ctor(
                    "csharp:member",
                    LiftTerm(member.Expression),
                    StrConst(member.Name.Identifier.Text));

            case ElementAccessExpressionSyntax element:
                return Ctor(
                    "csharp:index",
                    LiftTerm(element.Expression),
                    LiftTerm(element.ArgumentList.Arguments[0].Expression));

            case CastExpressionSyntax cast:
                return Ctor(
                    "csharp:cast",
                    StrConst(cast.Type.ToString()),
                    LiftTerm(cast.Expression));

            case InvocationExpressionSyntax invocation:
            {
                var args = invocation.ArgumentList.Arguments
                    .Select(argument => LiftTerm(argument.Expression))
                    .ToList();
                var calleeName = SimpleCalleeName(invocation.Expression) ?? invocation.Expression.ToString();
                var callArgs = new List<JsonObject> { StrConst(calleeName) };
                callArgs.AddRange(args);
                return Ctor("csharp:call", callArgs.ToArray());
            }

            case DefaultExpressionSyntax:
                return NullConst();

            default:
                return Ctor("csharp:expr", StrConst(expression.ToString()));
        }
    }

    private static ExpressionSyntax Unwrap(ExpressionSyntax expression)
    {
        while (expression is ParenthesizedExpressionSyntax parenthesized)
            expression = parenthesized.Expression;
        return expression;
    }

    private static string? SimpleCalleeName(ExpressionSyntax expression) => expression switch
    {
        IdentifierNameSyntax id => id.Identifier.Text,
        MemberAccessExpressionSyntax member => member.Name.Identifier.Text,
        _ => null,
    };

    private static JsonObject SubstituteFormula(
        JsonObject formula,
        IReadOnlyDictionary<string, JsonObject> substitutions)
    {
        var kind = formula["kind"]?.GetValue<string>();
        return kind switch
        {
            "atomic" => new JsonObject
            {
                ["kind"] = "atomic",
                ["name"] = formula["name"]?.DeepClone(),
                ["args"] = JsonSerializer.SerializeToNode(
                    formula["args"]?.AsArray()
                        .Select(arg => SubstituteTerm(arg!.AsObject(), substitutions))
                        .ToList() ?? new List<JsonObject>()),
            },
            "and" or "or" or "not" or "implies" => new JsonObject
            {
                ["kind"] = kind,
                ["operands"] = JsonSerializer.SerializeToNode(
                    formula["operands"]?.AsArray()
                        .Select(operand => SubstituteFormula(operand!.AsObject(), substitutions))
                        .ToList() ?? new List<JsonObject>()),
            },
            "forall" or "exists" => SubstituteQuantifier(formula, substitutions),
            _ => formula.DeepClone().AsObject(),
        };
    }

    private static JsonObject SubstituteQuantifier(
        JsonObject formula,
        IReadOnlyDictionary<string, JsonObject> substitutions)
    {
        var bound = formula["name"]?.GetValue<string>() ?? "";
        var childSubstitutions = substitutions
            .Where(pair => pair.Key != bound)
            .ToDictionary(
                pair => pair.Key,
                pair => pair.Value.DeepClone().AsObject(),
                StringComparer.Ordinal);

        var body = formula["body"]!.AsObject().DeepClone().AsObject();
        if (childSubstitutions.Values.Any(term => TermContainsVar(term, bound)))
        {
            var fresh = FreshName(bound, formula, childSubstitutions.Values);
            body = RenameFormulaVar(body, bound, fresh);
            bound = fresh;
        }

        return new JsonObject
        {
            ["kind"] = formula["kind"]?.DeepClone(),
            ["name"] = bound,
            ["sort"] = formula["sort"]?.DeepClone(),
            ["body"] = SubstituteFormula(body, childSubstitutions),
        };
    }

    private static JsonObject SubstituteTerm(
        JsonObject term,
        IReadOnlyDictionary<string, JsonObject> substitutions)
    {
        var kind = term["kind"]?.GetValue<string>();
        switch (kind)
        {
            case "var":
            {
                var name = term["name"]?.GetValue<string>() ?? "";
                return substitutions.TryGetValue(name, out var replacement)
                    ? replacement.DeepClone().AsObject()
                    : term.DeepClone().AsObject();
            }
            case "ctor":
                return new JsonObject
                {
                    ["kind"] = "ctor",
                    ["name"] = term["name"]?.DeepClone(),
                    ["args"] = JsonSerializer.SerializeToNode(
                        term["args"]?.AsArray()
                            .Select(arg => SubstituteTerm(arg!.AsObject(), substitutions))
                            .ToList() ?? new List<JsonObject>()),
                };
            default:
                return term.DeepClone().AsObject();
        }
    }

    private static JsonObject RenameFormulaVar(JsonObject formula, string from, string to)
    {
        var kind = formula["kind"]?.GetValue<string>();
        return kind switch
        {
            "atomic" => new JsonObject
            {
                ["kind"] = "atomic",
                ["name"] = formula["name"]?.DeepClone(),
                ["args"] = JsonSerializer.SerializeToNode(
                    formula["args"]?.AsArray()
                        .Select(arg => RenameTermVar(arg!.AsObject(), from, to))
                        .ToList() ?? new List<JsonObject>()),
            },
            "and" or "or" or "not" or "implies" => new JsonObject
            {
                ["kind"] = kind,
                ["operands"] = JsonSerializer.SerializeToNode(
                    formula["operands"]?.AsArray()
                        .Select(operand => RenameFormulaVar(operand!.AsObject(), from, to))
                        .ToList() ?? new List<JsonObject>()),
            },
            "forall" or "exists" when formula["name"]?.GetValue<string>() == from => formula.DeepClone().AsObject(),
            "forall" or "exists" => new JsonObject
            {
                ["kind"] = formula["kind"]?.DeepClone(),
                ["name"] = formula["name"]?.DeepClone(),
                ["sort"] = formula["sort"]?.DeepClone(),
                ["body"] = RenameFormulaVar(formula["body"]!.AsObject(), from, to),
            },
            _ => formula.DeepClone().AsObject(),
        };
    }

    private static JsonObject RenameTermVar(JsonObject term, string from, string to)
    {
        var kind = term["kind"]?.GetValue<string>();
        return kind switch
        {
            "var" when term["name"]?.GetValue<string>() == from => VarTerm(to),
            "ctor" => new JsonObject
            {
                ["kind"] = "ctor",
                ["name"] = term["name"]?.DeepClone(),
                ["args"] = JsonSerializer.SerializeToNode(
                    term["args"]?.AsArray()
                        .Select(arg => RenameTermVar(arg!.AsObject(), from, to))
                        .ToList() ?? new List<JsonObject>()),
            },
            _ => term.DeepClone().AsObject(),
        };
    }

    private static bool TermContainsVar(JsonObject term, string name)
    {
        var kind = term["kind"]?.GetValue<string>();
        return kind switch
        {
            "var" => term["name"]?.GetValue<string>() == name,
            "ctor" => term["args"]?.AsArray().Any(arg => TermContainsVar(arg!.AsObject(), name)) == true,
            _ => false,
        };
    }

    private static string FreshName(
        string baseName,
        JsonObject formula,
        IEnumerable<JsonObject> replacementTerms)
    {
        var used = new HashSet<string>(StringComparer.Ordinal);
        CollectFormulaVars(formula, used);
        foreach (var term in replacementTerms)
            CollectTermVars(term, used);

        var i = 0;
        string candidate;
        do
        {
            candidate = $"{baseName}__subst{i++}";
        }
        while (used.Contains(candidate));
        return candidate;
    }

    private static void CollectFormulaVars(JsonObject formula, HashSet<string> used)
    {
        var kind = formula["kind"]?.GetValue<string>();
        if (kind == "atomic")
        {
            foreach (var arg in formula["args"]?.AsArray() ?? new JsonArray())
                if (arg is JsonObject term) CollectTermVars(term, used);
            return;
        }

        if (kind is "and" or "or" or "not" or "implies")
        {
            foreach (var operand in formula["operands"]?.AsArray() ?? new JsonArray())
                if (operand is JsonObject child) CollectFormulaVars(child, used);
            return;
        }

        if (kind is "forall" or "exists")
        {
            if (formula["name"] is not null)
                used.Add(formula["name"]!.GetValue<string>());
            if (formula["body"] is JsonObject body)
                CollectFormulaVars(body, used);
        }
    }

    private static void CollectTermVars(JsonObject term, HashSet<string> used)
    {
        if (term["kind"]?.GetValue<string>() == "var"
            && term["name"] is not null)
        {
            used.Add(term["name"]!.GetValue<string>());
            return;
        }

        if (term["kind"]?.GetValue<string>() == "ctor")
        {
            foreach (var arg in term["args"]?.AsArray() ?? new JsonArray())
                if (arg is JsonObject child) CollectTermVars(child, used);
        }
    }

    private static bool IsTrueFormula(JsonObject formula)
    {
        return formula["kind"]?.GetValue<string>() == "atomic"
               && formula["name"]?.GetValue<string>() == "true"
               && (formula["args"] as JsonArray)?.Count == 0;
    }

    private static JsonObject TrueFormula() => new()
    {
        ["kind"] = "atomic",
        ["name"] = "true",
        ["args"] = new JsonArray(),
    };

    private static JsonObject AtomicFormula(string name, params JsonObject[] args) => new()
    {
        ["kind"] = "atomic",
        ["name"] = name,
        ["args"] = JsonSerializer.SerializeToNode(args.ToList()),
    };

    private static JsonObject AndFormula(IEnumerable<JsonObject> operands)
    {
        var list = operands.ToList();
        return list.Count switch
        {
            0 => TrueFormula(),
            1 => list[0].DeepClone().AsObject(),
            _ => new JsonObject
            {
                ["kind"] = "and",
                ["operands"] = JsonSerializer.SerializeToNode(list),
            },
        };
    }

    private static JsonObject OrFormula(IEnumerable<JsonObject> operands)
    {
        var list = operands.ToList();
        return list.Count switch
        {
            0 => TrueFormula(),
            1 => list[0].DeepClone().AsObject(),
            _ => new JsonObject
            {
                ["kind"] = "or",
                ["operands"] = JsonSerializer.SerializeToNode(list),
            },
        };
    }

    private static JsonObject NegateFormula(JsonObject operand) => new()
    {
        ["kind"] = "not",
        ["operands"] = JsonSerializer.SerializeToNode(new[] { operand.DeepClone().AsObject() }),
    };

    private static JsonObject ImpliesFormula(JsonObject antecedent, JsonObject consequent) => new()
    {
        ["kind"] = "implies",
        ["operands"] = JsonSerializer.SerializeToNode(new[]
        {
            antecedent.DeepClone().AsObject(),
            consequent.DeepClone().AsObject(),
        }),
    };

    private static JsonObject ForAllFormula(string name, JsonObject sort, JsonObject body) => new()
    {
        ["kind"] = "forall",
        ["name"] = name,
        ["sort"] = sort,
        ["body"] = body,
    };

    private static JsonObject Ctor(string name, params JsonObject[] args) => new()
    {
        ["kind"] = "ctor",
        ["name"] = name,
        ["args"] = JsonSerializer.SerializeToNode(args.ToList()),
    };

    private static JsonObject VarTerm(string name) => new()
    {
        ["kind"] = "var",
        ["name"] = name,
    };

    private static JsonObject IntConst(long value) => new()
    {
        ["kind"] = "const",
        ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("Int"),
    };

    private static JsonObject BoolConst(bool value) => new()
    {
        ["kind"] = "const",
        ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("Bool"),
    };

    private static JsonObject StrConst(string value) => new()
    {
        ["kind"] = "const",
        ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("String"),
    };

    private static JsonObject NullConst() => new()
    {
        ["kind"] = "const",
        ["value"] = null,
        ["sort"] = PrimSort("Ref"),
    };

    private static JsonObject PrimSort(string name) => new()
    {
        ["kind"] = "primitive",
        ["name"] = name,
    };
}
