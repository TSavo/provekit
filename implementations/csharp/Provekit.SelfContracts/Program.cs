// SPDX-License-Identifier: Apache-2.0
//
// Provekit.SelfContracts: the C# peer self-contracts orchestrator.
//
// Lifts native C# self-contract witness sources, mints the resulting
// function-contract declarations as signed mementos, bundles into a
// `.proof` whose filename IS its catalog CID, and asserts byte-determinism
// by minting twice into separate output dirs.
//
// Mirrors:
//   implementations/rust/provekit-self-contracts/src/lib.rs
//   implementations/cpp/provekit-self-contracts/mint_cpp_self_contracts.cpp
//   implementations/go/provekit-self-contracts/cmd/mint-go-self-contracts/main.go
//
// Run:
//   dotnet run --project implementations/csharp/Provekit.SelfContracts
//   dotnet run --project implementations/csharp/Provekit.SelfContracts -- /tmp/csharp-self-out
//
// The protocol is the bytes. The minted .proof is conformant with the
// catalog (v1.1.0) and verifies under the same foundation key as the
// Rust / Go / C++ / TS peers.

using System.Text.Json;
using System.Text.Json.Nodes;

using Provekit.Canonicalizer;
using Provekit.ClaimEnvelope;
using Provekit.IR;
using Provekit.Lift.Csharp;
using Provekit.ProofEnvelope;
using Provekit.SelfContracts.Invariants;
using CValue = Provekit.Canonicalizer.Value;

namespace Provekit.SelfContracts;

public static class Program
{
    // Foundation key: test seed [0x42; 32], same as Rust/C++/Go/TS peers.
    public static readonly byte[] FoundationSeed = Enumerable.Repeat((byte)0x42, 32).ToArray();
    public const string DeclaredAt = "2026-04-30T12:00:00.000Z";
    public const string ProducedBy = "csharp-kit@1.0";
    public const string CatalogName = "@provekit/csharp-self-contracts";
    public const string CatalogVersion = "1.0.0";
    private static readonly string[] NativeContractSourcePaths =
    [
        "Provekit.Canonicalizer/Hash.Contracts.cs",
        "Provekit.Canonicalizer/Jcs.Contracts.cs",
        "Provekit.Canonicalizer/Value.Contracts.cs",
        "Provekit.IR/Sort.Contracts.cs",
        "Provekit.IR/Term.Contracts.cs",
        "Provekit.IR/Formula.Contracts.cs",
        "Provekit.IR/Predicates.Contracts.cs",
        "Provekit.IR/Quantifiers.Contracts.cs",
        "Provekit.IR/Collector.Contracts.cs",
        "Provekit.IR/Serialize.Contracts.cs",
        "Provekit.ClaimEnvelope/Authoring.Contracts.cs",
        "Provekit.ClaimEnvelope/Mint.Contracts.cs",
        "Provekit.ProofEnvelope/Cbor.Contracts.cs",
        "Provekit.ProofEnvelope/Sign.Contracts.cs",
        "Provekit.ProofEnvelope/Proof.Contracts.cs",
    ];

    public static int Main(string[] args)
    {
        if (args.Contains("--rpc"))
        {
            RunRpcMode();
            return 0;
        }

        var outDir = args.Length >= 1 ? args[0] : ".";

        Console.WriteLine("== ProvekIt C# self-contracts orchestrator ==");
        Console.WriteLine();
        Console.WriteLine($"output dir: {outDir}");
        Console.WriteLine();

        Directory.CreateDirectory(outDir);

        Console.WriteLine("== mint #1 ==");
        var (cid1, contractSetCid1, contractCount, fileCount) = MintOneRun(outDir, verbose: true);

        var outDir2 = Path.Combine(outDir, "_determinism_check");
        Directory.CreateDirectory(outDir2);
        Console.WriteLine();
        Console.WriteLine("== mint #2 (determinism check) ==");
        var (cid2, contractSetCid2, _, _) = MintOneRun(outDir2, verbose: false);

        if (cid1 != cid2 || contractSetCid1 != contractSetCid2)
        {
            Console.Error.WriteLine("DETERMINISM FAILURE:");
            Console.Error.WriteLine($"  run 1 cid:              {cid1}");
            Console.Error.WriteLine($"  run 2 cid:              {cid2}");
            Console.Error.WriteLine($"  run 1 contractSetCid:   {contractSetCid1}");
            Console.Error.WriteLine($"  run 2 contractSetCid:   {contractSetCid2}");
            return 1;
        }
        Console.WriteLine("  determinism check:  OK (two runs produced identical CIDs)");
        Console.WriteLine();
        Console.WriteLine($"== done. C# self-application: live ({contractCount} contracts across {fileCount} native C# sources). ==");
        return 0;
    }

    private static void RunRpcMode()
    {
        while (true)
        {
            var line = Console.ReadLine();
            if (line == null) break;
            if (string.IsNullOrWhiteSpace(line)) continue;

            JsonNode? req;
            try
            {
                req = JsonNode.Parse(line);
            }
            catch
            {
                WriteError(null, -32700, "Parse error");
                continue;
            }

            if (req == null)
            {
                WriteError(null, -32700, "Parse error");
                continue;
            }

            var id = req["id"];
            var method = req["method"]?.GetValue<string>();

            switch (method)
            {
                case "initialize":
                    WriteResponse(id, new JsonObject
                    {
                        ["name"] = "csharp-self-contracts",
                        ["version"] = "1.0.0",
                        ["protocol_version"] = "pep/1.7.0",
                        ["capabilities"] = new JsonObject
                        {
                            ["authoring_surfaces"] = new JsonArray { "csharp" },
                            ["ir_version"] = "v1.1.0",
                            ["emits_signed_mementos"] = false,
                        }
                    });
                    break;

                case "lift":
                    try
                    {
                        var paramsObj = req["params"] as JsonObject;
                        var workspaceRoot = paramsObj?["workspace_root"]?.GetValue<string>() ?? FindCsharpWorkspaceRoot();
                        var document = LiftNativeContractDocument(workspaceRoot);
                        WriteResponse(id, new JsonObject
                        {
                            ["kind"] = "ir-document",
                            ["ir"] = JsonSerializer.SerializeToNode(document.Declarations),
                            ["callEdges"] = JsonSerializer.SerializeToNode(document.CallEdges),
                            ["diagnostics"] = JsonSerializer.SerializeToNode(document.Diagnostics),
                            ["refusals"] = JsonSerializer.SerializeToNode(document.Refusals),
                        });
                    }
                    catch (Exception ex)
                    {
                        WriteError(id, -32603, $"Lift failed: {ex.Message}");
                    }
                    break;

                case "shutdown":
                    WriteResponse(id, null);
                    return;

                default:
                    WriteError(id, -32601, $"METHOD_NOT_FOUND: {method}");
                    break;
            }
        }
    }

    private static void WriteResponse(JsonNode? id, JsonNode? result)
    {
        var resp = new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone() ?? JsonValue.Create((object?)null),
            ["result"] = result?.DeepClone() ?? JsonValue.Create((object?)null),
        };
        Console.WriteLine(resp.ToJsonString(JsonSerializerOptions.Web));
    }

    private static void WriteError(JsonNode? id, int code, string message)
    {
        var resp = new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone() ?? JsonValue.Create((object?)null),
            ["error"] = new JsonObject
            {
                ["code"] = code,
                ["message"] = message,
            }
        };
        Console.WriteLine(resp.ToJsonString(JsonSerializerOptions.Web));
    }

    /// <summary>
    /// One full author + mint + bundle pass. Returns the catalog CID
    /// (also used as the .proof filename), the contractSetCid (spec #94),
    /// the number of contracts, and the number of native source files lifted.
    /// </summary>
    public static (string Cid, string ContractSetCid, int ContractCount, int FileCount) MintOneRun(string outDir, bool verbose)
    {
        // 1. Lift every native C# self-contract witness source.
        var document = LiftNativeContractDocument(FindCsharpWorkspaceRoot());
        var contractDecls = document.Declarations;

        if (verbose)
        {
            Console.WriteLine($"lifted {contractDecls.Count} contracts across {document.SourceFileCount} native C# sources");
        }

        // 2. Mint each as a signed ClaimEnvelope under the foundation key.
        // Use SortedDictionary so insertion order doesn't leak into bytes
        // (the proof envelope sorts CBOR keys, but keep dict order tidy).
        var members = new Dictionary<string, byte[]>();
        var contentCids = new List<string>();
        foreach (var d in contractDecls)
        {
            var args = new MintContractArgs
            {
                ContractName = ContractName(d),
                Pre = FormulaValue(d, "pre"),
                Post = FormulaValue(d, "post"),
                Inv = FormulaValue(d, "inv"),
                OutBinding = d["outBinding"]?.GetValue<string>() ?? "out",
                ProducedBy = ProducedBy,
                ProducedAt = DeclaredAt,
                InputCids = Array.Empty<string>(),
                Authoring = new Authoring.KitAuthor(ProducedBy),
                SignerSeed = FoundationSeed,
            };
            // Compute signer-independent content CID BEFORE minting (spec #94).
            var contentCid = Mint.ContractCid(args);
            contentCids.Add(contentCid);
            var minted = Mint.MintContract(args);
            members[minted.Cid] = minted.CanonicalBytes;
        }

        // 3. Build the catalog .proof (deterministic CBOR + Ed25519).
        // signer_cid = BLAKE3-512 of the self-identifying pubkey string,
        // matching the Rust peer (`signer_cid = blake3_512_of(signer_pubkey.as_bytes())`).
        var pubkeyString = Sign.Ed25519PubkeyString(FoundationSeed);
        var signerCid = Hash.Blake3_512Utf8(pubkeyString);

        var proofInput = new ProofEnvelopeInput
        {
            Name = CatalogName,
            Version = CatalogVersion,
            Members = members,
            SignerCid = signerCid,
            SignerSeed = FoundationSeed,
            DeclaredAt = DeclaredAt,
        };
        var built = Proof.Build(proofInput);

        // 4. Compute contractSetCid (spec #94): signer-independent trust anchor.
        var contractSetCid = Mint.ContractSetCid(contentCids);

        // 5. Write <cid>.proof to disk.
        var outPath = Path.Combine(outDir, $"{built.Cid}.proof");
        File.WriteAllBytes(outPath, built.Bytes);

        if (verbose)
        {
            Console.WriteLine($"  catalog CID:        {built.Cid}");
            Console.WriteLine($"  contractSetCid:     {contractSetCid}");
            Console.WriteLine($"  proof bytes:        {built.Bytes.Length}");
            Console.WriteLine($"  .proof file:        {outPath}");
        }

        return (built.Cid, contractSetCid, contractDecls.Count, document.SourceFileCount);
    }

    private sealed record NativeContractDocument(
        List<JsonObject> Declarations,
        List<JsonObject> CallEdges,
        List<JsonObject> Diagnostics,
        List<Refusal> Refusals,
        int SourceFileCount);

    private static NativeContractDocument LiftNativeContractDocument(string workspaceRoot)
    {
        var lifter = new CsharpLifter();
        var result = lifter.LiftPaths(workspaceRoot, NativeContractSourcePaths.ToList());
        var declarations = result.Declarations
            .Select(NormalizeFunctionContractName)
            .ToList();
        declarations.AddRange(CrossKitBridgeContracts());

        return new NativeContractDocument(
            declarations,
            result.CallEdges,
            result.Diagnostics,
            result.Refusals,
            NativeContractSourcePaths.Length + 1);
    }

    private static JsonObject NormalizeFunctionContractName(JsonObject declaration)
    {
        var clone = declaration.DeepClone().AsObject();
        if (clone["kind"]?.GetValue<string>() == "function-contract"
            && clone["name"] is null
            && clone["fnName"] is not null)
        {
            clone["name"] = clone["fnName"]!.DeepClone();
        }
        return clone;
    }

    private static List<JsonObject> CrossKitBridgeContracts()
    {
        Collector.BeginCollecting();
        CrossKitBridges.Register();
        var contracts = Collector.Finish();
        var json = Serialize.MarshalDeclarations(contracts);
        return JsonNode.Parse(json)!.AsArray()
            .Select(node => node!.AsObject())
            .ToList();
    }

    private static string ContractName(JsonObject declaration) =>
        declaration["name"]?.GetValue<string>()
        ?? declaration["fnName"]?.GetValue<string>()
        ?? declaration["symbol"]?.GetValue<string>()
        ?? "unnamed";

    private static CValue? FormulaValue(JsonObject declaration, string key) =>
        declaration[key] is JsonNode node ? JsonToValue(node) : null;

    private static CValue JsonToValue(JsonNode? node)
    {
        if (node is null) return CValue.Null;
        if (node is JsonObject obj)
        {
            return CValue.Object(obj.Select(pair =>
                new KeyValuePair<string, CValue>(pair.Key, JsonToValue(pair.Value))).ToArray());
        }
        if (node is JsonArray arr)
        {
            return CValue.Array(arr.Select(JsonToValue).ToArray());
        }
        var value = node.AsValue();
        if (value.TryGetValue<string>(out var s)) return CValue.String(s);
        if (value.TryGetValue<bool>(out var b)) return CValue.Boolean(b);
        if (value.TryGetValue<long>(out var l)) return CValue.Integer(l);
        if (value.TryGetValue<int>(out var i)) return CValue.Integer(i);
        if (value.TryGetValue<uint>(out var u)) return CValue.Integer(u);
        if (value.TryGetValue<ulong>(out var ul)) return CValue.Integer(unchecked((long)ul));
        if (value.TryGetValue<double>(out var d)) return CValue.Integer((long)d);
        return CValue.String(node.ToJsonString());
    }

    private static string FindCsharpWorkspaceRoot()
    {
        foreach (var start in new[] { Directory.GetCurrentDirectory(), AppContext.BaseDirectory })
        {
            var dir = new DirectoryInfo(start);
            while (dir is not null)
            {
                if (File.Exists(Path.Combine(dir.FullName, NativeContractSourcePaths[0])))
                {
                    return dir.FullName;
                }
                var nested = Path.Combine(dir.FullName, "implementations", "csharp");
                if (File.Exists(Path.Combine(nested, NativeContractSourcePaths[0])))
                {
                    return nested;
                }
                dir = dir.Parent;
            }
        }
        throw new DirectoryNotFoundException("could not locate implementations/csharp workspace root");
    }
}
