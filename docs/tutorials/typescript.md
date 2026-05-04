# Tutorial: TypeScript

> **Status:** kit shipping (v1.4.1). Lift adapters shipping: `zod`, `class-validator`, `fast-check`. Verification today is via the Rust CLI as a subprocess; a native TypeScript CLI is planned. LSP plugin planned — until then, no IDE squigglies.

A walkthrough for TypeScript developers. By the end you have a `.proof` catalog of signed contract mementos for an npm package, lifted from existing `zod` schemas / `class-validator` decorators / `fast-check` properties, and verified via the Rust CLI.

## 1. What you'll have at the end

- A `.proof` file shipping alongside your `package.json`.
- Mementos derived from your existing `z.object`, `@IsEmail`, `fc.assert(fc.property(...))` annotations — no new spec language, no parallel spec to maintain.
- A handshake report from `provekit prove` showing the discharge breakdown.

## 2. Prerequisites

- Node 22+ and pnpm.
- Rust toolchain on `PATH` (the verifier runs as a subprocess for v1.1).
- Z3 on `PATH` (only Tier 3 of the handshake invokes Z3).

## 3. Install

```bash
# the canonical verifier (Rust CLI)
cargo install provekit
provekit verify-protocol

# the TypeScript kit
pnpm add @provekit/kit @provekit/lift-zod @provekit/lift-class-validator @provekit/lift-fast-check
```

Package names are placeholder; actual names will be confirmed when the v1.1 npm publish lands. See [implementations/typescript/](../../implementations/typescript/) for the in-tree workspace.

## 4. Lift your first contract

If your codebase already uses `zod`:

```typescript
import { z } from 'zod';

export const UserSchema = z.object({
  email: z.string().email(),
  age: z.number().int().min(0).max(150),
});
```

Run the lift adapter:

```bash
npx provekit-lift
```

The lifter walks every `z.object`, `z.string`, `z.number` chain, canonicalizes each schema into IR, hashes each IR formula to a CID, signs each memento, and writes `target/.proof`.

**`class-validator`** decorators (`@IsNotEmpty`, `@MinLength`, `@Min`, `@Max`, `@IsEmail`, etc.) and **`fast-check`** properties (`fc.assert(fc.property(...))`) are picked up by the same `npx provekit-lift` invocation.

## 5. Verify

```bash
provekit prove
```

The handshake walks the catalog, runs the three tiers, and reports the discharge breakdown. See the [Rust tutorial step 4](rust.md#step-4-verify) for the output shape; it is identical regardless of which kit produced the catalog.

## 6. Wire your IDE and CI

- **IDE:** the TypeScript LSP plugin is planned for v1.2. Until then, no in-editor squigglies. The `npx provekit-lift` + `provekit prove` cycle is the loop today.
- **CI:** see [docs/how-to/ci-integration/github-actions.md](../how-to/ci-integration/github-actions.md) for the recipe (uses the same Rust CLI verifier).

## What's next

- [docs/how-to/publishing-a-proof.md](../how-to/publishing-a-proof.md) — ship the `.proof` alongside your npm package.
- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md) — bind a TypeScript implementation to a reference contract that Rust / Python / Java implementations also bridge to.
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md) — what `zod`, `class-validator`, `fast-check` adapters see and what they miss.
- [docs/explanation/thesis.md](../explanation/thesis.md) — why the petabyte-to-64-bytes ratio works.

---

*This tutorial is a stub. Contributions welcome — see [docs/contributing/overview.md](../contributing/overview.md). Known gaps: actual npm package names, end-to-end runnable example, IDE wire-up once the LSP ships.*
