# Tutorial: C#

> **Status:** kit + canonicalizer + verifier shipping in the current v1.6.3 tree. Lift adapters shipping: `DataAnnotations`, `LINQ`. LSP plugin shipping. Verification via the Rust CLI today; `Sugar.Verifier` in-process verifier planned.

A walkthrough for C# developers. By the end you have a `.proof` catalog lifted from `[Required]`, `[Range]`, `[StringLength]` data annotations and LINQ predicate quantifiers (`All`, `Any`).

## 1. What you'll have at the end

- A `.proof` file alongside your NuGet package.
- Mementos derived from existing `DataAnnotations` and LINQ predicates.
- LSP-driven squigglies in your editor.

## 2. Prerequisites

- .NET 10 SDK.
- Rust toolchain on `PATH` (verifier subprocess).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install --path implementations/rust/sugar-cli
sugar verify-protocol

cd implementations/csharp && dotnet build
```

The C# kit ships as `Sugar.IR`, `Sugar.Canonicalizer`, `Sugar.SelfContracts`, `Sugar.ClaimEnvelope`, `Sugar.ProofEnvelope`, `Sugar.Verifier`.

## 4. Lift your first contract

If your codebase already uses DataAnnotations:

```csharp
public class User {
    [Required, EmailAddress]
    public string Email { get; set; }

    [Range(0, 150)]
    public int Age { get; set; }
}
```

Or LINQ predicate quantifiers:

```csharp
var allAdults = users.All(u => u.Age >= 18);
```

Run the lifter:

```bash
sugar-lift-cs
```

`Sugar.Lift.DataAnnotations` walks `[Required]`, `[StringLength]`, `[Range]`, `[RegularExpression]`, `[EmailAddress]`. `Sugar.Lift.Linq` walks LINQ expression trees and lifts `All` / `Any` to `forall` / `exists` IR.

## 5. Verify

```bash
sugar prove
```

## 6. Wire your IDE

- **IDE:** install the LSP plugin (`Sugar.Lsp.Plugin`). See [docs/how-to/ide-integration/](../how-to/ide-integration/).

## What's next

- [docs/how-to/publishing-a-proof.md](../how-to/publishing-a-proof.md): ship the `.proof` alongside your NuGet package.
- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Known gaps: actual NuGet package names, Bridge IR full v1.1.0 shape (task #192), LSP install per editor.*
