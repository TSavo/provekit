# Adapter coverage rubric

How much of a source library should an adapter cover before shipping? This rubric gives a precise answer.

## The principle

**Lift only what canonicalizes cleanly. Skip everything else.** Coverage is the fraction of the source library's annotations that lift to canonical IR with full semantic fidelity. Annotations that lift partially, ambiguously, or not at all are not lifted.

This is a strong claim. The rubric below makes it operational.

## The four categories

For every annotation in the source library, the adapter places it in one of:

### A: handled

The adapter recognizes the annotation, understands its semantics fully, and produces canonical IR that captures the semantics losslessly.

Test: round-trip the canonical IR through a third-party verifier (e.g., feed the IR to Z3 directly with a known-true model, expect `sat`; with a known-false model, expect `unsat`). If the round-trip works, the canonical IR captures semantics correctly.

### B: skipped (with warning)

The adapter recognizes the annotation but does not lift it. The annotation is structurally well-formed; the adapter just doesn't have a canonical IR mapping yet.

The adapter logs a warning at the source location with a clear message: "this annotation is not yet lifted; see [issue link]."

Reasons for skipping:

- The annotation requires an IR primitive that doesn't exist yet (proposal pending).
- The annotation has under-specified semantics in the source library.
- The adapter's coverage tier (B in the [pick-a-source-library.md](writing-a-lift-adapter/01-pick-a-source-library.md) rubric) intentionally excludes it.

### C: structurally unrecognized

The adapter doesn't know what to do with the annotation. The library has evolved, or the user is using an extension the adapter wasn't aware of.

The adapter silently ignores. No warning (because the adapter doesn't know it's a Sugar-relevant annotation; it might be a third-party annotation entirely unrelated).

### D: malformed

The annotation is structurally invalid (e.g., `@Min` with no argument). The source library would itself reject this; the adapter forwards the rejection.

The adapter errors with a clear message pointing at the malformation.

## What the rubric demands

A shipping adapter has the following ratios for the annotations it claims to cover:

- **A (handled): >= 80% of annotations the user will encounter in real code.** This is the lift-not-author payoff: the adapter "just works" on most of what users wrote.
- **B (skipped with warning): the residue.** Users get warnings about unsupported annotations and know what to expect.
- **C (unrecognized): rare and silent.** Most adapters will encounter zero of these in practice.
- **D (malformed): defer to the source library's parser.** The adapter doesn't second-guess.

The "80% in real code" benchmark is not theoretical. Pick a sample of real codebases using the source library; measure the adapter's coverage on that sample. If it's below 80%, broaden A.

## How to think about "skipped"

A skipped annotation is a load-bearing decision, not a placeholder. The user sees a warning. The user adjusts: either uses a different annotation, simplifies their constraint, files an issue requesting support, or accepts the gap.

The wrong instinct is to lift the annotation imperfectly to "fill the coverage." This pollutes the lattice. A wrong canonical IR is worse than no canonical IR; it gives users false confidence.

The right instinct is to skip explicitly with a warning. Coverage gaps are honest. Wrong coverage is dishonest.

## Cross-adapter equivalence as a coverage check

If your adapter targets a library that has a sibling adapter (Bean Validation has JML; zod has class-validator), use cross-adapter equivalence as a coverage check:

For each annotation in your adapter's "A" set, find the equivalent annotation in the sibling adapter. Both should produce identical canonical IR. If they don't, one of:

1. Your canonical IR is wrong; fix.
2. The sibling's canonical IR is wrong; coordinate a fix.
3. The annotations aren't actually equivalent; document the difference.

Cross-adapter equivalence is the strongest test. It catches subtle canonicalization errors that single-adapter testing misses.

## The "clean canonicalization" smell test

When deciding whether to put an annotation in A or B, apply this smell test:

- Can you write down the canonical IR by hand?
- Does the IR have any "fudge": sort coercions, bound-variable renaming, predicate composition that doesn't quite match the annotation's semantics?
- Would a careful reader of the canonical IR be surprised by what it claims?

If any answer is "yes," the annotation belongs in B. Don't lift fudge.

## Coverage reporting

Each adapter ships a `COVERAGE.md` with the breakdown:

```markdown
# Coverage: provekit-lift-zod

## A: handled (47 annotations)

z.string() with: .min(N), .max(N), .length(N), .email(), .url(), .uuid(), .regex(R), .startsWith(S), .endsWith(S), .includes(S)
z.number() with: .int(), .min(N), .max(N), .positive(), .negative(), .nonnegative(), .nonpositive(), .finite(), .safe()
z.boolean()
z.bigint() with: .min(N), .max(N), .positive(), .negative()
z.date() with: .min(D), .max(D)
z.object({...}) with nested schemas
z.array(...) with: .min(N), .max(N), .length(N), .nonempty()
z.tuple([...])
z.optional(), z.nullable(), z.nullish()
z.union([...]), z.discriminatedUnion(...)
z.literal(V), z.enum([...]), z.nativeEnum(E)

## B: skipped with warning (8 annotations)

z.string().datetime(): temporal sort not yet in IR primitives [provekit#142]
z.string().ip(): requires ip-format predicate [provekit#143]
z.preprocess(...): runtime transforms, structurally not liftable
z.transform(...): same
z.refine(fn): custom predicate; refines runtime, not statically liftable
z.lazy(...): recursive schemas, planned for v0.4 [provekit#144]
z.intersection([...]): intersect of schemas; semantics differ from logical and
z.brand(...): phantom-type tagging; no IR equivalent

## C: unrecognized (0 annotations as of 2025-12-01)

## D: malformed (forwarded)

Adapter does not validate syntactic well-formedness; defers to zod's parser.

## Cross-adapter parity

Tested against:
- (none currently; zod is the only TypeScript schema library this adapter covers)

## Real-codebase sample

Tested against the top 20 zod-using packages on npm by download. Coverage: 91% of annotations in A. Residue is exclusively z.string().datetime() (B-listed) and custom z.refine() (B-listed).
```

The COVERAGE.md is the contract with users. Be honest.

## When to defer to a future version

Some annotations need IR primitives that don't exist. The right move:

1. List in B with a tracking issue number.
2. Open a spec proposal per [proposing-a-spec-change.md](proposing-a-spec-change.md).
3. When the spec lands, move the annotation from B to A and update COVERAGE.md.
4. Bump the adapter version.

This is the slow path. Coverage grows monotonically over time; old codebases benefit automatically when the lattice grows.

## Read next

- [writing-a-lift-adapter/01-pick-a-source-library.md](writing-a-lift-adapter/01-pick-a-source-library.md): coverage tiers (A/B/C) per adapter.
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md): aggregator across all shipping adapters.
- [proposing-a-spec-change.md](proposing-a-spec-change.md): when an IR primitive doesn't exist yet.
