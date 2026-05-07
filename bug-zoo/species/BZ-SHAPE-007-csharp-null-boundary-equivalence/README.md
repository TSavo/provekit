# BZ-SHAPE-007: C# Null Boundary Equivalence

This specimen shows the same null-boundary shape through several existing C#
lifter paths: DataAnnotations, `//provekit:` annotation scanning, and LINQ
predicate lifting. The C# code compiles and the ordinary harness passes, but
the lifted boundary records the missing semantic edge:

`maybe_null(name) => non_null(name)`

The dropped environment uses a C# realizer under `implementations/csharp` to
insert the null guard and emits a receipt whose closure ProofIR is
byte-identical to the Java and TypeScript null-boundary witnesses.

The specimen has two explicit phases:

1. Run the C# implementation CLI to discover the boundary from C# source using
   the requested C# lifter. For example, from the repo root:

   ```bash
   dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/exposed/linq-where/harness
   ```

2. Run the zoo verifier, which asks the lifter RPC for canonical ProofIR and
   checks the byte-identical CID against the other C# exposures and the
   TypeScript/Java null-boundary witnesses:

   ```bash
   provekit zoo bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence
   ```
