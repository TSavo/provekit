# Lift, don't author

The fundamental problem of formal verification has always been: how do we get the specifications? For fifty years the answer was "convince developers to write them." That never worked at scale.

Sugar's answer is different. The specifications already exist. They live in the annotation libraries codebases already use. The job is not to convince developers to author specs; the job is to lift specs from where they already are.

This is the lift-not-author posture. It is the most consequential single design choice in Sugar's adoption story.

## The history

For five decades, formal verification has had a chicken-and-egg adoption problem. The verification tools were powerful (Coq, Lean, F\*, ACL2, Isabelle); the specifications were absent. Without specs, the tools had nothing to verify. Writing specs required:

- Learning a spec language (Coq's Gallina, Lean's tactic mode, F\*'s effects).
- Maintaining specs separate from code (drift between code and spec is constant).
- Convincing developers that specs are worth writing (they usually weren't, given the cost).

Adoption never reached the mainstream. The tools remained academic / niche. Industrial use was confined to high-stakes domains (aerospace, cryptography, kernels) where the cost was justified.

The community knew this was the bottleneck. Multiple attempts have been made:

- **Lighter-weight spec languages** (TLA+, Dafny, Spec#) lower the spec authoring cost but still require spec authoring.
- **Type systems with refinements** (Liquid Haskell, Flux, Refined-TS) embed lightweight specs in types but require authors to add refinement annotations.
- **Property-based testing** (QuickCheck, Hypothesis, fast-check, proptest) lets developers express properties as test code. Adoption is wider but still requires authors to think specifications.

Each lowered the cost. None eliminated it. Each still requires the developer to think "let me write a specification."

## What "lift" actually does

Take a closer look at a typical Rust codebase. It probably uses one or more of:

- `proptest` for property tests.
- `contracts` for `#[requires]` / `#[ensures]`.
- `serde` for serialization annotations.
- `validator` for input validation.
- `clap` for CLI argument constraints.
- Manual `assert!` / `debug_assert!` invariants.

Each of these is a **specification the developer already wrote**, not for verification, but as part of normal development. The codebase already maintains them. They drift less than specs in a separate spec language because they're checked at test time.

The lift adapter walks these annotations and emits canonical IR. The developer didn't add anything. They didn't change their workflow. They didn't learn a spec language. The specifications they already authored became proof candidates.

This is what "lift, don't author" means operationally. The codebase keeps its existing annotations. The author keeps their existing workflow. Sugar does not ask the author to learn a new spec language, write a parallel specification, or migrate to a different annotation library.

## Coverage by ecosystem

The lift-not-author posture only works because annotation libraries are widespread:

| Ecosystem | Annotation libraries that already ship specs |
|---|---|
| Rust | `proptest`, `contracts`, `kani`, `prusti`, `serde`, `validator`, `clap` |
| TypeScript | `zod`, `class-validator`, `fast-check`, `io-ts`, `valibot`, `runtypes`, `ajv` |
| Python | `pydantic`, `dataclasses`, `attrs`, `deal`, `hypothesis`, `icontract` |
| Java / JVM | Bean Validation, JML, Spring annotations, Cofoja, Kotlin contracts |
| C# | `DataAnnotations`, `FluentValidation`, contracts in System.Diagnostics |
| Ruby | `active_model`, `dry-validation`, RSpec matchers |
| Go | `go-playground/validator`, `ozzo-validation` |
| C++ | C++26 contracts, Boost.Contract, `assert.h` |

Every mainstream ecosystem has at least one widely-deployed annotation library. Most have several. The lift adapter is the bridge from the source library to the protocol's canonical IR.

## What this changes about adoption

The traditional adoption story for formal verification:

1. Convince a team to adopt a new spec language.
2. Pay the spec-authoring cost upfront.
3. Maintain specs alongside code.
4. Verify with the tool.
5. Receive verification reports.

Adoption is gated on step 1, which usually fails. Even when it succeeds, step 2 dominates ongoing cost.

Sugar's adoption story:

1. Notice your codebase already uses an annotation library.
2. Install the lift adapter for that library.
3. Run `sugar prove`.
4. Receive verification reports.

Steps 1 and 2 are trivial. Step 3 is automated. The friction that defeated formal verification for fifty years is bypassed.

This is the structural reason Sugar has a chance at mainstream adoption where the previous fifty years' tools did not.

## What "lift" doesn't do

Two important non-claims:

### 1. It doesn't increase what your annotations express

The lift adapter takes the annotations you wrote and translates them. If the annotations don't express something, the canonical IR doesn't either. `@Min(0)` lifts to `x >= 0`; it doesn't lift to a stronger property because no stronger property is in the annotation.

If you want stronger contracts, you write stronger annotations. The lift adapter doesn't infer them.

### 2. It doesn't lift everything

Per [`../contributing/adapter-coverage-rubric.md`](../contributing/adapter-coverage-rubric.md), adapters cover a documented subset of their source library's annotations. Annotations outside the covered set are skipped (with a warning) or unrecognized (silently ignored).

The lifting story is partial coverage with explicit honesty about gaps. It is not "every annotation in every library is liftable." Some annotations are inherently unliftable (custom validators with arbitrary code), some require IR primitives that don't exist, some are simply not yet covered.

For most codebases, the partial coverage is enough to dominate the contract surface. For codebases with unusual annotation patterns, the coverage might be lower; the path is to file an issue and contribute coverage.

## Sit beneath, don't compete

The framing "Sugar sits beneath every annotation library" is precise. The protocol does not provide a competing annotation library. It provides a lower layer that promotes existing annotations to a content-addressed substrate.

This is intentional. Competing with annotation libraries would re-introduce the adoption problem (developers would have to choose between their existing library and Sugar's). Sitting beneath them eliminates the choice.

A codebase using `pydantic` for type validation now has a path: ship the same `pydantic` annotations, and let `sugar-lift-pydantic` (running underneath) emit canonical IR. The team's authoring workflow is unchanged; the verification surface is gained.

## Comparison to "convince developers to spec"

The traditional approach asked developers to do new work. Sugar asks developers to do nothing. The work is in the lift adapter, written once per annotation library, maintained by the adapter author (typically the kit maintainers or ecosystem contributors).

This shifts the cost from "every developer in the world" to "one adapter author per library." For a library with thousands of users, the adapter cost is amortized across all of them.

Operationally: writing a lift adapter is one engineer's project for a few weeks. Adopting that adapter is `cargo add` (or equivalent). The leverage is enormous.

## Why this is "free"

A common reaction: "lifting must be approximate; there's no way the adapter captures the full semantics."

The reaction is partially right. Some annotations are approximated (especially regex-based ones, where the lifted IR uses a canonical regex that may not match what the source library's regex compiler implements bit-perfectly). For those, the adapter documents the approximation; the user knows.

But for the bulk of annotations (numeric constraints, presence checks, length checks, type assertions), the lifting is exact. The annotation has a precise semantics; the canonical IR has the same precise semantics; the lift is faithful.

Where the lift is exact, the verification is "free" in the sense that no developer effort was added; the existing annotations now participate in formal verification.

## What this argues for

The lift-not-author posture argues for:

- **Per-library adapter investment.** Each annotation library deserves an adapter; each adapter is one engineer's project.
- **Conservative coverage rubrics.** Lift exactly what you can lift correctly; skip the rest with warnings.
- **Cross-adapter parity.** Equivalent constraints in different libraries should produce identical canonical IR; the parity test is the strongest correctness signal.
- **Sit-beneath posture.** Don't compete with annotation libraries; complement them.
- **Spec evolution from need.** When users hit annotations that aren't liftable because the IR lacks primitives, propose the primitives. The IR grows from real need.

The five together are the operational consequences of "lift, don't author."

## Read next

- [`../contributing/writing-a-lift-adapter/`](../contributing/writing-a-lift-adapter/): how to write one.
- [`../contributing/adapter-coverage-rubric.md`](../contributing/adapter-coverage-rubric.md): coverage standards.
- [content-addressing-not-registry.md](content-addressing-not-registry.md): companion explanation.
- [thesis.md](thesis.md): the central claim that "lift, don't author" supports.
- [`../reference/per-adapter-coverage.md`](../reference/per-adapter-coverage.md): current coverage across shipping adapters.
