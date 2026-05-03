// SPDX-License-Identifier: Apache-2.0
//
// Provekit.Lift.Core.PInvokeResolver — Roslyn-driven P/Invoke call-edge
// emitter for the C# kit, per spec #114 R3.
//
// Handles:
//   Pattern A  — [DllImport("libname")] static extern T Method(...)
//   Pattern B  — [DllImport("libname", EntryPoint = "name", CallingConvention = ...)]
//   Pattern C  — [LibraryImport("libname", EntryPoint = "name")] partial T Method(...)
//
// For each P/Invoke declaration found, the resolver:
//   1. Extracts the library name and normalises it (strip lib prefix,
//      strip .so/.dll/.dylib extensions).
//   2. Maps the normalised name to a kit: rust-kit, cpp-kit,
//      libc-system, or resolver-error if unknown.
//   3. Determines the target function name: EntryPoint arg if present,
//      otherwise the C# method name.
//   4. For every call site that invokes a registered P/Invoke method,
//      emits a CallEdgeDeclaration with targetSymbol = "<kit>:<name>".
//
// Resolution algorithm mirrors the Go cgo resolver in
// implementations/go/cmd/provekit-lsp-go/main.go (resolveCgoKit).
// When the library is unresolvable, targetSymbol = "resolver-error:<name>"
// per spec #97 R2 (fail-loud; the linker promotes to linker-error memento).

using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Provekit.Canonicalizer;
using Provekit.IR;

namespace Provekit.Lift.Core;

public static class PInvokeResolver
{
    // Known well-known libraries that map to libc-system. Mirrors Go's
    // resolveCgoKit system-libs set.
    private static readonly HashSet<string> SystemLibs = new(StringComparer.OrdinalIgnoreCase)
    {
        "c", "m", "z", "pthread", "dl", "ssl", "crypto", "curl",
        "libc", "libm", "libz",
    };

    // Libraries whose normalised name starts with "rust" → rust-kit.
    // Everything else with a resolved name → cpp-kit.

    /// <summary>
    /// Walk <paramref name="source"/> for P/Invoke declarations and their
    /// call sites; return one <see cref="CallEdgeDeclaration"/> per call
    /// site where the callee is a known P/Invoke method and the calling
    /// function has a contract in <paramref name="contractCids"/>.
    ///
    /// <paramref name="contractCids"/> maps contract name → CID, as built
    /// by the upstream lifter (DataAnnotations + annotation scan).
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

        // Pass 1: collect P/Invoke method registrations.
        // Map: C# method name → (kit, entryPoint)
        var pinvokeMap = CollectPInvokeMethods(root);
        if (pinvokeMap.Count == 0)
            return Array.Empty<CallEdgeDeclaration>();

        // Pass 2: walk method bodies for call sites that invoke P/Invoke methods.
        return WalkCallSites(root, path, pinvokeMap, contractCids);
    }

    // ------------------------------------------------------------------ //
    //  Pass 1: collect [DllImport] / [LibraryImport] declarations         //
    // ------------------------------------------------------------------ //

    private sealed record PInvokeInfo(string Kit, string EntryPoint);

    private static Dictionary<string, PInvokeInfo> CollectPInvokeMethods(SyntaxNode root)
    {
        var result = new Dictionary<string, PInvokeInfo>(StringComparer.Ordinal);

        foreach (var method in root.DescendantNodes().OfType<MethodDeclarationSyntax>())
        {
            var info = TryExtractPInvoke(method);
            if (info is not null)
                result[method.Identifier.Text] = info;
        }

        return result;
    }

    private static PInvokeInfo? TryExtractPInvoke(MethodDeclarationSyntax method)
    {
        // Must be extern (Pattern A/B) or partial (Pattern C).
        bool isExtern = method.Modifiers.Any(m => m.IsKind(SyntaxKind.ExternKeyword));
        bool isPartial = method.Modifiers.Any(m => m.IsKind(SyntaxKind.PartialKeyword));
        if (!isExtern && !isPartial)
            return null;

        foreach (var attrList in method.AttributeLists)
        {
            foreach (var attr in attrList.Attributes)
            {
                var attrName = attr.Name.ToString();
                bool isDllImport = attrName is "DllImport" or "System.Runtime.InteropServices.DllImport";
                bool isLibraryImport = attrName is "LibraryImport" or "System.Runtime.InteropServices.LibraryImport";

                if (!isDllImport && !isLibraryImport)
                    continue;

                // First positional argument is the library name.
                if (attr.ArgumentList is null || attr.ArgumentList.Arguments.Count == 0)
                    continue;

                var firstArg = attr.ArgumentList.Arguments[0];
                var rawLib = ExtractStringLiteral(firstArg.Expression);
                if (rawLib is null)
                    continue;

                var normLib = NormaliseLibName(rawLib);
                var kit = ResolveKit(normLib);

                // Optional EntryPoint named argument.
                string? entryPoint = null;
                foreach (var arg in attr.ArgumentList.Arguments.Skip(1))
                {
                    if (arg.NameEquals?.Name.Identifier.Text == "EntryPoint")
                    {
                        entryPoint = ExtractStringLiteral(arg.Expression);
                        break;
                    }
                }

                var targetName = entryPoint ?? method.Identifier.Text;
                return new PInvokeInfo(kit, targetName);
            }
        }

        return null;
    }

    // ------------------------------------------------------------------ //
    //  Pass 2: walk call sites                                             //
    // ------------------------------------------------------------------ //

    private static IReadOnlyList<CallEdgeDeclaration> WalkCallSites(
        SyntaxNode root,
        string path,
        IReadOnlyDictionary<string, PInvokeInfo> pinvokeMap,
        IReadOnlyDictionary<string, string> contractCids)
    {
        var edges = new List<CallEdgeDeclaration>();

        foreach (var method in root.DescendantNodes().OfType<MethodDeclarationSyntax>())
        {
            var callerName = method.Identifier.Text;
            if (!contractCids.TryGetValue(callerName, out var sourceCid))
                continue; // caller has no contract; skip per R1

            if (method.Body is null && method.ExpressionBody is null)
                continue; // abstract / extern caller; skip

            var bodyNode = (SyntaxNode?)method.Body ?? method.ExpressionBody;
            foreach (var invocation in bodyNode!.DescendantNodes().OfType<InvocationExpressionSyntax>())
            {
                var calleeName = ExtractCalleeName(invocation.Expression);
                if (calleeName is null)
                    continue;

                if (!pinvokeMap.TryGetValue(calleeName, out var info))
                    continue;

                var lineSpan = invocation.GetLocation().GetLineSpan();
                var locus = new Locus(
                    File: path,
                    Line: lineSpan.StartLinePosition.Line + 1,
                    Column: lineSpan.StartLinePosition.Character + 1);

                var targetSymbol = info.Kit != "" && !info.Kit.StartsWith("resolver-error")
                    ? $"{info.Kit}:{info.EntryPoint}"
                    : $"resolver-error:{info.EntryPoint}";

                // Placeholder evidence term (structural obligation; linker
                // resolves the actual post ⊃ pre obligation per R2).
                var evidence = $"call-site-obligation({callerName})";

                edges.Add(new CallEdgeDeclaration(
                    SourceContractCid: sourceCid,
                    TargetContractCid: null,  // cross-kit; always null for P/Invoke
                    TargetSymbol: targetSymbol,
                    CallSiteLocus: locus,
                    EvidenceTerm: evidence));
            }
        }

        return edges;
    }

    // ------------------------------------------------------------------ //
    //  Kit resolution                                                      //
    // ------------------------------------------------------------------ //

    /// <summary>
    /// Map a normalised library name to a ProvekIt kit label.
    /// Resolution order (first match wins):
    ///   1. Name starts with "rust" → rust-kit.
    ///   2. Name is in the system-libs set → libc-system.
    ///   3. Non-empty name → cpp-kit (generic native library).
    ///   4. Empty name → "" (resolver-error; caller wraps the entry point).
    /// </summary>
    public static string ResolveKit(string normalisedLib)
    {
        if (normalisedLib.Length == 0)
            return "";

        if (normalisedLib.StartsWith("rust", StringComparison.OrdinalIgnoreCase))
            return "rust-kit";

        if (SystemLibs.Contains(normalisedLib))
            return "libc-system";

        return "cpp-kit";
    }

    /// <summary>
    /// Normalise a library name:
    ///   1. Strip path components (basename only).
    ///   2. Strip a leading "lib" prefix (case-insensitive).
    ///   3. Strip known extensions: .so, .dll, .dylib (and versioned
    ///      variants like .so.6, .dylib.1).
    /// </summary>
    public static string NormaliseLibName(string raw)
    {
        // Basename only.
        var name = System.IO.Path.GetFileName(raw);

        // Strip versioned extension suffixes first (.so.6, .dylib.1, etc.)
        // then the primary extension.
        while (true)
        {
            var ext = System.IO.Path.GetExtension(name).ToLowerInvariant();
            if (ext is ".so" or ".dll" or ".dylib")
            {
                name = System.IO.Path.GetFileNameWithoutExtension(name);
            }
            else if (int.TryParse(ext.TrimStart('.'), out _) && name.Contains('.'))
            {
                // versioned number suffix e.g. ".6"
                name = System.IO.Path.GetFileNameWithoutExtension(name);
            }
            else
            {
                break;
            }
        }

        // Strip leading "lib" prefix (case-insensitive).
        if (name.StartsWith("lib", StringComparison.OrdinalIgnoreCase) && name.Length > 3)
            name = name[3..];

        return name;
    }

    // ------------------------------------------------------------------ //
    //  Helpers                                                             //
    // ------------------------------------------------------------------ //

    /// <summary>
    /// Extract a string literal value from a syntax expression.
    /// Handles both verbatim (@"...") and regular ("...") string literals.
    /// Returns null for non-literal expressions.
    /// </summary>
    private static string? ExtractStringLiteral(ExpressionSyntax expr)
    {
        if (expr is LiteralExpressionSyntax lit &&
            lit.IsKind(SyntaxKind.StringLiteralExpression))
        {
            return lit.Token.ValueText;
        }
        return null;
    }

    /// <summary>
    /// Extract the simple callee name from an invocation's expression.
    /// Returns null for complex expressions (method calls, lambdas, etc.)
    /// that can't be mapped to a P/Invoke declaration by name.
    /// </summary>
    private static string? ExtractCalleeName(ExpressionSyntax expr) => expr switch
    {
        IdentifierNameSyntax id => id.Identifier.Text,
        // Package-qualified or static-class-qualified call: ClassName.Method
        MemberAccessExpressionSyntax mem => mem.Name.Identifier.Text,
        _ => null,
    };

    // ------------------------------------------------------------------ //
    //  CID helper (mirrors Go's contractCidForDeclaration)                //
    // ------------------------------------------------------------------ //

    /// <summary>
    /// Compute the BLAKE3-512 CID for a contract declaration by hashing
    /// its canonical JSON bytes, same as Go's contractCidForDeclaration.
    /// </summary>
    public static string ContractCid(ContractDecl decl)
    {
        var json = Serialize.MarshalDeclarations(new[] { decl });
        return Hash.Blake3_512Utf8(json);
    }

    /// <summary>
    /// Build a name → CID index from a list of contract declarations.
    /// </summary>
    public static IReadOnlyDictionary<string, string> BuildContractIndex(
        IReadOnlyList<ContractDecl> decls)
    {
        var idx = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var d in decls)
        {
            var cid = ContractCid(d);
            idx[d.Name] = cid;
        }
        return idx;
    }
}
