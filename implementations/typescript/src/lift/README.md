# provekit-lift (TypeScript)

Promote your existing zod schemas, fast-check properties, and
class-validator DTOs to content-addressed signed contract mementos. The
toolchain sits BENEATH the schema/validation libraries you already use:
keep your code, get a `.proof` you can ship across kit boundaries.

## Canonical adoption: vitest plugin

The recommended on-ramp is the vitest plugin. After two lines of config,
lift runs automatically on every `pnpm test` (or `pnpm vitest run`).
There is no separate `provekit-lift` command to invoke; that CLI still
ships for non-vitest tooling, but it is not the primary UX.

Add to your `vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";
import provekitLift from "./implementations/typescript/src/lift/vitest-plugin.js";

export default defineConfig({
  plugins: [provekitLift({ strict: false, adapters: ["zod", "fast-check", "class-validator"] })],
});
```

That is the entire opt-in. Run `pnpm test` and you'll see, alongside
vitest's normal output:

```
ProvekIt: lifted 12 contracts (5 zod, 3 fast-check, 4 class-validator); minted .proof at <path>; <CID>.
```

The `.proof` is written to `node_modules/.cache/provekit/<cid>.proof`
unless you override `outDir`. The CID is a BLAKE3-512 self-identifying
hash of the canonical envelope bytes; the filename IS the CID.

## Strict mode

`strict: true` (or env `PROVEKIT_LIFT_STRICT=1`) makes the plugin throw
from `buildStart` on any of:

- one or more files failed to parse,
- one or more adapters emitted skip-warnings (a contract that the
  adapter recognized but couldn't lift to canonical IR; e.g., a zod
  `.refine(callback)` or a class-validator `@CustomMagicCheck()`),
- the workspace had zero liftable contracts.

This is the "fail the test run on contract violation" knob. In the
lift toolchain there is no notion of a runtime contract failure; the
structurally analogous signals are parse errors and adapter skips, and
strict mode treats those as failures so CI surfaces them instead of
quietly dropping signal.

Loose mode (the default) prints warnings to stderr and writes the proof
anyway.

## Three adapters

| adapter           | shape lifted                                          | output    |
|-------------------|-------------------------------------------------------|-----------|
| `zod`             | `const X = z.<chain>...`                              | `pre`     |
| `fast-check`      | `it("...", () => fc.assert(fc.property(<arb>, fn)))`  | `inv`     |
| `class-validator` | `class Dto { @IsX() field: T; ... }`                  | `pre`     |

`pre` for type/precondition adapters; `inv` for property tests
(universally-quantified invariants over the arbitraries' sorts).

The class-validator adapter recognizes the standard NestJS decorator
set:

- length / non-empty: `@IsNotEmpty`, `@IsEmpty`, `@MinLength(N)`, `@MaxLength(N)`, `@Length(min, max?)`
- numeric bounds: `@Min(N)`, `@Max(N)`, `@IsPositive`, `@IsNegative`
- type predicates: `@IsInt`, `@IsNumber`, `@IsBoolean`, `@IsString`, `@IsDate`, `@IsArray`, `@IsObject`
- string formats: `@IsEmail`, `@IsUrl`/`@IsURL`, `@IsUUID`, `@IsAlpha`, `@IsAlphanumeric`, `@IsAscii`, `@IsBase64`, `@IsHexadecimal`, `@IsJSON`, `@IsIP`, `@IsPhoneNumber`, `@Matches(/regex/)`
- accepted-but-not-constrained: `@IsOptional`, `@IsDefined`, `@Allow`, `@Expose`, `@Exclude`, `@Type`, `@Transform`, `@ValidateIf`

Unknown decorators (custom validators, `@Validate(MyConstraint)`, etc.)
cause the WHOLE class to skip with a warning, mirroring the
fail-loud-on-unknown discipline of the zod adapter's `.refine()`
handling.

## Strategic positioning

ProvekIt does NOT compete with zod, fast-check, io-ts, yup, joi,
class-validator, valibot, or any other schema/property library.
Developers keep their existing code; the lift adapters READ what's
already there and promote each construct to a content-addressed signed
contract memento. The `proveLift/` LLM-driven pipeline is a fallback
for greenfield code where no annotation library is in use.

## CLI fallback

The standalone CLI (`provekit-lift`) is kept for non-vitest contexts
(CI scripts, language tooling, IDE integrations, the Rust mirror's
parallel UX). For day-to-day TypeScript development you should use the
vitest plugin instead. Two lines of config is the right UX; an extra
shell command is not.
