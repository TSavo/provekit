using System.Text.Json.Nodes;
using Provekit.Lift.Csharp;
using Xunit;

public class CsharpLifterTests
{
    [Fact]
    public void LiftAddProducesFunctionContract()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = "class C { static int Add(int x, int y) { return x + y; } }";

        lifter.LiftSource(source, "test.cs", result);

        Assert.NotEmpty(result.Declarations);
        var contract = result.Declarations[0];
        Assert.Equal("M:C.Add(System.Int32,System.Int32)", contract["fnName"]?.GetValue<string>());
        Assert.Equal("function-contract", contract["kind"]?.GetValue<string>());
        Assert.Equal("1", contract["schemaVersion"]?.GetValue<string>());
    }

    [Fact]
    public void LiftIfStatementProducesConditionalContract()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = @"
class C {
static int Max(int a, int b) {
    if (a > b) {
        return a;
    } else {
        return b;
    }
}
}";

        lifter.LiftSource(source, "test.cs", result);

        Assert.NotEmpty(result.Declarations);
        var contract = result.Declarations[0];
        Assert.Equal("M:C.Max(System.Int32,System.Int32)", contract["fnName"]?.GetValue<string>());
    }

    [Fact]
    public void LiftWhileLoopProducesContract()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = @"
class C {
static int Factorial(int n) {
    int result = 1;
    while (n > 0) {
        result = result * n;
        n = n - 1;
    }
    return result;
}
}";

        lifter.LiftSource(source, "test.cs", result);

        Assert.NotEmpty(result.Declarations);
        var contract = result.Declarations[0];
        Assert.Equal("M:C.Factorial(System.Int32)", contract["fnName"]?.GetValue<string>());
    }

    [Fact]
    public void LiftMultipleMethodsProducesMultipleContracts()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = @"
class C {
static int Add(int x, int y) { return x + y; }
static int Sub(int x, int y) { return x - y; }
static int Mul(int x, int y) { return x * y; }
}";

        lifter.LiftSource(source, "test.cs", result);

        Assert.Equal(3, result.Declarations.Count);
        Assert.Equal("M:C.Add(System.Int32,System.Int32)", result.Declarations[0]["fnName"]?.GetValue<string>());
        Assert.Equal("M:C.Sub(System.Int32,System.Int32)", result.Declarations[1]["fnName"]?.GetValue<string>());
        Assert.Equal("M:C.Mul(System.Int32,System.Int32)", result.Declarations[2]["fnName"]?.GetValue<string>());
    }

    [Fact]
    public void LiftAndCompileRoundTrips()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = "class C { static int Add(int x, int y) { return x + y; } }";

        lifter.LiftSource(source, "test.cs", result);
        Assert.NotEmpty(result.Declarations);

        var contract = result.Declarations[0];
        var post = contract["post"];
        var bodyIr = post?["args"]?[1];
        Assert.NotNull(bodyIr);

        var compiler = new CsharpCompiler();
        var compiled = compiler.CompileBody(bodyIr);
        Assert.Contains("x + y", compiled);

        var reLifter = new CsharpLifter();
        var reResult = new LiftResult();
        reLifter.LiftSource($"class C {{ static int F() {{ {compiled} }} }}", "roundtrip.cs", reResult);
        Assert.NotEmpty(reResult.Declarations);
        Assert.Empty(reResult.Refusals);
        var reContract = reResult.Declarations[0];
        var rePost = reContract["post"];
        var reBody = rePost?["args"]?[1];
        Assert.NotNull(reBody);
        Assert.Equal(bodyIr!.ToJsonString(), reBody.ToJsonString());
    }

    [Fact]
    public void LiftedFunctionContractCarriesMaterializableBodySource()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = @"
class Shim {
static Response Fetch(string url, Headers headers) {
    return client.Execute(url, headers);
}
}";

        lifter.LiftSource(source, "shim.cs", result);

        var contract = Assert.Single(result.Declarations);
        var bodySource = Assert.IsType<JsonObject>(contract["body_source"]);
        Assert.Equal("return client.Execute(url, headers);", bodySource["body_text"]?.GetValue<string>());
        Assert.Equal("shim.cs", bodySource["file"]?.GetValue<string>());
        Assert.StartsWith("blake3-512:", bodySource["source_cid"]?.GetValue<string>());
        Assert.StartsWith("blake3-512:", bodySource["template_cid"]?.GetValue<string>());
        Assert.Equal(new[] { "url", "headers" }, bodySource["param_names"]!.AsArray().Select(p => p!.GetValue<string>()));

        var template = Assert.IsType<JsonObject>(bodySource["ast_template"]);
        Assert.Equal("block", template["kind"]?.GetValue<string>());
        var stmt = Assert.IsType<JsonObject>(template["stmts"]!.AsArray()[0]);
        Assert.Equal("return", stmt["kind"]?.GetValue<string>());
        var call = Assert.IsType<JsonObject>(stmt["expr"]);
        Assert.Equal("method_call", call["kind"]?.GetValue<string>());
        Assert.Equal("Execute", call["method"]?.GetValue<string>());
        var args = call["args"]!.AsArray();
        Assert.Equal("param_ref", args[0]!["kind"]?.GetValue<string>());
        Assert.Equal(1, args[0]!["index"]?.GetValue<int>());
        Assert.Equal("param_ref", args[1]!["kind"]?.GetValue<string>());
        Assert.Equal(2, args[1]!["index"]?.GetValue<int>());
    }

    [Fact]
    public void BodyTemplateCidIsStableUnderParameterRenaming()
    {
        static JsonObject BodySourceFor(string source)
        {
            var lifter = new CsharpLifter();
            var result = new LiftResult();
            lifter.LiftSource(source, "shim.cs", result);
            return Assert.IsType<JsonObject>(Assert.Single(result.Declarations)["body_source"]);
        }

        var bodyA = BodySourceFor(@"
class Shim {
static Response Fetch(string url, Headers headers) {
    return client.Execute(url, headers);
}
}");
        var bodyB = BodySourceFor(@"
class Shim {
static Response Fetch(string uri, Headers h) {
    return client.Execute(uri, h);
}
}");

        Assert.Equal(bodyA["ast_template"]!.ToJsonString(), bodyB["ast_template"]!.ToJsonString());
        Assert.Equal(bodyA["template_cid"]!.GetValue<string>(), bodyB["template_cid"]!.GetValue<string>());
        Assert.NotEqual(bodyA["body_text"]!.GetValue<string>(), bodyB["body_text"]!.GetValue<string>());
        Assert.NotEqual(bodyA["source_cid"]!.GetValue<string>(), bodyB["source_cid"]!.GetValue<string>());
    }

    [Fact]
    public void CompilerProducesValidCsharpExpression()
    {
        var compiler = new CsharpCompiler();
        var ir = new JsonObject
        {
            ["kind"] = "ctor",
            ["name"] = "csharp:add",
            ["args"] = new JsonArray
            {
                new JsonObject { ["kind"] = "var", ["name"] = "x" },
                new JsonObject { ["kind"] = "var", ["name"] = "y" },
            },
        };

        var compiled = compiler.CompileBody(ir);
        Assert.Contains("x + y", compiled);
    }

    [Fact]
    public void VoidMethodsAreRefused()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = "class C { static void DoNothing(int x) { return; } }";

        lifter.LiftSource(source, "test.cs", result);
        Assert.Empty(result.Declarations);
        Assert.NotEmpty(result.Refusals);
        Assert.Equal("unsupported-return-sort", result.Refusals[0].Kind);
    }

    [Fact]
    public void GuardThrowIsLiftedAsMethodPrecondition()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = @"
using System;
class C {
static int Inner(int y) {
    if (y < 5) throw new ArgumentOutOfRangeException(nameof(y));
    return y;
}
}";

        lifter.LiftSource(source, "test.cs", result);

        var contract = Assert.Single(result.Declarations,
            d => d["fnName"]?.GetValue<string>() == "M:C.Inner(System.Int32)");
        var pre = contract["pre"]!;
        var preJson = pre.ToJsonString();
        AssertGtePredicate(preJson);
        Assert.Contains("\"name\":\"y\"", preJson);
        Assert.Contains("\"value\":5", preJson);
    }

    [Fact]
    public void ComposeCallsitePreSubstitutesFormalToActual()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var source = @"
using System;
class C {
static int Inner(int y) {
    if (y < 5) throw new ArgumentOutOfRangeException(nameof(y));
    return y;
}
static int Outer(int x) {
    return Inner(x);
}
}";

        lifter.LiftSource(source, "test.cs", result);

        var outer = Assert.Single(result.Declarations,
            d => d["fnName"]?.GetValue<string>() == "M:C.Outer(System.Int32)");
        var outerPreJson = outer["pre"]!.ToJsonString();
        AssertGtePredicate(outerPreJson);
        Assert.Contains("\"name\":\"x\"", outerPreJson);
        Assert.Contains("\"value\":5", outerPreJson);
        Assert.DoesNotContain("\"name\":\"y\"", outerPreJson);

        var callsite = Assert.Single(result.Declarations,
            d => d["fnName"]?.GetValue<string>()?.Contains("::callsite-pre@", StringComparison.Ordinal) == true);
        var callsitePreJson = callsite["pre"]!.ToJsonString();
        Assert.Contains("\"kind\":\"forall\"", callsitePreJson);
        Assert.Contains("\"kind\":\"implies\"", callsitePreJson);
        Assert.Contains("\"name\":\"x\"", callsitePreJson);
        Assert.DoesNotContain("\"name\":\"y\"", callsitePreJson);

        Assert.Single(result.CallEdges);
        Assert.Equal("call-edge", result.CallEdges[0]["kind"]?.GetValue<string>());
        Assert.Equal("M:C.Inner(System.Int32)", result.CallEdges[0]["targetSymbol"]?.GetValue<string>());
    }

    [Fact]
    public void ProofEnvelopeSignSourceEmitsGuardedCallsiteImplication()
    {
        var lifter = new CsharpLifter();
        var result = new LiftResult();
        var path = "implementations/csharp/Provekit.ProofEnvelope/Sign.cs";
        var source = File.ReadAllText(FindRepoFile(path));

        lifter.LiftSource(source, path, result);

        var callsite = Assert.Single(result.Declarations,
            d =>
            {
                var name = d["fnName"]?.GetValue<string>() ?? "";
                return name.Contains("Ed25519SignString", StringComparison.Ordinal)
                       && name.Contains("Ed25519SignWithSeed", StringComparison.Ordinal)
                       && name.Contains("::callsite-pre@", StringComparison.Ordinal);
            });
        var preJson = callsite["pre"]!.ToJsonString();
        Assert.Contains("\"kind\":\"forall\"", preJson);
        Assert.Contains("\"kind\":\"implies\"", preJson);
        Assert.Contains("\"value\":32", preJson);
        Assert.Contains("\"value\":\"Length\"", preJson);
    }

    private static string FindRepoFile(string relativePath)
    {
        var dir = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (dir is not null)
        {
            var candidate = Path.Combine(dir.FullName, relativePath);
            if (File.Exists(candidate)) return candidate;
            dir = dir.Parent;
        }
        throw new FileNotFoundException($"Could not locate {relativePath}");
    }

    private static void AssertGtePredicate(string json)
    {
        Assert.True(
            json.Contains("\"name\":\"≥\"", StringComparison.Ordinal)
            || json.Contains("\"name\":\"\\u2265\"", StringComparison.Ordinal),
            $"expected ≥ predicate in {json}");
    }
}
