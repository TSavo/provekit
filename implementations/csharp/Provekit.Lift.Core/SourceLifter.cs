// SPDX-License-Identifier: Apache-2.0
//
// Provekit.Lift.Core.SourceLifter: Roslyn-driven source-to-IR pipeline
// for the C# kit. Compiles a C# source string in-process, applies
// DataAnnotations reflection lift to every public class, and adds any
// //provekit: annotation declarations.
//
// This is the shared core for the C# kit (task #219). Both the LSP plugin
// and any future batch-CLI lift binary call into it; the binaries are
// thin shells around this orchestration.

using System.Reflection;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Provekit.IR;
using Provekit.Lift.DataAnnotations;

namespace Provekit.Lift.Core;

public static class SourceLifter
{
    /// <summary>
    /// Compile <paramref name="source"/> to an in-memory assembly, lift
    /// every public class via DataAnnotations, append annotation-scan
    /// declarations, and return both the combined ContractDecl list and
    /// the P/Invoke call-edge stream per spec #114 R1.
    /// </summary>
    public static (List<ContractDecl> Declarations, IReadOnlyList<CallEdgeDeclaration> CallEdges)
        LiftSourceWithCallEdges(string source, string path = "Source.cs")
    {
        var decls = LiftSource(source, path);
        var cidIndex = PInvokeResolver.BuildContractIndex(decls);
        var edges = PInvokeResolver.WalkCallEdges(source, path, cidIndex);
        return (decls, edges);
    }

    /// <summary>
    /// Compile <paramref name="source"/> to an in-memory assembly, lift
    /// every public class via DataAnnotations, append annotation-scan
    /// declarations, and return the combined ContractDecl list.
    /// Returns just the annotation-scan results if compilation fails.
    /// </summary>
    public static List<ContractDecl> LiftSource(string source, string path = "Source.cs")
    {
        var decls = new List<ContractDecl>();

        try
        {
            var assembly = CompileToAssembly(source, path);
            if (assembly is not null)
            {
                foreach (var type in assembly.GetTypes())
                {
                    if (type.IsClass
                        && !type.IsNestedPrivate
                        && type.GetCustomAttribute<System.Runtime.CompilerServices.CompilerGeneratedAttribute>() is null)
                    {
                        decls.AddRange(DataAnnotationsLift.LiftType(type));
                    }
                }
            }
        }
        catch
        {
            // Compilation failed: fall through to annotation scan only.
        }

        decls.AddRange(AnnotationScanner.ScanAnnotations(source));
        return decls;
    }

    /// <summary>
    /// Compile a C# source string to an in-memory assembly with the
    /// minimal references needed for DataAnnotations reflection.
    /// Returns null on emit failure.
    /// </summary>
    public static Assembly? CompileToAssembly(string source, string path)
    {
        var tree = CSharpSyntaxTree.ParseText(source, path: path);

        var references = new List<MetadataReference>
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.ComponentModel.DataAnnotations.RequiredAttribute).Assembly.Location),
            MetadataReference.CreateFromFile(Assembly.Load("System.Runtime").Location),
            MetadataReference.CreateFromFile(Assembly.Load("System.ComponentModel.Primitives").Location),
        };

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
}
