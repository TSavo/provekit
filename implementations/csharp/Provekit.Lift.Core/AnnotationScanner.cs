// SPDX-License-Identifier: Apache-2.0
//
// Provekit.Lift.Core.AnnotationScanner — shared //provekit: annotation
// scanner for the C# kit. Used by both Provekit.Lsp.Plugin (live editing)
// and any future batch-CLI lift binary that consumes annotated source.
//
// Recognized annotations:
//   //provekit:contract                  — declares the next function as a contract
//   //provekit:implement <cid>           — declares the next function as an
//                                          implementation bridge to <cid>

using System.Text.RegularExpressions;
using Provekit.IR;

namespace Provekit.Lift.Core;

public static partial class AnnotationScanner
{
    [GeneratedRegex(@"//\s*provekit:\s*contract")]
    private static partial Regex ContractAnnotation();

    [GeneratedRegex(@"//\s*provekit:\s*implement\s+([\w-]+)")]
    private static partial Regex ImplementAnnotation();

    [GeneratedRegex(@"(?:public|private|protected|internal|static)\s+\w+(?:\<[^>]*\>)?\s+(\w+)\s*\(")]
    private static partial Regex FunctionSig();

    /// <summary>
    /// Scan C# source for //provekit:contract and //provekit:implement
    /// annotations. Each annotation attaches to the first function-shaped
    /// signature within 10 lines below it.
    /// </summary>
    public static List<ContractDecl> ScanAnnotations(string source)
    {
        var decls = new List<ContractDecl>();
        var lines = source.Split('\n');
        for (int i = 0; i < lines.Length; i++)
        {
            if (ContractAnnotation().IsMatch(lines[i]))
            {
                var fn = FindFn(lines, i);
                if (fn.Length > 0)
                    decls.Add(new ContractDecl(fn, null, Predicates.And(), null, "out"));
            }
            if (ImplementAnnotation().Match(lines[i]) is { Success: true } m)
            {
                var cid = m.Groups[1].Value;
                var fn = FindFn(lines, i);
                if (fn.Length > 0)
                    decls.Add(new ContractDecl($"{fn}→{cid}", null, Predicates.And(), null, "out"));
            }
        }
        return decls;
    }

    private static string FindFn(string[] lines, int start)
    {
        for (int j = start + 1; j < lines.Length && j < start + 10; j++)
        {
            var m = FunctionSig().Match(lines[j]);
            if (m.Success) return m.Groups[1].Value;
        }
        return "";
    }
}
