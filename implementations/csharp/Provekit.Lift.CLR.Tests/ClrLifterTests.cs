// SPDX-License-Identifier: Apache-2.0

using System.Text.Json;
using Provekit.IR;
using Provekit.Lift.CLR;
using Xunit;

namespace Provekit.Lift.CLR.Tests;

public static class ClrFixture
{
    public static int AddOne(int value) => value + 1;
}

public class ClrLifterTests
{
    [Fact]
    public void LiftAssemblyUsesClrMetadataAndInstructionStream()
    {
        var lifted = ClrAssemblyLifter.LiftAssembly(typeof(ClrFixture).Assembly.Location);

        var contract = Assert.Single(lifted.Contracts, c =>
            c.Name.StartsWith("clr:Provekit.Lift.CLR.Tests.ClrFixture::AddOne#", StringComparison.Ordinal));
        var json = Serialize.MarshalDeclarations(new[] { contract });

        Assert.Contains("ldarg.0", json);
        Assert.Contains("ldc.i4.1", json);
        Assert.Contains("add", json);
        Assert.Contains("ret", json);
    }

    [Fact]
    public void LiftResponseIsCanonicalIrDocumentSurface()
    {
        var lifted = ClrAssemblyLifter.LiftAssembly(typeof(ClrFixture).Assembly.Location);
        var response = RpcProtocol.LiftSuccessResponse(7, lifted);

        Assert.Equal("ir-document", response.RootElement.GetProperty("result").GetProperty("kind").GetString());
        Assert.True(response.RootElement.GetProperty("result").GetProperty("ir").GetArrayLength() > 0);
        Assert.False(response.RootElement.GetProperty("result").TryGetProperty("declarations", out _));
    }
}
