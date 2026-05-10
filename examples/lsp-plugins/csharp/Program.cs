// ProvekIt LSP Language Plugin: C#
//
// A standalone binary that speaks provekit-lsp-plugin/1 over stdio.
// Parses C# source files and extracts provekit annotations.
//
// Usage: dotnet run -- --rpc
//
// To use this plugin, add to `.provekit/config.toml`:
//   [[language]]
//   name = "csharp"
//   extensions = [".cs"]
//   plugin = "provekit-lsp-csharp"
//
// Build: dotnet publish -c Release -o ./out

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;

namespace ProvekitLspCSharp;

class Program
{
    static void Main(string[] args)
    {
        if (!args.Contains("--rpc"))
        {
            Console.Error.WriteLine("Usage: provekit-lsp-csharp --rpc");
            Environment.Exit(1);
        }

        var reImpl = new Regex(@"//\s*provekit:implement\s+([\w-]+)");
        var reContract = new Regex(@"//\s*provekit:contract");
        var reVerify = new Regex(@"//\s*provekit:verify");
        var reFn = new Regex(@"\b(?:public|private|protected|internal|static|\s)+\s*[\w<>\[\]]+\s+(\w+)\s*\(");

        var stdin = Console.In;
        var stdout = Console.Out;

        while (true)
        {
            var line = stdin.ReadLine();
            if (line == null) break;

            JsonElement req;
            try
            {
                req = JsonDocument.Parse(line).RootElement;
            }
            catch (Exception ex)
            {
                WriteResp(stdout, new { jsonrpc = "2.0", id = (object?)null, error = new { code = -32700, message = $"parse error: {ex.Message}" } });
                continue;
            }

            var id = req.TryGetProperty("id", out var idProp) ? (object?)idProp : null;
            var method = req.GetProperty("method").GetString() ?? "";

            switch (method)
            {
                case "initialize":
                    WriteResp(stdout, new { jsonrpc = "2.0", id, result = new { name = "provekit-lsp-csharp", version = "0.1.0", capabilities = Array.Empty<string>() } });
                    break;

                case "parse":
                    var text = req.GetProperty("params").GetProperty("text").GetString() ?? "";
                    var annotations = ParseCs(text, reImpl, reContract, reVerify, reFn);
                    WriteResp(stdout, new { jsonrpc = "2.0", id, result = new { annotations } });
                    break;

                case "shutdown":
                    WriteResp(stdout, new { jsonrpc = "2.0", id, result = (object?)null });
                    return;

                default:
                    WriteResp(stdout, new { jsonrpc = "2.0", id, error = new { code = -32601, message = $"unknown method: {method}" } });
                    break;
            }
        }
    }

    static void WriteResp(TextWriter writer, object resp)
    {
        writer.WriteLine(JsonSerializer.Serialize(resp));
        writer.Flush();
    }

    static List<Annotation> ParseCs(string text, Regex reImpl, Regex reContract, Regex reVerify, Regex reFn)
    {
        var annotations = new List<Annotation>();
        var lines = text.Split('\n');

        for (int i = 0; i < lines.Length; i++)
        {
            var line = lines[i];

            var implMatch = reImpl.Match(line);
            if (implMatch.Success)
            {
                var cid = implMatch.Groups[1].Value;
                var fnName = FindAhead(lines, i, reFn);
                annotations.Add(new Annotation { FunctionName = fnName, Kind = "implement", TargetCID = cid, Range = new Range { Start = new Position { Line = (uint)i, Character = 0 }, End = new Position { Line = (uint)(i + 1), Character = 0 } } });
            }

            if (reContract.IsMatch(line))
            {
                var fnName = FindAhead(lines, i, reFn);
                annotations.Add(new Annotation { FunctionName = fnName, Kind = "contract", Range = new Range { Start = new Position { Line = (uint)i, Character = 0 }, End = new Position { Line = (uint)(i + 1), Character = 0 } } });
            }

            if (reVerify.IsMatch(line))
            {
                var fnName = FindAhead(lines, i, reFn);
                annotations.Add(new Annotation { FunctionName = fnName, Kind = "verify", Range = new Range { Start = new Position { Line = (uint)i, Character = 0 }, End = new Position { Line = (uint)(i + 1), Character = 0 } } });
            }
        }

        return annotations;
    }

    static string FindAhead(string[] lines, int start, Regex re)
    {
        for (int j = start + 1; j < lines.Length && j < start + 10; j++)
        {
            var m = re.Match(lines[j]);
            if (m.Success) return m.Groups[1].Value;
        }
        return "unknown";
    }
}

class Annotation
{
    public string FunctionName { get; set; } = "";
    public string Kind { get; set; } = "";
    public string? TargetCID { get; set; }
    public Range Range { get; set; } = new();
}

class Range
{
    public Position Start { get; set; } = new();
    public Position End { get; set; } = new();
}

class Position
{
    public uint Line { get; set; }
    public uint Character { get; set; }
}
