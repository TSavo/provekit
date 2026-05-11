# Provekit.Lift.CLR

`Provekit.Lift.CLR` is the lift-plugin adapter for CLR bytecode. It uses
.NET metadata and PE readers to walk managed assemblies, decode method IL, and
return a Provekit `ir-document` over the `clr-bytecode` surface.

The plugin is intentionally bytecode-first: C#, F#, VB, or any other .NET
language can feed it by compiling to a managed assembly first.
