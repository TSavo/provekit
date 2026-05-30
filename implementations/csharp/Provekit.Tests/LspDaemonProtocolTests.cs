// SPDX-License-Identifier: Apache-2.0
//
// Integration smoke test: protocol conformance of the provekit-lsp-csharp binary.
//
// Asserts:
//   - The binary responds to initialize with the shared LSP protocol shape.
//   - analyzeDocument returns an lsp-document-analysis envelope.
//   - The solver-facing diagnostic lands on the broken callsite.
//   - parse response has result.declarations as a JSON array, not a string.
//   - parse response has result.callEdges as a JSON array.
//   - parse response has result.warnings as a JSON array.
//   - Each declaration in a non-empty result is an object with kind=="contract".
//   - Each declaration has a "name" field.
//   - Empty/trivial source returns declarations==[] and callEdges==[].
//   - Byte-determinism: two independent runs on the same input produce identical parse output.
//
// Binary discovery (in order):
//   1. PROVEKIT_LSP_CSHARP_BIN env var (for CI override).
//   2. provekit-lsp-csharp on PATH (post dotnet publish install).
//   3. Release build output relative to this file's assembly location.

using System.Diagnostics;
using System.Text;
using System.Text.Json;
using Xunit;

namespace Provekit.Tests;

public class LspDaemonProtocolTests
{
    // ── Fixture source (//provekit:contract annotation → at least one declaration)
    private const string FixtureSource =
        "//provekit:contract\n" +
        "public static void ValidateName(string name) {}\n";

    private const string FixturePath = "fixture.cs";

    private const string FloorFixtureSource =
        "public static class FloorFixture {\n" +
        "  public static bool checkPositive(int x) {\n" +
        "    if (x <= 0) { return false; }\n" +
        "    return true;\n" +
        "  }\n" +
        "  public static bool CallerSatisfiesPre() {\n" +
        "    var result = checkPositive(5);\n" +
        "    return result;\n" +
        "  }\n" +
        "  public static bool CallerViolatesPre() {\n" +
        "    var result = checkPositive(-1);\n" +
        "    return result;\n" +
        "  }\n" +
        "  public static bool CallerWithLoop() {\n" +
        "    for (var i = 0; i < 10; i++) {\n" +
        "      var result = checkPositive(i);\n" +
        "      if (!result) { return false; }\n" +
        "    }\n" +
        "    return true;\n" +
        "  }\n" +
        "}\n";

    // ── Binary resolution ────────────────────────────────────────────────────

    private static string? _binaryPath;

    private static string BinaryPath()
    {
        if (_binaryPath is not null) return _binaryPath;

        // 1. Env override.
        var envPath = Environment.GetEnvironmentVariable("PROVEKIT_LSP_CSHARP_BIN");
        if (!string.IsNullOrEmpty(envPath) && File.Exists(envPath))
            return _binaryPath = envPath;

        // 2. On PATH.
        var onPath = WhichBinary("provekit-lsp-csharp");
        if (onPath is not null)
            return _binaryPath = onPath;

        // 3. Build-output relative to this assembly's directory.
        // The test assembly lives in:
        //   implementations/csharp/Provekit.Tests/bin/<cfg>/net10.0/
        // The LSP plugin is built at:
        //   implementations/csharp/Provekit.Lsp.Plugin/bin/Release/net10.0/provekit-lsp-csharp
        var asmDir = Path.GetDirectoryName(typeof(LspDaemonProtocolTests).Assembly.Location)
                     ?? Directory.GetCurrentDirectory();

        // Walk up to implementations/csharp/
        var dir = new DirectoryInfo(asmDir);
        while (dir is not null && dir.Name != "csharp")
            dir = dir.Parent;

        if (dir is not null)
        {
            // Try Release then Debug.
            foreach (var cfg in new[] { "Release", "Debug" })
            {
                var candidate = Path.Combine(
                    dir.FullName,
                    "Provekit.Lsp.Plugin", "bin", cfg, "net10.0", "provekit-lsp-csharp");
                if (File.Exists(candidate))
                    return _binaryPath = candidate;
            }
        }

        throw new InvalidOperationException(
            "provekit-lsp-csharp binary not found. " +
            "Run `dotnet build implementations/csharp/Provekit.Lsp.Plugin` first, " +
            "or set PROVEKIT_LSP_CSHARP_BIN to the binary path.");
    }

    private static string? WhichBinary(string name)
    {
        var pathVar = Environment.GetEnvironmentVariable("PATH") ?? "";
        var sep = Path.PathSeparator;
        foreach (var dir in pathVar.Split(sep))
        {
            var candidate = Path.Combine(dir, name);
            if (File.Exists(candidate))
                return candidate;
        }
        return null;
    }

    // ── NDJSON session builder ───────────────────────────────────────────────

    private static string BuildSession(string source = FixtureSource, string path = FixturePath)
    {
        var msgs = new object[]
        {
            new { jsonrpc = "2.0", id = 1, method = "initialize", @params = new { } },
            new { jsonrpc = "2.0", id = 2, method = "parse",
                  @params = new { path, source } },
            new { jsonrpc = "2.0", id = 3, method = "shutdown" },
        };
        var sb = new StringBuilder();
        foreach (var m in msgs)
        {
            sb.AppendLine(JsonSerializer.Serialize(m));
        }
        return sb.ToString();
    }

    private static string BuildAnalyzeSession(string source = FloorFixtureSource, string path = "FloorFixture.cs")
    {
        var msgs = new object[]
        {
            new { jsonrpc = "2.0", id = 1, method = "initialize", @params = new { } },
            new { jsonrpc = "2.0", id = 2, method = "analyzeDocument",
                  @params = new
                  {
                      kit_id = "csharp",
                      uri = $"file:///project/{path}",
                      file = path,
                      text = source,
                      document_version = 42,
                      workspace_root = "/project",
                      accepted_protocol_catalog_cids = Array.Empty<string>(),
                      policy_cids = Array.Empty<string>(),
                  } },
            new { jsonrpc = "2.0", id = 3, method = "shutdown" },
        };
        var sb = new StringBuilder();
        foreach (var m in msgs)
        {
            sb.AppendLine(JsonSerializer.Serialize(m));
        }
        return sb.ToString();
    }

    private static async Task<List<JsonDocument>> RunLsp(string ndjson)
    {
        var binary = BinaryPath();
        var psi = new ProcessStartInfo(binary, "--rpc")
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        using var proc = Process.Start(psi)
                         ?? throw new InvalidOperationException("Failed to start LSP binary");

        proc.StandardInput.Write(ndjson);
        proc.StandardInput.Close();

        // Drain stdout and stderr asynchronously so the timeout path
        // is reachable even if the LSP process hangs. Blocking
        // ReadToEnd() would deadlock before WaitForExit is called.
        var outputTask = proc.StandardOutput.ReadToEndAsync();
        var stderrDrain = proc.StandardError.ReadToEndAsync(); // avoid pipe deadlock
        if (!proc.WaitForExit(15_000))
        {
            proc.Kill(entireProcessTree: true);
            throw new Exception("Process did not exit within 15000ms");
        }
        var output = await outputTask;
        await stderrDrain; // drain stderr to avoid unobserved exception

        Assert.Equal(0, proc.ExitCode);

        var docs = new List<JsonDocument>();
        foreach (var line in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            docs.Add(JsonDocument.Parse(line));
        }
        return docs;
    }

    private static JsonElement FindById(List<JsonDocument> docs, int id)
    {
        foreach (var doc in docs)
        {
            var root = doc.RootElement;
            if (root.TryGetProperty("id", out var idProp) && idProp.GetInt32() == id)
                return root;
        }
        throw new InvalidOperationException($"No response with id={id} found");
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    [Fact]
    public async Task Initialize_ReturnsExpectedShape()
    {
        var responses = await RunLsp(BuildSession());
        var initResp = FindById(responses, 1);

        Assert.True(initResp.TryGetProperty("result", out var result),
            "initialize response missing 'result'");
        Assert.Equal("provekit-lsp-csharp", result.GetProperty("name").GetString());
        Assert.Equal("provekit-lsp-shared/1", result.GetProperty("protocol_version").GetString());
        Assert.Equal("csharp", result.GetProperty("kit_id").GetString());
        AssertBlake3Cid(result.GetProperty("protocol_catalog_cid").GetString());
        Assert.True(result.TryGetProperty("capabilities", out var caps));
        Assert.Equal(JsonValueKind.Object, caps.ValueKind);
        Assert.Contains("csharp-source",
            caps.GetProperty("source_surfaces").EnumerateArray().Select(c => c.GetString()));
        Assert.Contains("provekit.lsp.implication_failed",
            caps.GetProperty("diagnostic_codes").EnumerateArray().Select(c => c.GetString()));
    }

    [Fact]
    public async Task AnalyzeDocument_FloorFixtureEmitsSharedCallsiteDiagnostic()
    {
        var responses = await RunLsp(BuildAnalyzeSession());
        var analyzeResp = FindById(responses, 2);

        Assert.False(analyzeResp.TryGetProperty("error", out _),
            $"analyzeDocument returned error: {analyzeResp}");
        Assert.True(analyzeResp.TryGetProperty("result", out var result));

        Assert.Equal("lsp-document-analysis", result.GetProperty("kind").GetString());
        Assert.Equal("1", result.GetProperty("schema_version").GetString());
        Assert.Equal("csharp", result.GetProperty("kit_id").GetString());
        Assert.Equal("file:///project/FloorFixture.cs", result.GetProperty("uri").GetString());
        Assert.Equal("FloorFixture.cs", result.GetProperty("file").GetString());
        AssertBlake3Cid(result.GetProperty("document_cid").GetString());
        AssertBlake3Cid(result.GetProperty("protocol_catalog_cid").GetString());
        Assert.Equal(JsonValueKind.Array, result.GetProperty("entries").ValueKind);
        Assert.Equal(0, result.GetProperty("statuses").GetArrayLength());
        Assert.Equal(JsonValueKind.Null, result.GetProperty("project").ValueKind);

        var diagnostics = result.GetProperty("diagnostics").EnumerateArray().ToList();
        var diagnostic = Assert.Single(diagnostics,
            d => d.GetProperty("code").GetString() == "provekit.lsp.implication_failed");
        Assert.Equal("error", diagnostic.GetProperty("severity").GetString());
        Assert.Equal("forward-propagation", diagnostic.GetProperty("producer").GetString());
        Assert.Equal("csharp", diagnostic.GetProperty("kit_id").GetString());
        AssertBlake3Cid(diagnostic.GetProperty("protocol_catalog_cid").GetString());

        var expected = PositionOf(FloorFixtureSource, "checkPositive(-1)");
        var range = diagnostic.GetProperty("range");
        Assert.Equal(expected.Line, range.GetProperty("start_line").GetInt32());
        Assert.Equal(expected.Col, range.GetProperty("start_col").GetInt32());
        Assert.Equal("checkPositive", diagnostic.GetProperty("data").GetProperty("callee").GetString());
    }

    [Fact]
    public async Task Parse_DeclarationsIsArray()
    {
        var responses = await RunLsp(BuildSession());
        var parseResp = FindById(responses, 2);

        Assert.False(parseResp.TryGetProperty("error", out _),
            $"parse returned error: {parseResp}");
        Assert.True(parseResp.TryGetProperty("result", out var result));
        Assert.Equal(JsonValueKind.Array, result.GetProperty("declarations").ValueKind);
    }

    [Fact]
    public async Task Parse_CallEdgesIsArray()
    {
        var responses = await RunLsp(BuildSession());
        var parseResp = FindById(responses, 2);

        Assert.True(parseResp.TryGetProperty("result", out var result));
        Assert.Equal(JsonValueKind.Array, result.GetProperty("callEdges").ValueKind);
    }

    [Fact]
    public async Task Parse_EmitsSameLanguageCallEdgeLocus()
    {
        const string source =
            "public class C {\n" +
            "  //provekit:contract\n" +
            "  public static int AddOne(int x) { return x + 1; }\n" +
            "  //provekit:contract\n" +
            "  public static int CallAddOne(int x) { return AddOne(x); }\n" +
            "}\n";

        var responses = await RunLsp(BuildSession(source, "call-edge.cs"));
        var parseResp = FindById(responses, 2);

        Assert.False(parseResp.TryGetProperty("error", out _),
            $"parse returned error: {parseResp}");
        Assert.True(parseResp.TryGetProperty("result", out var result));

        var callEdges = result.GetProperty("callEdges").EnumerateArray().ToList();
        var edge = Assert.Single(callEdges,
            e => e.GetProperty("targetSymbol").GetString() == "csharp-kit:AddOne");

        Assert.Equal("call-edge", edge.GetProperty("kind").GetString());
        Assert.StartsWith("blake3-512:", edge.GetProperty("sourceContractCid").GetString());
        Assert.StartsWith("blake3-512:", edge.GetProperty("targetContractCid").GetString());

        var locus = edge.GetProperty("callSiteLocus");
        Assert.Equal("call-edge.cs", locus.GetProperty("file").GetString());
        Assert.Equal(5, locus.GetProperty("line").GetInt32());
        Assert.True(locus.GetProperty("column").GetInt32() > 0);
    }

    [Fact]
    public async Task Parse_WarningsIsArray()
    {
        var responses = await RunLsp(BuildSession());
        var parseResp = FindById(responses, 2);

        Assert.True(parseResp.TryGetProperty("result", out var result));
        Assert.Equal(JsonValueKind.Array, result.GetProperty("warnings").ValueKind);
    }

    [Fact]
    public async Task Parse_DeclarationsContainContracts()
    {
        var responses = await RunLsp(BuildSession());
        var parseResp = FindById(responses, 2);

        Assert.True(parseResp.TryGetProperty("result", out var result));
        var decls = result.GetProperty("declarations").EnumerateArray().ToList();
        Assert.True(decls.Count >= 1, "Expected at least one declaration from annotated fixture");
        foreach (var d in decls)
        {
            Assert.Equal(JsonValueKind.Object, d.ValueKind);
            Assert.Equal("contract", d.GetProperty("kind").GetString());
        }
    }

    [Fact]
    public async Task Parse_DeclarationsHaveNameField()
    {
        var responses = await RunLsp(BuildSession());
        var parseResp = FindById(responses, 2);

        Assert.True(parseResp.TryGetProperty("result", out var result));
        foreach (var d in result.GetProperty("declarations").EnumerateArray())
        {
            Assert.True(d.TryGetProperty("name", out _),
                $"declaration missing 'name': {d}");
        }
    }

    [Fact]
    public async Task Parse_EmptySourceReturnsEmptyArrays()
    {
        var ndjson = BuildSession(source: "// no contracts here\n");
        var responses = await RunLsp(ndjson);
        var parseResp = FindById(responses, 2);

        Assert.True(parseResp.TryGetProperty("result", out var result));
        Assert.Equal(0, result.GetProperty("declarations").GetArrayLength());
        Assert.Equal(0, result.GetProperty("callEdges").GetArrayLength());
    }

    [Fact]
    public async Task Parse_ByteDeterminism()
    {
        var ndjson = BuildSession();
        var run1 = await RunLsp(ndjson);
        var run2 = await RunLsp(ndjson);

        var parse1 = FindById(run1, 2);
        var parse2 = FindById(run2, 2);

        // Normalize via round-trip through sorted-keys serialization.
        var opts = new JsonSerializerOptions { WriteIndented = false };
        var s1 = JsonSerializer.Serialize(parse1, opts);
        var s2 = JsonSerializer.Serialize(parse2, opts);
        Assert.Equal(s1, s2);
    }

    [Fact]
    public async Task Parse_UnknownLanguageParam_IsIgnored()
    {
        // The C# plugin ignores unknown params keys; a 'language' field that
        // isn't C# doesn't cause an error (unlike the Python plugin which
        // errors on a non-matching language). This test asserts the binary
        // doesn't crash when extra fields are present.
        var msgs = new object[]
        {
            new { jsonrpc = "2.0", id = 1, method = "initialize", @params = new { } },
            new { jsonrpc = "2.0", id = 2, method = "parse",
                  @params = new { path = "f.cs", source = "// empty", language = "csharp" } },
            new { jsonrpc = "2.0", id = 3, method = "shutdown" },
        };
        var ndjson = string.Join("\n", msgs.Select(m => JsonSerializer.Serialize(m))) + "\n";
        var responses = await RunLsp(ndjson);
        var parseResp = FindById(responses, 2);
        Assert.True(parseResp.TryGetProperty("result", out _),
            "Expected result (not error) when extra 'language' param is present");
    }

    private static (int Line, int Col) PositionOf(string source, string needle)
    {
        var offset = source.IndexOf(needle, StringComparison.Ordinal);
        Assert.True(offset >= 0, $"needle not found: {needle}");

        var line = 1;
        var col = 0;
        for (var i = 0; i < offset; i++)
        {
            if (source[i] == '\n')
            {
                line++;
                col = 0;
            }
            else
            {
                col++;
            }
        }
        return (line, col);
    }

    private static void AssertBlake3Cid(string? value)
    {
        Assert.NotNull(value);
        Assert.StartsWith("blake3-512:", value);
        Assert.Equal("blake3-512:".Length + 128, value.Length);
        foreach (var c in value["blake3-512:".Length..])
        {
            Assert.True(c is >= '0' and <= '9' or >= 'a' and <= 'f',
                $"CID contains non-lowercase-hex character: {value}");
        }
    }
}
