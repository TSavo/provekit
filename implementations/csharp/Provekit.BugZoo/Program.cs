// SPDX-License-Identifier: Apache-2.0

using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using Provekit.Canonicalizer;
using Provekit.IR;
using Provekit.Lift.Core;
using Provekit.Lift.Linq;

var mode = args.Length > 0 ? args[0] : "";
var surface = args.Length > 1 ? args[1] : "";
if (mode == "discover")
{
    if (args.Length != 3)
    {
        Console.Error.WriteLine("Usage: Provekit.BugZoo discover <surface> <workspaceRoot>");
        Environment.Exit(1);
    }

    var discovery = BugZooRpc.Discover(surface, args[2]);
    Console.WriteLine(JsonSerializer.Serialize(discovery, BugZooRpc.JsonOptions));
    return;
}

new BugZooRpc(mode, surface).Run();

sealed class BugZooRpc
{
    internal static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    private const string MissingEdge = "maybe_null(name) => non_null(name)";
    private const string SourcePredicate = "maybe_null(name)";
    private const string TargetPredicate = "non_null(name)";
    private const string CanonicalPostLiftCid = "blake3-512:209a087d2638b71b93ccfb8ac7081e15b2759ac2e59907aaa5c38c2d0bc8d873663f5fafa5b4d57c452c23c9b9112147d2f14ef71b65dd210ad3e3d744d3a08b";
    private const string CanonicalClosureWitnessCid = "blake3-512:0d7db11b2df12f815f5cf7aadae95886919466b93a7e77c05d685be6b49c25ff1f0518c9b3291bc77dc2a7fb1a46982b7c1191dd4bec91abbae13fa8521f2e21";

    private readonly string mode;
    private readonly string surface;

    public BugZooRpc(string mode, string surface)
    {
        this.mode = mode;
        this.surface = surface;
    }

    public void Run()
    {
        string? line;
        while ((line = Console.ReadLine()) is not null)
        {
            object? id = null;
            try
            {
                using var document = JsonDocument.Parse(line);
                var request = document.RootElement;
                id = ReadId(request);
                var method = request.GetProperty("method").GetString();

                if (method == "initialize")
                {
                    WriteResult(id, new
                    {
                        name = "provekit-csharp-bug-zoo",
                        version = "0",
                        capabilities = new[] { "bug-zoo-boundary", "csharp-source-lifter", "csharp-linq-lifter" },
                    });
                    continue;
                }

                if (method == "lift" && mode == "lifter")
                {
                    var workspaceRoot = request.GetProperty("params").GetProperty("workspace_root").GetString()
                        ?? throw new InvalidOperationException("lift params.workspace_root must be a string");
                    WriteResult(id, Lift(workspaceRoot));
                    continue;
                }

                if (method == "realize" && mode == "realizer")
                {
                    var plan = request.GetProperty("params").GetProperty("plan");
                    WriteResult(id, new { output = Realize(plan) });
                    continue;
                }

                if (method == "shutdown")
                {
                    return;
                }

                throw new InvalidOperationException($"unsupported {mode} method {method}");
            }
            catch (Exception error)
            {
                WriteEnvelope(new { jsonrpc = "2.0", id, error = new { code = -32000, message = error.Message } });
            }
        }
    }

    private object Lift(string workspaceRoot)
    {
        var discovery = Discover(surface, workspaceRoot);

        return new
        {
            kind = "ir-document",
            ir = NullBoundaryIrJson(),
            source = discovery,
        };
    }

    internal static object Discover(string surface, string workspaceRoot)
    {
        var sources = Directory.GetFiles(workspaceRoot, "*.cs", SearchOption.AllDirectories)
            .Select(path => (path, text: File.ReadAllText(path)))
            .ToList();
        if (sources.Count == 0)
        {
            throw new InvalidOperationException($"no C# sources found in {workspaceRoot}");
        }

        var evidence = surface switch
        {
            "csharp-data-annotations" => DiscoverDataAnnotations(sources),
            "csharp-provekit-annotations" => DiscoverProvekitAnnotations(sources),
            "csharp-linq" => DiscoverLinq(sources),
            _ => throw new InvalidOperationException($"unsupported C# zoo surface {surface}"),
        };

        return new
        {
            kind = "bug-zoo-discovery",
            language = "csharp",
            toolchain = "dotnet",
            surface,
            boundary = SourcePredicate,
            sink = TargetPredicate,
            missingEdge = MissingEdge,
            evidence,
        };
    }

    private static NativeDiscovery DiscoverDataAnnotations(IReadOnlyList<(string path, string text)> sources)
    {
        var source = sources.FirstOrDefault(item => item.text.Contains("[Required]", StringComparison.Ordinal));
        if (source.text is null)
        {
            throw new InvalidOperationException("data-annotations exposure missing [Required]");
        }

        var decls = SourceLifter.LiftSource(source.text, source.path);
        var irJson = Serialize.MarshalDeclarations(decls);
        var hasRequiredName = decls.Any(decl => decl.Name == "LookupRequest.Name")
            && irJson.Contains("\"name\":\"≠\"", StringComparison.Ordinal);
        if (!hasRequiredName)
        {
            throw new InvalidOperationException("SourceLifter did not lift LookupRequest.Name [Required]");
        }

        return new NativeDiscovery(
            Adapter: "csharp-source-lifter",
            Contract: "LookupRequest.Name",
            Lifter: "DataAnnotationsLift",
            SourcePath: Path.GetFileName(source.path),
            IrEvidenceCid: Hash.Blake3_512(Encoding.UTF8.GetBytes(irJson)));
    }

    private static NativeDiscovery DiscoverProvekitAnnotations(IReadOnlyList<(string path, string text)> sources)
    {
        var source = sources.FirstOrDefault(item => item.text.Contains("//provekit:contract", StringComparison.Ordinal));
        if (source.text is null)
        {
            throw new InvalidOperationException("annotation exposure missing //provekit:contract");
        }

        var decls = SourceLifter.LiftSource(source.text, source.path);
        var irJson = Serialize.MarshalDeclarations(decls);
        if (!decls.Any(decl => decl.Name == "Lookup"))
        {
            throw new InvalidOperationException("SourceLifter annotation scan did not lift Lookup");
        }

        return new NativeDiscovery(
            Adapter: "csharp-source-lifter",
            Contract: "Lookup",
            Lifter: "AnnotationScanner",
            SourcePath: Path.GetFileName(source.path),
            IrEvidenceCid: Hash.Blake3_512(Encoding.UTF8.GetBytes(irJson)));
    }

    private static NativeDiscovery DiscoverLinq(IReadOnlyList<(string path, string text)> sources)
    {
        var source = sources.FirstOrDefault(item => item.text.Contains(".Where(name => name != null)", StringComparison.Ordinal));
        if (source.text is null)
        {
            throw new InvalidOperationException("LINQ exposure missing Where(name => name != null)");
        }

        var lifted = new LinqLifter().Lift(source.text, source.path);
        var memento = lifted.SingleOrDefault(item => item.Name == "nonNull_where")
            ?? throw new InvalidOperationException("LinqLifter did not lift nonNull_where");
        if (!memento.IrJson.Contains("\"name\":\"≠\"", StringComparison.Ordinal)
            || !memento.IrJson.Contains("\"value\":null", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("LinqLifter did not preserve name != null");
        }

        return new NativeDiscovery(
            Adapter: "csharp-linq-lifter",
            Contract: memento.Name,
            Lifter: "LinqLifter",
            SourcePath: Path.GetFileName(source.path),
            IrEvidenceCid: Hash.Blake3_512(Encoding.UTF8.GetBytes(memento.IrJson)));
    }

    private static object Realize(JsonElement plan)
    {
        var source = plan.GetProperty("source").GetString()
            ?? throw new InvalidOperationException("realizer plan source must be a string");
        var gapCid = plan.GetProperty("gapCid").GetString()
            ?? throw new InvalidOperationException("realizer plan gapCid must be a string");
        var policyCid = plan.GetProperty("policyCid").GetString()
            ?? throw new InvalidOperationException("realizer plan policyCid must be a string");

        Require(plan.GetProperty("surface").GetString() == "csharp-native", $"unsupported C# surface for {MissingEdge}");
        Require(plan.GetProperty("targetSymbol").GetString() == "lookup", $"unsupported C# target for {MissingEdge}");
        Require(plan.GetProperty("proofVar").GetString() == "name", $"unsupported C# proof var for {MissingEdge}");
        Require(plan.GetProperty("sourcePredicate").GetString() == SourcePredicate, "unsupported source predicate");
        Require(plan.GetProperty("targetPredicate").GetString() == TargetPredicate, "unsupported target predicate");
        Require(source == LabSource(), "unsupported source shape for C# null-boundary realizer");

        var modifiedSource = DroppedSource();
        var transformedArtifactCid = Hash.Blake3_512(Encoding.UTF8.GetBytes(modifiedSource));
        var postLiftCid = CanonicalPostLiftCid;
        var closureWitnessCid = CanonicalClosureWitnessCid;

        return new
        {
            status = "closed",
            modifiedSource,
            gapCid,
            transformedArtifactCid,
            postLiftCid,
            postLift = PostLiftJson(),
            closureWitness = ClosureWitnessJson(gapCid, policyCid, postLiftCid, transformedArtifactCid),
            closureWitnessCid,
        };
    }

    private static object[] NullBoundaryIrJson() =>
    [
        new
        {
            kind = "contract",
            symbol = "lookup",
            precondition = new
            {
                kind = "atomic",
                name = "neq",
                args = new object?[]
                {
                    new { kind = "var", name = "name" },
                    new { kind = "const", value = (object?)null, sort = new { kind = "primitive", name = "Ref" } },
                },
            },
        },
    ];

    private static object PostLiftJson() => new
    {
        kind = "ir-document",
        ir = NullBoundaryIrJson(),
        source = new
        {
            adapter = "csharp-native-dropper",
            contract = "lookup",
            sourcePath = "dropped/csharp-native/library/src/UserDirectory.cs",
        },
    };

    private static object ClosureWitnessJson(
        string gapCid,
        string policyCid,
        string postLiftCid,
        string transformedArtifactCid) => new
        {
            kind = "TruthDischargeBodyClaim",
            claimKind = "closure",
            gapCid,
            policyCid,
            postLiftCid,
            sourcePredicate = SourcePredicate,
            targetPredicate = TargetPredicate,
            transformedArtifactCid,
        };

    private static string LabSource() =>
        "namespace BugZoo.CSharpNullBoundary;\n" +
        "\n" +
        "public static class UserDirectory\n" +
        "{\n" +
        "    public static string Lookup(string name) => \"user:\" + name.ToUpperInvariant();\n" +
        "}\n";

    private static string DroppedSource() =>
        "namespace BugZoo.CSharpNullBoundary;\n" +
        "\n" +
        "public static class UserDirectory\n" +
        "{\n" +
        "    public static string Lookup(string? name)\n" +
        "    {\n" +
        "        if (name is null)\n" +
        "        {\n" +
        "            throw new ArgumentNullException(nameof(name), \"name must be non-null\");\n" +
        "        }\n" +
        "\n" +
        "        return \"user:\" + name.ToUpperInvariant();\n" +
        "    }\n" +
        "}\n";

    private static object? ReadId(JsonElement request)
    {
        if (!request.TryGetProperty("id", out var id))
        {
            return null;
        }

        return id.ValueKind switch
        {
            JsonValueKind.Number when id.TryGetInt64(out var value) => value,
            JsonValueKind.String => id.GetString(),
            _ => null,
        };
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private static void WriteResult(object? id, object result) =>
        WriteEnvelope(new { jsonrpc = "2.0", id, result });

    private static void WriteEnvelope(object value)
    {
        Console.WriteLine(JsonSerializer.Serialize(value, JsonOptions));
        Console.Out.Flush();
    }

    private sealed record NativeDiscovery(
        string Adapter,
        string Contract,
        string Lifter,
        string SourcePath,
        string IrEvidenceCid);
}
