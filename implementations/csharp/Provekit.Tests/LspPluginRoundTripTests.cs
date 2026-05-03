// SPDX-License-Identifier: Apache-2.0
//
// LSP plugin round-trip test (#221).
//
// Spawns `dotnet run --project ../Provekit.Lsp.Plugin -- --rpc` (the C# LSP
// plugin binary, `provekit-lsp-csharp`) and drives the NDJSON-over-stdio
// plugin protocol end to end:
//
//   1. initialize -> name/version/capabilities
//   2. parse      -> declarations + warnings
//   3. shutdown   -> null result, clean exit
//
// This is the single test that proves the binary actually speaks the protocol.

using System.Diagnostics;
using System.Text.Json;
using Xunit;

namespace Provekit.Tests;

public class LspPluginRoundTripTests
{
    // Resolve the precompiled plugin DLL via the ProjectReference declared in
    // Provekit.Tests.csproj. The test harness builds Provekit.Lsp.Plugin into
    // its own bin\<Configuration>\<TargetFramework>\ before running tests, so
    // we walk up from `bin/<Configuration>/<TargetFramework>/` (= AppContext
    // .BaseDirectory) to the sibling `Provekit.Lsp.Plugin/bin/<Configuration>
    // /<TargetFramework>/Provekit.Lsp.Plugin.dll`. This is faster than
    // `dotnet run --project ...` (no per-test MSBuild restore) and matches
    // the deployed binary the LSP coordinator would actually spawn.
    private static readonly string PluginDll = ResolvePluginDll();

    private static string ResolvePluginDll()
    {
        // bin/<Config>/<TFM>/  →  ../<Config>/<TFM> step is a no-op; we need
        // ../../../../Provekit.Lsp.Plugin/bin/<Config>/<TFM>/...dll
        var baseDir = AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar);
        // baseDir ends in <TFM>; parent is <Config>; grandparent is bin; great is Provekit.Tests dir.
        var tfm = Path.GetFileName(baseDir);
        var config = Path.GetFileName(Path.GetDirectoryName(baseDir));
        var testsDir = Path.GetDirectoryName(Path.GetDirectoryName(Path.GetDirectoryName(baseDir)))!;
        var solutionDir = Path.GetDirectoryName(testsDir)!;
        return Path.Combine(solutionDir,
            "Provekit.Lsp.Plugin", "bin", config!, tfm!,
            "provekit-lsp-csharp.dll");
    }

    private static Process SpawnPlugin()
    {
        if (!File.Exists(PluginDll))
        {
            throw new FileNotFoundException(
                $"plugin DLL not found at {PluginDll}; ensure ProjectReference to Provekit.Lsp.Plugin builds before tests run.");
        }
        var psi = new ProcessStartInfo
        {
            FileName = "dotnet",
            ArgumentList = { PluginDll, "--rpc" },
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        var proc = Process.Start(psi)!;
        return proc;
    }

    private static JsonElement Exchange(Process proc, object payload)
    {
        var line = JsonSerializer.Serialize(payload);
        proc.StandardInput.WriteLine(line);
        proc.StandardInput.Flush();
        var resp = proc.StandardOutput.ReadLine();
        Assert.False(string.IsNullOrEmpty(resp),
            $"plugin closed stdout; stderr={proc.StandardError.ReadToEnd()}");
        return JsonDocument.Parse(resp!).RootElement;
    }

    [Fact]
    public void RoundTrip_Initialize_Parse_Shutdown()
    {
        var proc = SpawnPlugin();
        try
        {
            // 1. initialize ------------------------------------------------
            var init = Exchange(proc, new
            {
                jsonrpc = "2.0",
                id = 1,
                method = "initialize",
                @params = new { },
            });
            Assert.Equal("2.0", init.GetProperty("jsonrpc").GetString());
            Assert.Equal(1, init.GetProperty("id").GetInt32());
            Assert.True(init.TryGetProperty("result", out var initResult),
                $"initialize returned error: {init}");
            Assert.Equal("provekit-lsp-csharp",
                initResult.GetProperty("name").GetString());
            var version = initResult.GetProperty("version").GetString();
            Assert.False(string.IsNullOrEmpty(version));
            var caps = initResult.GetProperty("capabilities");
            var capsList = caps.EnumerateArray()
                .Select(c => c.GetString())
                .ToList();
            Assert.Contains("parse", capsList);

            // 2. parse -----------------------------------------------------
            const string sample = "// //provekit:contract\npublic class Adder {\n    public int Add(int a, int b) => a + b;\n}\n";
            var parse = Exchange(proc, new
            {
                jsonrpc = "2.0",
                id = 2,
                method = "parse",
                @params = new { path = "Sample.cs", source = sample },
            });
            Assert.Equal(2, parse.GetProperty("id").GetInt32());
            Assert.True(parse.TryGetProperty("result", out var parseResult),
                $"parse returned error: {parse}");
            Assert.True(parseResult.TryGetProperty("declarations", out _),
                $"parse result missing `declarations`: {parseResult}");
            Assert.True(parseResult.TryGetProperty("warnings", out var warnings),
                $"parse result missing `warnings`: {parseResult}");
            Assert.Equal(JsonValueKind.Array, warnings.ValueKind);

            // 3. shutdown --------------------------------------------------
            var shut = Exchange(proc, new
            {
                jsonrpc = "2.0",
                id = 3,
                method = "shutdown",
            });
            Assert.Equal(3, shut.GetProperty("id").GetInt32());
            Assert.Equal(JsonValueKind.Null,
                shut.GetProperty("result").ValueKind);

            proc.StandardInput.Close();
            Assert.True(proc.WaitForExit(15_000),
                "plugin did not exit after shutdown");
            Assert.Equal(0, proc.ExitCode);
        }
        finally
        {
            if (!proc.HasExited)
            {
                try { proc.Kill(true); } catch { }
            }
            proc.Dispose();
        }
    }

    [Fact]
    public void UnknownMethod_ReturnsJsonRpcError()
    {
        var proc = SpawnPlugin();
        try
        {
            Exchange(proc, new
            {
                jsonrpc = "2.0",
                id = 1,
                method = "initialize",
                @params = new { },
            });
            var bad = Exchange(proc, new
            {
                jsonrpc = "2.0",
                id = 2,
                method = "no_such_method",
            });
            Assert.True(bad.TryGetProperty("error", out var error));
            Assert.Equal(-32601, error.GetProperty("code").GetInt32());

            var shut = Exchange(proc, new
            {
                jsonrpc = "2.0",
                id = 3,
                method = "shutdown",
            });
            Assert.Equal(JsonValueKind.Null,
                shut.GetProperty("result").ValueKind);
            proc.StandardInput.Close();
            proc.WaitForExit(15_000);
        }
        finally
        {
            if (!proc.HasExited)
            {
                try { proc.Kill(true); } catch { }
            }
            proc.Dispose();
        }
    }
}
