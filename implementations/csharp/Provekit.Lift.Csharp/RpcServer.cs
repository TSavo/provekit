using System.Text.Json;
using System.Text.Json.Nodes;

namespace Provekit.Lift.Csharp;

public static class RpcServer
{
    public const string Surface = "csharp";
    public const string Dialect = "csharp-source";

    public static void Run()
    {
        var lifter = new CsharpLifter();
        var compiler = new CsharpCompiler();
        string? line;
        while ((line = Console.In.ReadLine()) is not null)
        {
            line = line.Trim();
            if (string.IsNullOrEmpty(line)) continue;

            JsonNode? response;
            try
            {
                var parsed = JsonNode.Parse(line);
                if (parsed is null)
                {
                    response = Error(null, -32700, "PARSE_ERROR: null request");
                    Console.Out.WriteLine(response.ToJsonString());
                    Console.Out.Flush();
                    continue;
                }
                var request = parsed.AsObject();
                response = Dispatch(request, lifter, compiler);
            }
            catch (JsonException ex)
            {
                response = Error(null, -32700, $"PARSE_ERROR: {ex.Message}");
            }
            catch (InvalidOperationException ex)
            {
                response = Error(null, -32600, $"INVALID_REQUEST: {ex.Message}");
            }

            if (response is not null)
            {
                Console.Out.WriteLine(response.ToJsonString());
                Console.Out.Flush();
            }
        }
    }

    private static JsonObject Dispatch(JsonObject request, CsharpLifter lifter, CsharpCompiler compiler)
    {
        var id = request["id"];
        var methodNode = request["method"];
        var method = methodNode is not null ? methodNode.GetValue<string>() : "";

        return method switch
        {
            "initialize" => Initialize(id),
            "lift" => LiftRpc(id, request["params"] as JsonObject, lifter),
            "compile" => CompileRpc(id, request["params"] as JsonObject, compiler),
            "provekit.plugin.recognize" => RecognizeRpc(id, request["params"] as JsonObject),
            "shutdown" => new JsonObject { ["jsonrpc"] = "2.0", ["id"] = id?.DeepClone(), ["result"] = null },
            _ => Error(id, -32601, $"METHOD_NOT_FOUND: {method}"),
        };
    }

    public static JsonObject DispatchForTest(JsonObject request) =>
        Dispatch(request, new CsharpLifter(), new CsharpCompiler());

    private static JsonObject Initialize(JsonNode? id)
    {
        return new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone(),
            ["result"] = new JsonObject
            {
                ["name"] = "provekit-lift-csharp",
                ["version"] = "0.1.0",
                ["protocol_version"] = "pep/1.7.0",
                ["capabilities"] = new JsonObject
                {
                    ["authoring_surfaces"] = new JsonArray { Surface, Dialect },
                    ["ir_version"] = "v1.1.0",
                    ["emits_signed_mementos"] = false,
                },
            },
        };
    }

    private static JsonObject LiftRpc(JsonNode? id, JsonObject? paramsObj, CsharpLifter lifter)
    {
        if (paramsObj is null)
            return Error(id, -32602, "params required");

        var surface = paramsObj["surface"]?.GetValue<string>() ?? Surface;
        if (surface != Surface && surface != Dialect)
            return Error(id, 1003, $"SURFACE_NOT_SUPPORTED: {surface}");

        var sourcePathsNode = paramsObj["source_paths"];
        JsonArray? sourcePaths;
        try { sourcePaths = sourcePathsNode?.AsArray(); } catch { sourcePaths = null; }
        if (sourcePaths is null || sourcePaths.Count == 0)
            return Error(id, -32602, "source_paths must be a non-empty array of strings");

        var workspaceRoot = paramsObj["workspace_root"]?.GetValue<string>() ?? ".";

        var paths = sourcePaths.Select(p => p?.GetValue<string>() ?? "").Where(p => !string.IsNullOrEmpty(p)).ToList();
        if (paths.Count == 0)
            return Error(id, -32602, "source_paths must be a non-empty array of strings");

        try
        {
            var result = lifter.LiftPaths(workspaceRoot, paths);
            return LiftSuccessResponse(id, result);
        }
        catch (Exception ex)
        {
            return Error(id, -32603, ex.Message);
        }
    }

    private static JsonObject CompileRpc(JsonNode? id, JsonObject? paramsObj, CsharpCompiler compiler)
    {
        if (paramsObj is null)
            return Error(id, -32602, "params required");

        if (!paramsObj.ContainsKey("ir") || paramsObj["ir"] is null)
            return Error(id, -32602, "ir required");
        var ir = paramsObj["ir"];

        try
        {
            var result = compiler.Compile(ir);
            return new JsonObject
            {
                ["jsonrpc"] = "2.0",
                ["id"] = id?.DeepClone(),
                ["result"] = new JsonObject
                {
                    ["kind"] = "compiled-formula",
                    ["body"] = result,
                },
            };
        }
        catch (Exception ex)
        {
            return Error(id, -32603, ex.Message);
        }
    }

    private static JsonObject RecognizeRpc(JsonNode? id, JsonObject? paramsObj)
    {
        if (paramsObj is null)
            return Error(id, -32602, "params required");

        var sourcePathsNode = paramsObj["source_paths"];
        JsonArray? sourcePaths;
        try { sourcePaths = sourcePathsNode?.AsArray(); } catch { sourcePaths = null; }
        if (sourcePaths is null || sourcePaths.Count == 0)
            return Error(id, -32602, "source_paths must be a non-empty array of strings");

        var projectRoot = paramsObj["project_root"]?.GetValue<string>() ?? ".";
        var paths = sourcePaths.Select(p => p?.GetValue<string>() ?? "")
            .Where(p => !string.IsNullOrEmpty(p))
            .ToList();
        if (paths.Count == 0)
            return Error(id, -32602, "source_paths must be a non-empty array of strings");

        try
        {
            var directTemplates = BindingTemplatesFromParams(paramsObj);
            var templates = directTemplates.Count > 0
                ? directTemplates
                : CsharpBindingTemplateResolver.ResolveFromProject(projectRoot);
            var result = CsharpRecognizer.RecognizePaths(projectRoot, paths, templates);
            return new JsonObject
            {
                ["jsonrpc"] = "2.0",
                ["id"] = id?.DeepClone(),
                ["result"] = result,
            };
        }
        catch (Exception ex)
        {
            return Error(id, -32603, ex.Message);
        }
    }

    private static IReadOnlyList<JsonObject> BindingTemplatesFromParams(JsonObject paramsObj)
    {
        if (paramsObj["binding_templates"] is not JsonArray directTemplates)
            return Array.Empty<JsonObject>();

        var templates = new List<JsonObject>();
        foreach (var template in directTemplates)
        {
            if (template is JsonObject obj) templates.Add(obj);
        }

        return templates;
    }

    private static JsonObject LiftSuccessResponse(JsonNode? id, LiftResult result)
    {
        return new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone(),
            ["result"] = new JsonObject
            {
                ["kind"] = "ir-document",
                ["ir"] = JsonSerializer.SerializeToNode(result.Declarations),
                ["callEdges"] = JsonSerializer.SerializeToNode(result.CallEdges),
                ["diagnostics"] = JsonSerializer.SerializeToNode(result.Diagnostics),
                ["opacityReport"] = JsonSerializer.SerializeToNode(result.OpacityReport),
                ["refusals"] = JsonSerializer.SerializeToNode(result.Refusals),
            },
        };
    }

    private static JsonObject Error(JsonNode? id, int code, string message)
    {
        return new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone(),
            ["error"] = new JsonObject
            {
                ["code"] = code,
                ["message"] = message,
            },
        };
    }
}
