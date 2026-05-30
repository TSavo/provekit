// SPDX-License-Identifier: Apache-2.0
//
// Provekit.Lift.Core.CsharpCallEdgeResolver: Roslyn-driven same-language
// call-edge emitter for the C# LSP parse path.

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Provekit.IR;

namespace Provekit.Lift.Core;

public static class CsharpCallEdgeResolver
{
    /// <summary>
    /// Walk <paramref name="source"/> for C# method calls where both caller
    /// and callee have lifted contracts in <paramref name="contractCids"/>.
    /// </summary>
    public static IReadOnlyList<CallEdgeDeclaration> WalkCallEdges(
        string source,
        string path,
        IReadOnlyDictionary<string, string> contractCids)
    {
        if (contractCids.Count == 0)
            return Array.Empty<CallEdgeDeclaration>();

        var tree = CSharpSyntaxTree.ParseText(source, path: path);
        var root = tree.GetRoot();
        var edges = new List<CallEdgeDeclaration>();
        var seen = new HashSet<string>(StringComparer.Ordinal);

        foreach (var method in root.DescendantNodes().OfType<MethodDeclarationSyntax>())
        {
            var callerName = method.Identifier.Text;
            if (!contractCids.TryGetValue(callerName, out var sourceCid))
                continue;

            var bodyNode = (SyntaxNode?)method.Body ?? method.ExpressionBody;
            if (bodyNode is null)
                continue;

            foreach (var invocation in bodyNode.DescendantNodes().OfType<InvocationExpressionSyntax>())
            {
                var calleeName = ExtractCalleeName(invocation.Expression);
                if (calleeName is null || calleeName == callerName)
                    continue;

                if (!contractCids.TryGetValue(calleeName, out var targetCid))
                    continue;

                var lineSpan = invocation.GetLocation().GetLineSpan();
                var line = lineSpan.StartLinePosition.Line + 1;
                var column = lineSpan.StartLinePosition.Character + 1;
                var key = $"{sourceCid}\0{targetCid}\0{line}\0{column}";
                if (!seen.Add(key))
                    continue;

                edges.Add(new CallEdgeDeclaration(
                    SourceContractCid: sourceCid,
                    TargetContractCid: targetCid,
                    TargetSymbol: $"csharp-kit:{calleeName}",
                    CallSiteLocus: new Locus(path, line, column),
                    EvidenceTerm: $"call-site-obligation({callerName})"));
            }
        }

        return edges;
    }

    private static string? ExtractCalleeName(ExpressionSyntax expression)
    {
        return expression switch
        {
            IdentifierNameSyntax id => id.Identifier.Text,
            GenericNameSyntax generic => generic.Identifier.Text,
            MemberAccessExpressionSyntax member => ExtractMemberName(member.Name),
            _ => null,
        };
    }

    private static string ExtractMemberName(SimpleNameSyntax name)
    {
        return name switch
        {
            IdentifierNameSyntax id => id.Identifier.Text,
            GenericNameSyntax generic => generic.Identifier.Text,
            _ => name.Identifier.Text,
        };
    }
}
