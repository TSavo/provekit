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

public static class OuterA
{
    public static class Inner
    {
        public static int Value() => 101;
    }
}

public static class OuterB
{
    public static class Inner
    {
        public static int Value() => 202;
    }
}

public class ClrLifterTests
{
    [Fact]
    public void LiftAssemblyUsesClrMetadataAndInstructionStream()
    {
        var lifted = ClrAssemblyLifter.LiftAssembly(typeof(ClrFixture).Assembly.Location);

        var contract = Assert.Single(lifted.Contracts, c =>
            c.Name.StartsWith(ContractPrefix(typeof(ClrFixture), nameof(ClrFixture.AddOne)), StringComparison.Ordinal));
        var json = Serialize.MarshalDeclarations(new[] { contract });

        Assert.Contains("ldarg.0", json);
        Assert.Contains("ldc.i4.1", json);
        Assert.Contains("add", json);
        Assert.Contains("ret", json);
    }

    [Fact]
    public void LiftAssemblyQualifiesContractNameWithAssemblyName()
    {
        var lifted = ClrAssemblyLifter.LiftAssembly(typeof(ClrFixture).Assembly.Location);

        var contract = Assert.Single(lifted.Contracts, c =>
            c.Name.Contains("ClrFixture::AddOne#", StringComparison.Ordinal));

        Assert.StartsWith(
            $"clr:{typeof(ClrFixture).Assembly.GetName().Name}!Provekit.Lift.CLR.Tests.ClrFixture::AddOne#",
            contract.Name,
            StringComparison.Ordinal);
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

    [Fact]
    public void LiftPathsKeepsSameNamedNestedTypesUnderDifferentDeclaringTypes()
    {
        var assemblyPath = typeof(OuterA.Inner).Assembly.Location;
        var workspaceRoot = Path.GetDirectoryName(assemblyPath)!;

        var lifted = ClrAssemblyLifter.LiftPaths(workspaceRoot, new[] { assemblyPath });

        Assert.Contains(lifted.Contracts, contract =>
            contract.Name.StartsWith(ContractPrefix(typeof(OuterA.Inner), nameof(OuterA.Inner.Value)), StringComparison.Ordinal));
        Assert.Contains(lifted.Contracts, contract =>
            contract.Name.StartsWith(ContractPrefix(typeof(OuterB.Inner), nameof(OuterB.Inner.Value)), StringComparison.Ordinal));
        Assert.DoesNotContain(lifted.Diagnostics, diagnostic =>
            diagnostic.Contains("duplicate CLR contract name", StringComparison.Ordinal));
    }

    [Fact]
    public void LiftPathsPrefersReleaseProjectOutputOverDebug()
    {
        var projectDir = CreateProjectOutputFixture(
            ("Debug", "net10.0", typeof(ClrAssemblyLifter).Assembly.Location),
            ("Release", "net10.0", typeof(ClrFixture).Assembly.Location));

        try
        {
            var lifted = ClrAssemblyLifter.LiftPaths(projectDir, new[] { "." });

            Assert.Contains(lifted.Contracts, contract =>
                contract.Name.StartsWith(ContractPrefix(typeof(ClrFixture), nameof(ClrFixture.AddOne)), StringComparison.Ordinal));
            Assert.DoesNotContain(lifted.Contracts, contract =>
                contract.Name.StartsWith(ContractPrefix(typeof(ClrAssemblyLifter), nameof(ClrAssemblyLifter.LiftAssembly)), StringComparison.Ordinal));
        }
        finally
        {
            Directory.Delete(projectDir, recursive: true);
        }
    }

    [Fact]
    public void LiftPathsResolvesProjectFileInputToBuiltOutput()
    {
        var projectDir = CreateProjectOutputFixture(
            ("Release", "net10.0", typeof(ClrFixture).Assembly.Location));
        var projectPath = Path.Combine(projectDir, "FixtureProject.csproj");

        try
        {
            var lifted = ClrAssemblyLifter.LiftPaths(projectDir, new[] { projectPath });

            Assert.Contains(lifted.Contracts, contract =>
                contract.Name.StartsWith(ContractPrefix(typeof(ClrFixture), nameof(ClrFixture.AddOne)), StringComparison.Ordinal));
        }
        finally
        {
            Directory.Delete(projectDir, recursive: true);
        }
    }

    [Fact]
    public void LiftPathsProjectFileInputRequiresBuiltOutput()
    {
        var projectDir = CreateProjectOutputFixture();
        var projectPath = Path.Combine(projectDir, "FixtureProject.csproj");

        try
        {
            var exception = Assert.Throws<InvalidOperationException>(() =>
                ClrAssemblyLifter.LiftPaths(projectDir, new[] { projectPath }));

            Assert.Contains($"no built output found for project {projectPath}", exception.Message);
            Assert.Contains("build it first", exception.Message);
        }
        finally
        {
            Directory.Delete(projectDir, recursive: true);
        }
    }

    [Fact]
    public void LiftPathsRefusesAmbiguousReleaseProjectOutputs()
    {
        var projectDir = CreateProjectOutputFixture(
            ("Release", "net9.0", typeof(ClrFixture).Assembly.Location),
            ("Release", "net10.0", typeof(ClrFixture).Assembly.Location));

        try
        {
            var exception = Assert.Throws<InvalidOperationException>(() =>
                ClrAssemblyLifter.LiftPaths(projectDir, new[] { "." }));

            Assert.Contains(
                "ambiguous CLR project output assemblies named FixtureProject.dll under bin/Release/",
                exception.Message);
            Assert.Contains("specify the assembly path explicitly", exception.Message);
        }
        finally
        {
            Directory.Delete(projectDir, recursive: true);
        }
    }

    private static string CreateProjectOutputFixture(
        params (string Configuration, string TargetFramework, string SourceAssembly)[] outputs)
    {
        var projectDir = Path.Combine(Path.GetTempPath(), $"provekit-clr-lift-{Guid.NewGuid():N}");
        Directory.CreateDirectory(projectDir);
        File.WriteAllText(
            Path.Combine(projectDir, "FixtureProject.csproj"),
            """
            <Project Sdk="Microsoft.NET.Sdk">
              <PropertyGroup>
                <AssemblyName>FixtureProject</AssemblyName>
              </PropertyGroup>
            </Project>
            """);

        foreach (var output in outputs)
        {
            var outputDir = Path.Combine(projectDir, "bin", output.Configuration, output.TargetFramework);
            Directory.CreateDirectory(outputDir);
            File.Copy(output.SourceAssembly, Path.Combine(outputDir, "FixtureProject.dll"));
        }

        return projectDir;
    }

    private static string ContractPrefix(Type type, string methodName)
    {
        return $"clr:{type.Assembly.GetName().Name}!{type.FullName}::{methodName}#";
    }
}
