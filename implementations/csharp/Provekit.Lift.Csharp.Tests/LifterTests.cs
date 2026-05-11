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
}
