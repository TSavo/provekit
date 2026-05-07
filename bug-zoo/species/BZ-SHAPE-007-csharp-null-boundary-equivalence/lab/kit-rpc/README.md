The C# specimen routes through the existing implementation projects under
`implementations/csharp`.

There are two separate responsibilities:

- `dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover <surface> <workspaceRoot>`
  runs the C# implementation lifter and emits native discovery evidence for
  the null-boundary bug.
- `provekit zoo ...` invokes the lifter RPC, receives canonical Bug Zoo
  ProofIR, and verifies the IR CID against the checked-in witness bytes.
