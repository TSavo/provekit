// SPDX-License-Identifier: Apache-2.0

using System.Text.Json;
using System.Text.Json.Nodes;
using Provekit.IR;

namespace Provekit.Lift.CLR;

public static class RpcProtocol
{
    public static JsonDocument LiftSuccessResponse(int id, LiftedClrDocument lifted) =>
        JsonDocument.Parse(LiftSuccessResponseJson(JsonValue.Create(id), lifted));

    public static string LiftSuccessResponseJson(JsonNode? id, LiftedClrDocument lifted)
    {
        var ir = JsonNode.Parse(Serialize.MarshalDeclarations(lifted.Contracts))!.AsArray();
        var diagnostics = new JsonArray();
        foreach (var diagnostic in lifted.Diagnostics)
        {
            diagnostics.Add(diagnostic);
        }

        var response = new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone() ?? JsonValue.Create((object?)null),
            ["result"] = new JsonObject
            {
                ["kind"] = "ir-document",
                ["ir"] = ir,
                ["diagnostics"] = diagnostics,
                ["callEdges"] = new JsonArray(),
                ["opacityReport"] = new JsonArray(),
                ["refusals"] = new JsonArray(),
            },
        };
        return response.ToJsonString(JsonSerializerOptions.Web);
    }

    public static string InitializeResponseJson(JsonNode? id)
    {
        var response = new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone() ?? JsonValue.Create((object?)null),
            ["result"] = new JsonObject
            {
                ["name"] = "provekit-lift-clr-bytecode",
                ["version"] = "0.1.0",
                ["protocol_version"] = "provekit-lift/1",
                ["capabilities"] = new JsonObject
                {
                    ["authoring_surfaces"] = new JsonArray { "clr-bytecode", "cil" },
                    ["ir_version"] = "v1.1.0",
                    ["emits_signed_mementos"] = false,
                },
            },
        };
        return response.ToJsonString(JsonSerializerOptions.Web);
    }

    public static string NullResponseJson(JsonNode? id)
    {
        var response = new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone() ?? JsonValue.Create((object?)null),
            ["result"] = JsonValue.Create((object?)null),
        };
        return response.ToJsonString(JsonSerializerOptions.Web);
    }

    public static string ErrorResponseJson(JsonNode? id, int code, string message)
    {
        var response = new JsonObject
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id?.DeepClone() ?? JsonValue.Create((object?)null),
            ["error"] = new JsonObject
            {
                ["code"] = code,
                ["message"] = message,
            },
        };
        return response.ToJsonString(JsonSerializerOptions.Web);
    }
}
