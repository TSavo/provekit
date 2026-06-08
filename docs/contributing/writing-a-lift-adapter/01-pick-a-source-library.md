# Writing a lift adapter, step 1: pick a source library

A lift adapter walks the AST of a source library's annotations and emits canonical IR for each annotation it recognizes. The choice of source library is the most important decision; it determines what's lift-able, how much coverage you can claim, and how much value the adapter delivers.

## What makes a good lift target

In rough order of importance:

### 1. Wide deployment

Pick libraries that are already in widespread use. The lift-not-author posture only pays off when most codebases already have annotations to lift. A library with three users gives you nothing to lift.

Exemplary targets, by host language:

- Rust: `proptest`, `contracts`, `kani`, `prusti` (high-coverage in Rust ecosystem).
- TypeScript: `zod`, `class-validator`, `fast-check`, `io-ts`, `valibot`, `ajv` schemas.
- Python: `pydantic`, `deal`, `hypothesis`, `icontract`, `attrs`.
- Java: Bean Validation, JML, Spring Web annotations, Cofoja, Spring Security, Swagger annotations.
- C#: `DataAnnotations`, LINQ predicates, `FluentValidation`.
- Ruby: `active_model`, `dry-validation`, `rspec-expectations`.
- Go: `go-playground/validator`, `ozzo-validation`.

### 2. Structural annotations, not arbitrary code

A lift adapter walks structured annotations (decorators, attributes, comment-block tags). It does not walk arbitrary function bodies. Pick libraries whose contract surface is structurally distinct from imperative code.

Good fit: `@MinLength(5)`, `@requires(x > 0)`, `validate:"email"`.

Bad fit: a function whose precondition is hidden in an `if` statement that throws an exception. Inferring contracts from imperative code is a different problem (see Layer-2 structural lift in the Python kit, which handles a constrained subset).

### 3. Canonicalizable semantics

The adapter must be able to map each annotation to a canonical IR formula deterministically. If two equivalent expressions in the source library could lift to different IR (e.g., due to operator-order ambiguity), the adapter is broken.

Example: `@Min(0) @Max(100)` should produce the same IR as `@Range(0, 100)` if the two are semantically equivalent in Bean Validation. The Java kit's adapter has explicit normalization to ensure this.

### 4. Has explicit semantics

The library should have a clear, documented semantics. Adapters built on under-specified libraries spread that under-specification into the protocol.

For each constraint annotation, you should be able to state precisely: "an instance satisfies this annotation iff [first-order predicate]." If you cannot state the predicate, you cannot canonicalize.

### 5. Stable

Library version churn is your enemy. Every breaking change to the source library forces the adapter to update. Pick libraries whose annotation surface is stable; avoid libraries that rev their decorator API every minor release.

## What makes a bad lift target

By inversion of the above:

- **Niche libraries** (low deployment): lift gives you no leverage.
- **Arbitrary-code annotations** (e.g., libraries that let you write a Python lambda as the contract body): canonicalization is hopeless.
- **Under-specified semantics** (e.g., a validator library whose docs say "approximately a substring match"): you'll bake the ambiguity into the IR.
- **Churning APIs**: maintenance cost dominates value.

## Coverage rubric

Once you've picked a target, pick a coverage tier:

### Tier A: every annotation

You will support every constraint the library exposes. This is realistic for narrow libraries (e.g., `proptest` has a small surface). Rare for broad libraries.

### Tier B: the 80% set

You will support the most-used annotations and explicitly skip the rest. The adapter logs a warning when it encounters a skipped annotation. This is the typical starting point.

The 80% set is empirical. For Bean Validation, it's `@NotNull`, `@NotEmpty`, `@NotBlank`, `@Min`, `@Max`, `@Size`, `@Pattern`, `@Email`, `@Positive`, `@Negative`, `@AssertTrue`, `@AssertFalse`. Together those cover the vast majority of Bean Validation usage in the wild.

### Tier C: the cherry-picked set

You will support a small set of specific annotations and explicitly stop. This is a valid first-version adapter. Tier C → Tier B → Tier A is a valid coverage trajectory; ship Tier C, iterate.

Whatever tier, document it. The adapter's README should list every annotation it handles, every annotation it explicitly skips, and what it does with anything else (warn, ignore, error).

## Examples of well-scoped first adapters

- `sugar-lift-zod`: started with `z.string`, `z.number`, `z.object`, `z.array` plus the most-used validators (`min`, `max`, `email`, `regex`). Now broader. Tier B initial scope was the right call.
- `sugar-lift-pydantic`: started with `BaseModel` field annotations and `Field` constraints. The constraints handled in v1.1 are roughly the Bean Validation 80% set, mapped to pydantic equivalents.
- `sugar-lift-active_model`: started with the `presence`, `length`, `numericality`, `format` validators. Tier B.

## What this step produces

A written-down decision: "this adapter targets library X, version range Y, at coverage tier Z, supporting annotations [list]. Annotations [list] are explicitly skipped with a warning. Everything else is unrecognized and ignored."

This becomes the README of your adapter and the basis for the conformance fixtures (step 4).

## Read next

- [02-walk-the-AST.md](02-walk-the-AST.md): language-specific AST walking.
- [docs/contributing/adapter-coverage-rubric.md](../adapter-coverage-rubric.md) (when written): what counts as good coverage.
- [docs/reference/per-adapter-coverage.md](../../reference/per-adapter-coverage.md): current coverage for shipping adapters.
