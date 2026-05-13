// SPDX-License-Identifier: Apache-2.0

using System.Text.Json.Nodes;

namespace Provekit.Lift.CLR;

public static class Program
{
    public static int Main(string[] args)
    {
        if (!args.Contains("--rpc"))
        {
            Console.Error.WriteLine("usage: provekit-lift-clr-bytecode --rpc");
            return 1;
        }

        RunRpc();
        return 0;
    }

    private static void RunRpc()
    {
        while (true)
        {
            var line = Console.ReadLine();
            if (line is null)
            {
                return;
            }
            if (string.IsNullOrWhiteSpace(line))
            {
                continue;
            }

            JsonNode? request;
            try
            {
                request = JsonNode.Parse(line);
            }
            catch
            {
                Console.WriteLine(RpcProtocol.ErrorResponseJson(null, -32700, "PARSE_ERROR"));
                continue;
            }

            var id = request?["id"];
            var method = request?["method"]?.GetValue<string>() ?? "";
            switch (method)
            {
                case "initialize":
                    Console.WriteLine(RpcProtocol.InitializeResponseJson(id));
                    break;
                case "lift":
                    HandleLift(id, request?["params"]);
                    break;
                case "shutdown":
                    Console.WriteLine(RpcProtocol.NullResponseJson(id));
                    return;
                default:
                    Console.WriteLine(RpcProtocol.ErrorResponseJson(id, -32601, $"METHOD_NOT_FOUND: {method}"));
                    break;
            }
        }
    }

    private static void HandleLift(JsonNode? id, JsonNode? parameters)
    {
        try
        {
            var surface = parameters?["surface"]?.GetValue<string>() ?? "clr-bytecode";
            if (surface is not ("clr-bytecode" or "cil"))
            {
                Console.WriteLine(RpcProtocol.ErrorResponseJson(id, 1003, $"SURFACE_NOT_SUPPORTED: {surface}"));
                return;
            }

            var workspaceRoot = parameters?["workspace_root"]?.GetValue<string>() ?? ".";
            var sourcePaths = parameters?["source_paths"]?.AsArray()
                .Select(item => item?.GetValue<string>())
                .Where(item => !string.IsNullOrEmpty(item))
                .Cast<string>()
                .ToArray() ?? Array.Empty<string>();
            if (sourcePaths.Length == 0)
            {
                Console.WriteLine(RpcProtocol.ErrorResponseJson(id, -32602, "source_paths must be a non-empty array of strings"));
                return;
            }

            var lifted = ClrAssemblyLifter.LiftPaths(workspaceRoot, sourcePaths);
            Console.WriteLine(RpcProtocol.LiftSuccessResponseJson(id, lifted));
        }
        catch (Exception ex)
        {
            Console.WriteLine(RpcProtocol.ErrorResponseJson(id, -32603, $"Lift failed: {ex.Message}"));
        }
    }
}
