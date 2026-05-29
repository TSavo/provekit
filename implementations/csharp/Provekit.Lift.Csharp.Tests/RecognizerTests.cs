using System.Text.Json.Nodes;
using Provekit.Lift.Csharp;
using Xunit;

public class CsharpRecognizerTests
{
    [Fact]
    public void RecognizeMatchesExactAlphaEquivalentCsharpBody()
    {
        var binding = BindingTemplateFromShim();
        var user = @"
class App {
static Response Send(string uri, Headers h) {
    return client.Execute(uri, h);
}
}";

        var response = CsharpRecognizer.RecognizeText(user, "src/User.cs", new[] { binding });

        var tag = Assert.Single(response["tags"]!.AsArray());
        Assert.Equal("src/User.cs", tag!["file"]?.GetValue<string>());
        Assert.Equal("Send", tag["function_name"]?.GetValue<string>());
        Assert.Equal("concept:http-request", tag["concept_name"]?.GetValue<string>());
        Assert.Equal("csharp-http-shim", tag["library_tag"]?.GetValue<string>());
        Assert.Equal("concept:family:http", tag["family"]?.GetValue<string>());
        Assert.Equal(binding["template_cid"]!.GetValue<string>(), tag["template_cid"]?.GetValue<string>());
        Assert.Equal(binding["contract_cid"]!.GetValue<string>(), tag["contract_cid"]?.GetValue<string>());
        Assert.Equal("exact", tag["match_tier"]?.GetValue<string>());
        Assert.Equal(3, tag["span"]!["start_line"]?.GetValue<int>());
        Assert.Equal(5, tag["span"]!["end_line"]?.GetValue<int>());

        var paramBindings = tag["param_bindings"]!.AsArray();
        Assert.Equal(1, paramBindings[0]!["index"]?.GetValue<int>());
        Assert.Equal("uri", paramBindings[0]!["source_text"]?.GetValue<string>());
        Assert.Equal(2, paramBindings[1]!["index"]?.GetValue<int>());
        Assert.Equal("h", paramBindings[1]!["source_text"]?.GetValue<string>());
    }

    [Fact]
    public void RecognizeDoesNotMatchDifferentCsharpBody()
    {
        var binding = BindingTemplateFromShim();
        var user = @"
class App {
static Response Send(string uri, Headers h) {
    return other.Execute(uri, h);
}
}";

        var response = CsharpRecognizer.RecognizeText(user, "src/User.cs", new[] { binding });

        Assert.Empty(response["tags"]!.AsArray());
    }

    [Fact]
    public void RpcRecognizeDispatchResolvesTemplatesFromCsharpProof()
    {
        var root = Path.Combine(Path.GetTempPath(), $"provekit-csharp-recognize-{Guid.NewGuid():N}");
        Directory.CreateDirectory(root);
        try
        {
            var sourcePath = Path.Combine(root, "User.cs");
            File.WriteAllText(sourcePath, @"
class App {
static Response Send(string uri, Headers h) {
    return client.Execute(uri, h);
}
}");
            var request = new JsonObject
            {
                ["jsonrpc"] = "2.0",
                ["id"] = 7,
                ["method"] = "provekit.plugin.recognize",
                ["params"] = new JsonObject
                {
                    ["project_root"] = root,
                    ["source_paths"] = new JsonArray { "User.cs" },
                },
            };
            File.WriteAllText(Path.Combine(root, "csharp-shim.proof"), ProofJsonFromShim());

            var response = RpcServer.DispatchForTest(request);

            Assert.Equal("2.0", response["jsonrpc"]?.GetValue<string>());
            Assert.Equal(7, response["id"]?.GetValue<int>());
            var tag = Assert.Single(response["result"]!["tags"]!.AsArray());
            Assert.Equal("User.cs", tag!["file"]?.GetValue<string>());
            Assert.Equal("Send", tag["function_name"]?.GetValue<string>());
            Assert.Equal("exact", tag["match_tier"]?.GetValue<string>());
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    private static JsonObject BindingTemplateFromShim()
    {
        var entry = SugarEntryFromShim();
        var bodySource = Assert.IsType<JsonObject>(entry["body_source"]);
        return new JsonObject
        {
            ["concept_name"] = entry["concept_name"]!.DeepClone(),
            ["library_tag"] = entry["target_library_tag"]!.DeepClone(),
            ["family"] = entry["family"]!.DeepClone(),
            ["ast_template"] = bodySource["ast_template"]!.DeepClone(),
            ["template_cid"] = bodySource["template_cid"]!.GetValue<string>(),
            ["param_names"] = bodySource["param_names"]!.DeepClone(),
            ["contract_cid"] = entry["contract_cid"]!.DeepClone(),
        };
    }

    private static string ProofJsonFromShim()
    {
        var entry = SugarEntryFromShim();
        var proof = new JsonObject
        {
            ["ir"] = new JsonArray
            {
                new JsonObject
                {
                    ["schemaVersion"] = "1",
                    ["header"] = new JsonObject { ["kind"] = "library-sugar-binding-entry" },
                    ["body"] = entry,
                },
            },
        };
        return proof.ToJsonString();
    }

    private static JsonObject SugarEntryFromShim()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        lifter.LiftSource(@"
class Shim {
static Response Fetch(string url, Headers headers) {
    return client.Execute(url, headers);
}
}", "shim.cs", result);

        var contract = Assert.Single(result.Declarations);
        var bodySource = Assert.IsType<JsonObject>(contract["body_source"]);
        return new JsonObject
        {
            ["kind"] = "library-sugar-binding-entry",
            ["concept_name"] = "concept:http-request",
            ["target_language"] = "csharp",
            ["target_library_tag"] = "csharp-http-shim",
            ["family"] = "concept:family:http",
            ["source_function_name"] = "Fetch",
            ["param_names"] = bodySource["param_names"]!.DeepClone(),
            ["body_source"] = bodySource.DeepClone(),
            ["contract_cid"] = "blake3-512:" + new string('c', 128),
        };
    }
}
