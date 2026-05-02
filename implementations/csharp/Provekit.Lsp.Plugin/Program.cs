// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-csharp — NDJSON LSP plugin for C#.
//
// Uses Microsoft.CodeAnalysis (Roslyn) to compile C# source in-process,
// then applies Provekit.Lift.DataAnnotations reflection-based lifting
// and Provekit.Lift.Linq LINQ-expression lifting to emit canonical IR.
//
// Protocol:
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}

using System.Reflection;
using System.Text.Json;
using System.Text.RegularExpressions;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Provekit.IR;
using Provekit.Lift.DataAnnotations;

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

        var decls = new List<ContractDecl>();

        // 1. Roslyn compilation → reflection → DataAnnotations lift
        try
        {
            var assembly = CompileToAssembly(source, path);
            if (assembly is not null)
            {
                foreach (var type in assembly.GetTypes())
                {
                    if (type.IsClass && !type.IsNestedPrivate && type.GetCustomAttribute<System.Runtime.CompilerServices.CompilerGeneratedAttribute>() is null)
                    {
                        var lifted = DataAnnotationsLift.LiftType(type);
                        decls.AddRange(lifted);
                    }
                }
            }
        }
        catch
        {
            // Compilation failed — fall through to annotation scan
        }

        // 2. Annotation scan for //provekit: comments
        decls.AddRange(ScanAnnotations(source));

        // 3. Marshal
        var jcs = decls.Count > 0
            ? Serialize.MarshalDeclarations(decls)
            : "[]";

        Console.WriteLine($"{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"declarations\":{jcs},\"warnings\":[]}}}}");
    }

    // ── Roslyn compilation pipeline ─────────────────────────────────

    static Assembly? CompileToAssembly(string source, string path)
    {
        var tree = CSharpSyntaxTree.ParseText(source, path: path);

        var references = new List<MetadataReference>
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.ComponentModel.DataAnnotations.RequiredAttribute).Assembly.Location),
            MetadataReference.CreateFromFile(Assembly.Load("System.Runtime").Location),
            MetadataReference.CreateFromFile(Assembly.Load("System.ComponentModel.Primitives").Location),
        };

        // Add netstandard / System.Linq for LINQ lift support
        var netstandard = AppDomain.CurrentDomain.GetAssemblies()
            .FirstOrDefault(a => a.GetName().Name == "netstandard");
        if (netstandard is not null)
            references.Add(MetadataReference.CreateFromFile(netstandard.Location));

        var compilation = CSharpCompilation.Create(
            "LiftAssembly",
            new[] { tree },
            references,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        using var ms = new MemoryStream();
        var result = compilation.Emit(ms);
        if (!result.Success) return null;

        ms.Seek(0, SeekOrigin.Begin);
        return Assembly.Load(ms.ToArray());
    }

    // ── Annotation scanning ──────────────────────────────────────

    [GeneratedRegex(@"//\s*provekit:\s*contract")]
    private static partial Regex ContractAnnotation();

    [GeneratedRegex(@"//\s*provekit:\s*implement\s+([\w-]+)")]
    private static partial Regex ImplementAnnotation();

    [GeneratedRegex(@"(?:public|private|protected|internal|static)\s+\w+(?:\<[^>]*\>)?\s+(\w+)\s*\(")]
    private static partial Regex FunctionSig();

    static List<ContractDecl> ScanAnnotations(string source)
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

    static string FindFn(string[] lines, int start)
    {
        for (int j = start + 1; j < lines.Length && j < start + 10; j++)
        {
            var m = FunctionSig().Match(lines[j]);
            if (m.Success) return m.Groups[1].Value;
        }
        return "";
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
