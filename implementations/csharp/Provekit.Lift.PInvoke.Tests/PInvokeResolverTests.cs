// SPDX-License-Identifier: Apache-2.0
//
// PInvokeResolver tests: six conformance tests per spec #114 R3.
//
// Tests mirror the six scenarios named in the dispatch:
//   1. Classic [DllImport] + call site → "rust-kit:Process"
//   2. [DllImport] with EntryPoint → targetSymbol uses EntryPoint, not C# name
//   3. [LibraryImport] → resolved correctly
//   4. No P/Invoke → no call-edges
//   5. Unknown library → cpp-kit (non-empty, non-rust, non-system → cpp-kit);
//      empty normalised name → resolver-error prefix
//   6. Byte-determinism across two lifts of identical source

using Provekit.IR;
using Provekit.Lift.Core;
using Xunit;

namespace Provekit.Lift.PInvoke.Tests;

public class PInvokeResolverTests
{
    // ------------------------------------------------------------------ //
    //  Test 1: classic [DllImport] with call site → rust-kit:Process       //
    // ------------------------------------------------------------------ //

    [Fact]
    public void DllImport_ClassicPattern_EmitsRustKitEdge()
    {
        const string source = """
            using System.Runtime.InteropServices;

            public class Caller
            {
                public void RunProcess(int n)
                {
                    var result = RustBindings.Process(n);
                }
            }

            public class RustBindings
            {
                [DllImport("rust_callee")]
                public static extern int Process(int n);
            }
            """;

        var contracts = new[]
        {
            new ContractDecl("RunProcess", Predicates.And(), null, null, "out"),
        };
        var cidIndex = PInvokeResolver.BuildContractIndex(contracts);
        var edges = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex);

        Assert.Single(edges);
        Assert.Equal("rust-kit:Process", edges[0].TargetSymbol);
        Assert.Null(edges[0].TargetContractCid);
        Assert.NotEmpty(edges[0].SourceContractCid);
    }

    // ------------------------------------------------------------------ //
    //  Test 2: EntryPoint named arg → targetSymbol uses EntryPoint          //
    // ------------------------------------------------------------------ //

    [Fact]
    public void DllImport_WithEntryPoint_UsesEntryPointNotMethodName()
    {
        const string source = """
            using System.Runtime.InteropServices;

            public class Caller
            {
                public void DoWork(int n)
                {
                    Foo.CallProcess(n);
                }
            }

            public class Foo
            {
                [DllImport("librust_callee.so", EntryPoint = "actual_name",
                           CallingConvention = CallingConvention.Cdecl)]
                public static extern int CallProcess(int n);
            }
            """;

        var contracts = new[]
        {
            new ContractDecl("DoWork", Predicates.And(), null, null, "out"),
        };
        var cidIndex = PInvokeResolver.BuildContractIndex(contracts);
        var edges = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex);

        Assert.Single(edges);
        Assert.Equal("rust-kit:actual_name", edges[0].TargetSymbol);
    }

    // ------------------------------------------------------------------ //
    //  Test 3: [LibraryImport] (.NET 7+) → resolved correctly              //
    // ------------------------------------------------------------------ //

    [Fact]
    public void LibraryImport_NewAttribute_ResolvedCorrectly()
    {
        const string source = """
            using System.Runtime.InteropServices;

            public class Caller
            {
                public void Invoke(int n)
                {
                    Bar.Process(n);
                }
            }

            public partial class Bar
            {
                [LibraryImport("rust_callee", EntryPoint = "process")]
                public static partial int Process(int n);
            }
            """;

        var contracts = new[]
        {
            new ContractDecl("Invoke", Predicates.And(), null, null, "out"),
        };
        var cidIndex = PInvokeResolver.BuildContractIndex(contracts);
        var edges = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex);

        Assert.Single(edges);
        Assert.Equal("rust-kit:process", edges[0].TargetSymbol);
        Assert.Null(edges[0].TargetContractCid);
    }

    // ------------------------------------------------------------------ //
    //  Test 4: No P/Invoke → no FFI call-edges                             //
    // ------------------------------------------------------------------ //

    [Fact]
    public void NoPInvoke_NoFFICallEdges()
    {
        const string source = """
            public class Plain
            {
                public void DoStuff(int n)
                {
                    var x = n + 1;
                }
            }
            """;

        var contracts = new[]
        {
            new ContractDecl("DoStuff", Predicates.And(), null, null, "out"),
        };
        var cidIndex = PInvokeResolver.BuildContractIndex(contracts);
        var edges = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex);

        Assert.Empty(edges);
    }

    // ------------------------------------------------------------------ //
    //  Test 5: Unknown library → cpp-kit (the correct non-empty fallback)  //
    //          Empty normalised name → resolver-error prefix               //
    // ------------------------------------------------------------------ //

    [Fact]
    public void UnknownNonEmptyLibrary_MapsToCppKit_NotPlaceholder()
    {
        const string source = """
            using System.Runtime.InteropServices;

            public class Caller
            {
                public void Call(int n)
                {
                    Mystery.DoThing(n);
                }
            }

            public class Mystery
            {
                [DllImport("some_vendor_graphics_lib")]
                public static extern int DoThing(int n);
            }
            """;

        var contracts = new[]
        {
            new ContractDecl("Call", Predicates.And(), null, null, "out"),
        };
        var cidIndex = PInvokeResolver.BuildContractIndex(contracts);
        var edges = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex);

        Assert.Single(edges);
        // Non-empty, non-rust, non-system → cpp-kit (not resolver-error)
        Assert.StartsWith("cpp-kit:", edges[0].TargetSymbol);
        Assert.DoesNotContain("placeholder", edges[0].TargetSymbol);
    }

    [Fact]
    public void EmptyNormalisedLibName_ReturnsEmptyKit_CallerEmitsResolverError()
    {
        // An empty kit string (returned when normalised lib is empty) is
        // what causes the caller to prefix the entry point with "resolver-error:".
        // This matches spec #97 R2 (fail-loud on unresolvable names).
        var kit = PInvokeResolver.ResolveKit("");
        Assert.Equal("", kit);
    }

    // ------------------------------------------------------------------ //
    //  Test 6: Byte-determinism across two lifts of identical source        //
    // ------------------------------------------------------------------ //

    [Fact]
    public void ByteDeterminism_TwoLiftsOfIdenticalSource_ProduceSameCallEdgeJson()
    {
        const string source = """
            using System.Runtime.InteropServices;

            public class Caller
            {
                public void RunProcess(int n)
                {
                    RustBindings.Process(n);
                }
            }

            public class RustBindings
            {
                [DllImport("rust_callee")]
                public static extern int Process(int n);
            }
            """;

        var contracts = new[]
        {
            new ContractDecl("RunProcess", Predicates.And(), null, null, "out"),
        };

        var cidIndex1 = PInvokeResolver.BuildContractIndex(contracts);
        var edges1 = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex1);
        var json1 = Serialize.MarshalCallEdges(edges1);

        var cidIndex2 = PInvokeResolver.BuildContractIndex(contracts);
        var edges2 = PInvokeResolver.WalkCallEdges(source, "Test.cs", cidIndex2);
        var json2 = Serialize.MarshalCallEdges(edges2);

        Assert.Equal(json1, json2);
        Assert.NotEmpty(json1);
        Assert.NotEqual("[]", json1);
    }

    // ------------------------------------------------------------------ //
    //  Library name normalisation unit tests                               //
    // ------------------------------------------------------------------ //

    [Theory]
    [InlineData("rust_callee",          "rust_callee")]
    [InlineData("librust_callee.so",    "rust_callee")]
    [InlineData("rust_callee.dll",      "rust_callee")]
    [InlineData("librust_callee.dylib", "rust_callee")]
    [InlineData("librust_callee.so.6",  "rust_callee")]
    [InlineData("libc",                 "c")]
    [InlineData("libz.so",              "z")]
    [InlineData("libssl.so.3",          "ssl")]
    public void NormaliseLibName_StripsAffixes(string raw, string expected)
    {
        Assert.Equal(expected, PInvokeResolver.NormaliseLibName(raw));
    }

    [Theory]
    [InlineData("rust_callee",   "rust-kit")]
    [InlineData("rust_auth",     "rust-kit")]
    [InlineData("c",             "libc-system")]
    [InlineData("z",             "libc-system")]
    [InlineData("ssl",           "libc-system")]
    [InlineData("opengl",        "cpp-kit")]
    [InlineData("glib",          "cpp-kit")]
    [InlineData("",              "")]
    public void ResolveKit_MapsCorrectly(string normLib, string expectedKit)
    {
        Assert.Equal(expectedKit, PInvokeResolver.ResolveKit(normLib));
    }
}
