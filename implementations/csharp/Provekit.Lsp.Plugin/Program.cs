// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-csharp — NDJSON LSP plugin for C#.
//
// Thin shell around Provekit.Lift.Core.SourceLifter (task #219). All
// lift orchestration (Roslyn compile, DataAnnotations lift, annotation
// scan, marshal) lives in the shared core library; the plugin here just
// wires JSON-RPC to that pipeline.
//
// Protocol:
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}

using System.Text.Json;
using Provekit.IR;
using Provekit.Lift.Core;

namespace Provekit.Lsp.Plugin;

partial class Program
{
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
                    Respond(id, """{"name":"provekit-lsp-csharp","version":"0.2.0","capabilities":["parse"]}""");
                    break;
                case "parse":
                    HandleParse(id, req);
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
