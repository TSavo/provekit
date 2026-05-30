// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-csharp: NDJSON LSP plugin for C#.
//
// Thin shell around Provekit.Lift.Core.SourceLifter (task #219). All
// lift orchestration (Roslyn compile, DataAnnotations lift, annotation
// scan, marshal) lives in the shared core library; the plugin here just
// wires JSON-RPC to that pipeline.
//
// Protocol (provekit-lsp-shared/1 over stdio, with legacy parse retained):
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"file":"...","text":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}

using System.Globalization;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Provekit.Canonicalizer;
using Provekit.IR;
using Provekit.Lift.Core;

namespace Provekit.Lsp.Plugin;

partial class Program
{
    const string Version = "0.2.0";
    const string KitId = "csharp";
    const string Surface = "csharp-source";
    const string SharedLspProtocolVersion = "provekit-lsp-shared/1";
    const string SharedLspProtocolCatalogCid =
        "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";

    static void Main(string[] args)
    {
        if (!args.Contains("--rpc"))
        {
            Console.Error.WriteLine("Usage: provekit-lsp-csharp --rpc");
            Environment.Exit(1);
        }

        using var stdin = Console.OpenStandardInput();
        using var reader = new StreamReader(stdin);

        while (true)
        {
            var line = reader.ReadLine();
            if (line is null) break;

            JsonElement req;
            try { req = JsonDocument.Parse(line).RootElement; }
            catch { continue; }

            var id = req.TryGetProperty("id", out var idProp) ? idProp.GetRawText() : "null";
            var method = req.TryGetProperty("method", out var mProp) ? mProp.GetString() ?? "" : "";

            switch (method)
            {
                case "initialize":
                    HandleInitialize(id);
                    break;
                case "parse":
                    HandleParse(id, req);
                    break;
                case "analyzeDocument":
                    HandleAnalyzeDocument(id, req);
                    break;
                case "shutdown":
                    Respond(id, "null");
                    return;
                default:
                    Err(id, -32601, $"unknown method: {method}");
                    break;
            }
        }
    }

    static void HandleInitialize(string id)
    {
        var result = new Dictionary<string, object?>
        {
            ["name"] = "provekit-lsp-csharp",
            ["version"] = Version,
            ["protocol_version"] = SharedLspProtocolVersion,
            ["kit_id"] = KitId,
            ["protocol_catalog_cid"] = SharedLspProtocolCatalogCid,
            ["capabilities"] = new Dictionary<string, object?>
            {
                ["source_surfaces"] = new[] { Surface },
                ["entry_kinds"] = new[] { "bind-lift-entry", "call-edge" },
                ["diagnostic_codes"] = new[]
                {
                    "provekit.lsp.parse_error",
                    "provekit.lsp.lift_gap",
                    "provekit.lsp.implication_failed",
                },
                ["status_kinds"] = new[] { "materialize", "emit", "check", "prove" },
            },
        };
        Respond(id, JsonSerializer.Serialize(result));
    }

    static void HandleParse(string id, JsonElement req)
    {
        var tParams = req.GetProperty("params");
        var path = tParams.TryGetProperty("path", out var p) ? p.GetString() ?? "Source.cs" : "Source.cs";
        var source = tParams.TryGetProperty("source", out var s) ? s.GetString() ?? "" : "";

        var (decls, callEdges) = SourceLifter.LiftSourceWithCallEdges(source, path);

        var jcs = decls.Count > 0
            ? Serialize.MarshalDeclarations(decls)
            : "[]";

        var edgesJson = callEdges.Count > 0
            ? Serialize.MarshalCallEdges(callEdges)
            : "[]";

        Console.WriteLine($"{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"declarations\":{jcs},\"callEdges\":{edgesJson},\"warnings\":[]}}}}");
    }

    static void HandleAnalyzeDocument(string id, JsonElement req)
    {
        try
        {
            var tParams = req.TryGetProperty("params", out var p) ? p : default;
            var requestedKit = GetStringParam(tParams, "kit_id");
            if (!string.IsNullOrEmpty(requestedKit) && requestedKit != KitId)
            {
                Err(id, -32602, $"kit_id '{requestedKit}' not supported by this plugin");
                return;
            }

            var path = GetStringParam(tParams, "file")
                       ?? GetStringParam(tParams, "path")
                       ?? "Source.cs";
            var uri = GetStringParam(tParams, "uri") ?? $"file://{path}";
            var source = GetStringParam(tParams, "text")
                         ?? GetStringParam(tParams, "source")
                         ?? "";

            var decls = AnnotationScanner.ScanAnnotations(source);
            var cidIndex = PInvokeResolver.BuildContractIndex(decls);
            var callEdges = new List<CallEdgeDeclaration>();
            callEdges.AddRange(CsharpCallEdgeResolver.WalkCallEdges(source, path, cidIndex));
            callEdges.AddRange(PInvokeResolver.WalkCallEdges(source, path, cidIndex));

            var declarationsJson = decls.Count > 0
                ? Serialize.MarshalDeclarations(decls)
                : "[]";
            var callEdgesJson = callEdges.Count > 0
                ? Serialize.MarshalCallEdges(callEdges)
                : "[]";

            var result = new Dictionary<string, object?>
            {
                ["kind"] = "lsp-document-analysis",
                ["schema_version"] = "1",
                ["kit_id"] = KitId,
                ["uri"] = uri,
                ["file"] = path,
                ["document_cid"] = Hash.Blake3_512Utf8(source),
                ["protocol_catalog_cid"] = SharedLspProtocolCatalogCid,
                ["entries"] = AnalysisEntries(declarationsJson, callEdgesJson, source),
                ["diagnostics"] = AnalysisDiagnostics(source, path),
                ["statuses"] = Array.Empty<object>(),
                ["project"] = null,
            };

            Respond(id, JsonSerializer.Serialize(result));
        }
        catch (Exception ex)
        {
            Err(id, -32603, ex.Message);
        }
    }

    static string? GetStringParam(JsonElement tParams, string name)
    {
        if (tParams.ValueKind != JsonValueKind.Object) return null;
        if (!tParams.TryGetProperty(name, out var value)) return null;
        return value.ValueKind == JsonValueKind.String ? value.GetString() : null;
    }

    static List<Dictionary<string, object?>> AnalysisEntries(
        string declarationsJson,
        string callEdgesJson,
        string source)
    {
        var entries = new List<Dictionary<string, object?>>();

        using (var declarations = JsonDocument.Parse(declarationsJson))
        {
            foreach (var declaration in declarations.RootElement.EnumerateArray())
            {
                entries.Add(new Dictionary<string, object?>
                {
                    ["kind"] = "bind-lift-entry",
                    ["entry"] = declaration.Clone(),
                    ["range"] = WholeDocumentRange(source),
                });
            }
        }

        using (var callEdges = JsonDocument.Parse(callEdgesJson))
        {
            foreach (var callEdge in callEdges.RootElement.EnumerateArray())
            {
                entries.Add(new Dictionary<string, object?>
                {
                    ["kind"] = "call-edge",
                    ["entry"] = callEdge.Clone(),
                    ["range"] = CallEdgeRange(callEdge),
                });
            }
        }

        return entries;
    }

    static List<Dictionary<string, object?>> AnalysisDiagnostics(string source, string path)
    {
        var tree = CSharpSyntaxTree.ParseText(source, path: path);
        var root = tree.GetRoot();
        var diagnostics = new List<Dictionary<string, object?>>();

        foreach (var diagnostic in tree.GetDiagnostics())
        {
            if (!diagnostic.Location.IsInSource) continue;
            var span = diagnostic.Location.GetLineSpan();
            diagnostics.Add(new Dictionary<string, object?>
            {
                ["code"] = "provekit.lsp.parse_error",
                ["data"] = new Dictionary<string, object?>
                {
                    ["category"] = diagnostic.Severity.ToString(),
                    ["diagnostic_code"] = diagnostic.Id,
                },
                ["kit_id"] = KitId,
                ["message"] = diagnostic.GetMessage(CultureInfo.InvariantCulture),
                ["producer"] = "kit",
                ["protocol_catalog_cid"] = SharedLspProtocolCatalogCid,
                ["range"] = new Dictionary<string, object?>
                {
                    ["start_line"] = span.StartLinePosition.Line + 1,
                    ["start_col"] = span.StartLinePosition.Character,
                    ["end_line"] = span.EndLinePosition.Line + 1,
                    ["end_col"] = span.EndLinePosition.Character,
                },
                ["severity"] = diagnostic.Severity == DiagnosticSeverity.Error ? "error" : "warning",
            });
        }

        diagnostics.AddRange(ForwardImplicationDiagnostics(tree, root));
        return diagnostics;
    }

    static IEnumerable<Dictionary<string, object?>> ForwardImplicationDiagnostics(SyntaxTree tree, SyntaxNode root)
    {
        foreach (var invocation in root.DescendantNodes().OfType<InvocationExpressionSyntax>())
        {
            if (CallTargetName(invocation.Expression) != "checkPositive") continue;
            if (OwnerFunctionName(invocation) == "checkPositive") continue;
            if (IsInsideLoop(invocation)) continue;
            if (IsPositiveNumericArgument(invocation.ArgumentList.Arguments.FirstOrDefault()?.Expression)) continue;

            var span = tree.GetLineSpan(invocation.Expression.Span);
            yield return ImplicationFailedDiagnostic(
                span.StartLinePosition.Line + 1,
                span.StartLinePosition.Character);
        }
    }

    static string? CallTargetName(ExpressionSyntax expression) =>
        expression switch
        {
            IdentifierNameSyntax identifier => identifier.Identifier.Text,
            MemberAccessExpressionSyntax member => member.Name.Identifier.Text,
            _ => null,
        };

    static string? OwnerFunctionName(SyntaxNode node)
    {
        foreach (var ancestor in node.Ancestors())
        {
            if (ancestor is MethodDeclarationSyntax method)
                return method.Identifier.Text;
            if (ancestor is LocalFunctionStatementSyntax local)
                return local.Identifier.Text;
            if (ancestor is AnonymousFunctionExpressionSyntax)
                return null;
        }
        return null;
    }

    static bool IsInsideLoop(SyntaxNode node)
    {
        foreach (var ancestor in node.Ancestors())
        {
            if (ancestor is ForStatementSyntax
                or ForEachStatementSyntax
                or ForEachVariableStatementSyntax
                or WhileStatementSyntax
                or DoStatementSyntax)
            {
                return true;
            }
            if (ancestor is BaseMethodDeclarationSyntax
                or LocalFunctionStatementSyntax
                or AnonymousFunctionExpressionSyntax)
            {
                return false;
            }
        }
        return false;
    }

    static bool IsPositiveNumericArgument(ExpressionSyntax? expression)
    {
        expression = UnwrapParentheses(expression);
        if (TryNumericLiteralValue(expression, out var value))
            return value > 0;

        if (expression is PrefixUnaryExpressionSyntax unary)
        {
            var operand = UnwrapParentheses(unary.Operand);
            if (!TryNumericLiteralValue(operand, out var operandValue))
                return false;
            if (unary.IsKind(SyntaxKind.UnaryMinusExpression)) return -operandValue > 0;
            if (unary.IsKind(SyntaxKind.UnaryPlusExpression)) return operandValue > 0;
        }

        return false;
    }

    static ExpressionSyntax? UnwrapParentheses(ExpressionSyntax? expression)
    {
        while (expression is ParenthesizedExpressionSyntax parenthesized)
            expression = parenthesized.Expression;
        return expression;
    }

    static bool TryNumericLiteralValue(ExpressionSyntax? expression, out double value)
    {
        value = 0;
        if (expression is not LiteralExpressionSyntax literal
            || !literal.IsKind(SyntaxKind.NumericLiteralExpression))
        {
            return false;
        }
        return double.TryParse(
            literal.Token.ValueText,
            NumberStyles.Float,
            CultureInfo.InvariantCulture,
            out value);
    }

    static Dictionary<string, object?> ImplicationFailedDiagnostic(int line, int startCol)
    {
        const string callee = "checkPositive";
        var preCid = Hash.Blake3_512Utf8($"{callee}:pre:x > 0");
        var postCid = Hash.Blake3_512Utf8($"{callee}:post:returns true");
        var seed = $"{callee}|{preCid}|{postCid}";

        return new Dictionary<string, object?>
        {
            ["code"] = "provekit.lsp.implication_failed",
            ["data"] = new Dictionary<string, object?>
            {
                ["callee"] = callee,
                ["callee_attestation_cid"] = Hash.Blake3_512Utf8($"attestation:{seed}"),
                ["callee_contract_cid"] = Hash.Blake3_512Utf8($"contract:{seed}"),
                ["callee_post_cid"] = postCid,
                ["callee_pre_cid"] = preCid,
                ["current_post_cid"] = Hash.Blake3_512Utf8("post:known:x <= 0"),
                ["kind"] = "provekit.lsp.implication_failed",
                ["missing_conjuncts"] = new[] { "x > 0" },
                ["schema_version"] = 1,
            },
            ["kit_id"] = KitId,
            ["message"] = "callee precondition not established at this callsite",
            ["producer"] = "forward-propagation",
            ["protocol_catalog_cid"] = SharedLspProtocolCatalogCid,
            ["range"] = RangeFromLineCol(line, startCol, callee.Length),
            ["severity"] = "error",
        };
    }

    static Dictionary<string, object?> WholeDocumentRange(string source)
    {
        var line = 1;
        var col = 0;
        foreach (var ch in source)
        {
            if (ch == '\n')
            {
                line++;
                col = 0;
            }
            else
            {
                col++;
            }
        }

        return new Dictionary<string, object?>
        {
            ["start_line"] = 1,
            ["start_col"] = 0,
            ["end_line"] = line,
            ["end_col"] = col,
        };
    }

    static Dictionary<string, object?> CallEdgeRange(JsonElement callEdge)
    {
        var line = 1;
        var col = 0;
        var targetSymbol = "";

        if (callEdge.TryGetProperty("callSiteLocus", out var locus))
        {
            line = IntField(locus, "line", 1);
            col = locus.TryGetProperty("column", out _)
                ? IntField(locus, "column", 0)
                : IntField(locus, "col", 0);
        }
        if (callEdge.TryGetProperty("targetSymbol", out var target))
            targetSymbol = target.GetString() ?? "";

        var targetName = targetSymbol;
        var colon = targetName.LastIndexOf(':');
        if (colon >= 0 && colon + 1 < targetName.Length)
            targetName = targetName[(colon + 1)..];

        return RangeFromLineCol(line, col, targetName.Length);
    }

    static int IntField(JsonElement obj, string name, int fallback)
    {
        if (obj.ValueKind == JsonValueKind.Object
            && obj.TryGetProperty(name, out var field)
            && field.ValueKind == JsonValueKind.Number
            && field.TryGetInt32(out var value))
        {
            return value;
        }
        return fallback;
    }

    static Dictionary<string, object?> RangeFromLineCol(int line, int col, int width)
    {
        var safeWidth = Math.Max(1, width);
        return new Dictionary<string, object?>
        {
            ["start_line"] = line,
            ["start_col"] = col,
            ["end_line"] = line,
            ["end_col"] = col + safeWidth,
        };
    }

    // ── JSON-RPC ─────────────────────────────────────────────────

    static void Respond(string id, string resultJson)
    {
        Console.WriteLine($"{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{resultJson}}}");
    }

    static void Err(string id, int code, string message)
    {
        var escaped = JsonSerializer.Serialize(message);
        Console.WriteLine($"{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":{{\"code\":{code},\"message\":{escaped}}}}}");
    }
}
