// SPDX-License-Identifier: Apache-2.0
//
// Provekit.SelfContracts — the C# peer self-contracts orchestrator.
//
// Walks every *.invariant.cs file's Register() entrypoint, mints all
// collected contracts as signed mementos, bundles into a `.proof` whose
// filename IS its catalog CID, asserts byte-determinism by minting twice
// into separate output dirs.
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

using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

using Provekit.Canonicalizer;
using Provekit.ClaimEnvelope;
using Provekit.IR;
using Provekit.ProofEnvelope;
using Provekit.SelfContracts.Invariants;

namespace Provekit.SelfContracts;

public static class Program
{
    // Foundation key — test seed [0x42; 32], same as Rust/C++/Go/TS peers.
    public static readonly byte[] FoundationSeed = Enumerable.Repeat((byte)0x42, 32).ToArray();
    public const string DeclaredAt = "2026-04-30T12:00:00.000Z";
    public const string ProducedBy = "csharp-kit@1.0";
    public const string CatalogName = "@provekit/csharp-self-contracts";
    public const string CatalogVersion = "1.0.0";

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
        var (cid1, contractCount, fileCount) = MintOneRun(outDir, verbose: true);

        var outDir2 = Path.Combine(outDir, "_determinism_check");
        Directory.CreateDirectory(outDir2);
        Console.WriteLine();
        Console.WriteLine("== mint #2 (determinism check) ==");
        var (cid2, _, _) = MintOneRun(outDir2, verbose: false);

        if (cid1 != cid2)
        {
            Console.Error.WriteLine("DETERMINISM FAILURE:");
            Console.Error.WriteLine($"  run 1 cid: {cid1}");
            Console.Error.WriteLine($"  run 2 cid: {cid2}");
            return 1;
        }
        Console.WriteLine("  determinism check:  OK (two runs produced identical CIDs)");
        Console.WriteLine();
        Console.WriteLine($"== done. C# self-application: live ({contractCount} contracts across {fileCount} .invariant.cs files). ==");
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
                        ["protocol_version"] = "provekit-lift/1",
                        ["capabilities"] = new JsonObject
                        {
                            ["authoring_surfaces"] = new JsonArray { "csharp" },
                            ["ir_version"] = "v1.1.0",
                            ["emits_signed_mementos"] = true,
                        }
                    });
                    break;

                case "lift":
                    try
                    {
                        var tmpDir = Path.Combine(Path.GetTempPath(), $"provekit-csharp-rpc-{Guid.NewGuid()}");
                        Directory.CreateDirectory(tmpDir);
                        try
                        {
                            var (cid, _, _) = MintOneRun(tmpDir, verbose: false);
                            var proofPath = Path.Combine(tmpDir, $"{cid}.proof");
                            var bytes = File.ReadAllBytes(proofPath);
                            var b64 = Convert.ToBase64String(bytes);
                            WriteResponse(id, new JsonObject
                            {
                                ["kind"] = "proof-envelope",
                                ["filename_cid"] = cid,
                                ["bytes_base64"] = b64,
                                ["diagnostics"] = new JsonArray(),
                            });
                        }
                        finally
                        {
                            try { Directory.Delete(tmpDir, recursive: true); } catch { }
                        }
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
    /// (also used as the .proof filename), the number of contracts,
    /// and the number of invariant files registered.
    /// </summary>
    public static (string Cid, int ContractCount, int FileCount) MintOneRun(string outDir, bool verbose)
    {
        // 1. Register every .invariant.cs module.
        Collector.BeginCollecting();
        var fileCount = RegisterAll();
        var contractDecls = Collector.Finish();

        if (verbose)
        {
            Console.WriteLine($"authored {contractDecls.Count} contracts across {fileCount} .invariant.cs files");
        }

        // 2. Mint each as a signed ClaimEnvelope under the foundation key.
        // Use SortedDictionary so insertion order doesn't leak into bytes
        // (the proof envelope sorts CBOR keys, but keep dict order tidy).
        var members = new Dictionary<string, byte[]>();
        foreach (var d in contractDecls)
        {
            var args = new MintContractArgs
            {
                ContractName = d.Name,
                Pre = d.Pre is null ? null : Serialize.FormulaToValue(d.Pre),
                Post = d.Post is null ? null : Serialize.FormulaToValue(d.Post),
                Inv = d.Inv is null ? null : Serialize.FormulaToValue(d.Inv),
                OutBinding = d.OutBinding,
                ProducedBy = ProducedBy,
                ProducedAt = DeclaredAt,
                InputCids = Array.Empty<string>(),
                Authoring = new Authoring.KitAuthor(ProducedBy),
                SignerSeed = FoundationSeed,
            };
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

        // 4. Write <cid>.proof to disk.
        var outPath = Path.Combine(outDir, $"{built.Cid}.proof");
        File.WriteAllBytes(outPath, built.Bytes);

        if (verbose)
        {
            Console.WriteLine($"  catalog CID:        {built.Cid}");
            Console.WriteLine($"  proof bytes:        {built.Bytes.Length}");
            Console.WriteLine($"  .proof file:        {outPath}");
        }

        return (built.Cid, contractDecls.Count, fileCount);
    }

    /// <summary>
    /// Call every static invariant Register() entrypoint. Returns the
    /// count of files registered (one per invariant slab).
    ///
    /// Order is alphabetical by slab name to match the cross-language
    /// peers' registration order semantics; the protocol bytes don't
    /// depend on registration order (members are keyed by CID), but
    /// keeping it stable aids diffing.
    /// </summary>
    private static int RegisterAll()
    {
        // Provekit.Canonicalizer (3 slabs)
        HashInvariants.Register();
        JcsInvariants.Register();
        ValueInvariants.Register();

        // Provekit.IR (7 slabs)
        SortInvariants.Register();
        TermInvariants.Register();
        FormulaInvariants.Register();
        PredicatesInvariants.Register();
        QuantifiersInvariants.Register();
        CollectorInvariants.Register();
        SerializeInvariants.Register();

        // Provekit.ClaimEnvelope (2 slabs)
        AuthoringInvariants.Register();
        MintInvariants.Register();

        // Provekit.ProofEnvelope (3 slabs)
        CborInvariants.Register();
        SignInvariants.Register();
        ProofInvariants.Register();

        // Cross-kit bridges (Phase 2: lift-plugin protocol counterparts).
        CrossKitBridges.Register();

        return 16;
    }
}
