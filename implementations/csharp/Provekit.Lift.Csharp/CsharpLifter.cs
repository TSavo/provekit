using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace Provekit.Lift.Csharp;

public class CsharpLifter
{
    public LiftResult LiftPaths(string workspaceRoot, List<string> sourcePaths)
    {
        var result = new LiftResult();
        var resolvedRoot = Path.GetFullPath(workspaceRoot);
        foreach (var sourcePath in sourcePaths)
        {
            var combined = Path.Combine(resolvedRoot, sourcePath);
            var fullPath = Path.GetFullPath(combined);
            var rootPrefix = resolvedRoot.EndsWith(Path.DirectorySeparatorChar)
                ? resolvedRoot : resolvedRoot + Path.DirectorySeparatorChar;
            var isWindows = Path.DirectorySeparatorChar == '\\';
            var comp = isWindows ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
            if (fullPath != resolvedRoot && !fullPath.StartsWith(rootPrefix, comp))
            {
                result.Diagnostics.Add(Diag("error", $"path traversal rejected: {sourcePath}"));
                result.Refusals.Add(new Refusal
                {
                    Kind = "path-traversal", Function = null, Line = null,
                    Reason = $"path '{sourcePath}' escapes workspace root '{resolvedRoot}'",
                });
                continue;
            }
            if (Directory.Exists(fullPath))
            {
                try
                {
                    foreach (var file in Directory.GetFiles(fullPath, "*.cs", SearchOption.AllDirectories))
                        LiftFile(file, result);
                }
                catch (Exception ex)
                {
                    result.Diagnostics.Add(Diag("error", $"directory enumeration failed for {fullPath}: {ex.Message}"));
                    result.Refusals.Add(new Refusal
                    {
                        Kind = "io-error", Function = null, Line = null,
                        Reason = $"cannot enumerate directory '{fullPath}'",
                    });
                }
            }
            else if (File.Exists(fullPath))
            {
                LiftFile(fullPath, result);
            }
            else
            {
                result.Diagnostics.Add(Diag("warning", $"path not found: {fullPath}"));
            }
        }
        return result;
    }

    private void LiftFile(string path, LiftResult result)
    {
        string source;
        try { source = File.ReadAllText(path); }
        catch (Exception ex)
        {
            result.Diagnostics.Add(Diag("error", $"read {path}: {ex.Message}"));
            return;
        }
        // Lift only the per-method witness contracts, matching every other kit
        // (php/go/rust/...). The previous `<source-unit:{path}>` wrapper embedded
        // the whole file text plus an ABSOLUTE path into a content-addressed
        // contract -- two determinism leaks (path differs Mac/Linux; the embedded
        // text drifted the kit-hashed CID vs the verifier re-derivation). No other
        // kit emits a source-unit contract and none consume `csharp:source-unit`.
        LiftSource(source, path, result);
    }

    private static JsonObject Ctor(string name, params JsonObject[] args) => new()
    {
        ["kind"] = "ctor", ["name"] = name,
        ["args"] = JsonSerializer.SerializeToNode(args.ToList()),
    };

    private static JsonObject Skip() => Ctor("csharp:skip", IntConst(0));

    private static JsonObject IntConst(long value) => new()
    {
        ["kind"] = "const", ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("Int"),
    };

    private static JsonObject StrConst(string value) => new()
    {
        ["kind"] = "const", ["value"] = JsonValue.Create(value),
        ["sort"] = PrimSort("String"),
    };

    private static JsonObject VarTerm(string name) => new()
    {
        ["kind"] = "var", ["name"] = name
    };

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

    public void LiftSource(string source, string path, LiftResult result)
    {
        var tree = CSharpSyntaxTree.ParseText(source, path: path);
        var compilation = CSharpCompilation.Create(
            "LiftAssembly", new[] { tree }, References(),
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
        var model = compilation.GetSemanticModel(tree);
        var emittedMethods = new List<CsharpFunctionContractWalker.MethodBinding>();

        foreach (var method in tree.GetRoot().DescendantNodes().OfType<MethodDeclarationSyntax>())
        {
            var symbol = model.GetDeclaredSymbol(method);
            if (symbol is null) continue;

            var returnType = symbol.ReturnType;
            if (returnType?.SpecialType == SpecialType.System_Void)
            {
                result.Refusals.Add(new Refusal
                {
                    Kind = "unsupported-return-sort", Function = method.Identifier.Text,
                    Line = method.GetLocation().GetLineSpan().StartLinePosition.Line + 1,
                    Reason = "C# lifter slice currently expects int-returning methods, got void",
                });
                continue;
            }

            try
            {
                var contract = new ContractEmitter(method, model, path).Emit();
                if (contract is not null)
                {
                    var docId = symbol.GetDocumentationCommentId() ?? "M:?";
                    contract["fnName"] = JsonValue.Create(docId);
                    result.Declarations.Add(contract);
                    emittedMethods.Add(new CsharpFunctionContractWalker.MethodBinding(method, symbol, contract));
                }
            }
            catch (Exception ex)
            {
                result.Refusals.Add(new Refusal
                {
                    Kind = "analysis-error", Function = method.Identifier.Text,
                    Line = method.GetLocation().GetLineSpan().StartLinePosition.Line + 1,
                    Reason = ex.Message,
                });
            }
        }

        CsharpFunctionContractWalker.Apply(emittedMethods, model, path, result);
    }

    private static List<MetadataReference>? _cachedRefs;
    private static List<MetadataReference> References()
    {
        if (_cachedRefs is not null) return _cachedRefs;
        var refs = new List<MetadataReference> { MetadataReference.CreateFromFile(typeof(object).Assembly.Location) };
        var tpa = (string?)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES") ?? "";
        foreach (var p in tpa.Split(Path.PathSeparator))
        {
            if (string.IsNullOrEmpty(p)) continue;
            try { refs.Add(MetadataReference.CreateFromFile(p)); } catch { }
        }
        _cachedRefs = refs;
        return refs;
    }

    private static JsonObject Diag(string sev, string msg) => new()
    {
        ["severity"] = sev, ["message"] = msg
    };
}

public class LiftResult
{
    public List<JsonObject> Declarations { get; set; } = new();
    public List<JsonObject> CallEdges { get; set; } = new();
    public List<JsonObject> Diagnostics { get; set; } = new();
    public List<JsonObject> OpacityReport { get; set; } = new();
    public List<Refusal> Refusals { get; set; } = new();
}

public class Refusal
{
    public string Kind { get; set; } = "";
    public string? Function { get; set; }
    public int? Line { get; set; }
    public string? Instruction { get; set; }
    public string Reason { get; set; } = "";
}
