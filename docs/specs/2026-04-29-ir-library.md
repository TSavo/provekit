# ProvekIt: the IR library — `@provekit/ir`

> Author: shared session 2026-04-29 (T + Claude). The TypeScript-side
> reference IR library. Defines the canonical authoring surface that
> every kit's IR library implements in its host language.

## Why this spec exists

The kit standard requires every kit to ship an "IR library" — the
host-language authoring surface for properties. This spec defines the
TypeScript reference implementation. Other kits (Rust's
`provekit_ir` crate, COBOL's PROVEKIT.cpy, Lisp's `:provekit-ir`
ASDF system) implement the same logical surface in their host
language's idiom.

The TypeScript implementation is the *reference* because:
- TypeScript has the broadest LLM training-data coverage; producers
  can author IR fluently in TS first.
- TypeScript's type system is expressive enough to encode the
  type-dialect surface (branded types, constraints, refinements) and
  the library-dialect surface (value-level IR formulas) cleanly.
- TypeScript's tooling (tsserver, the LSP ecosystem, npm distribution)
  is the cleanest substrate to dogfood the framework against.

This spec fixes:
- The library's exports (the public API every kit's IR library
  exposes in some idiomatic form).
- The internal representation (the `IrFormula` data type that the
  AST canonicalizer consumes).
- The package layout (`@provekit/ir`'s npm structure).
- Forward compatibility rules (how new primitives are added).
- The relationship to the AST canonicalizer (next spec).

## The two dialects

The IR library exposes two coherent surfaces. They produce comparable
mementos but suit different authoring contexts.

### Type-dialect surface

Property authoring as TypeScript types. Verified by tsserver as a
producer; mementos emitted on every clean type-check pass.

```typescript
import type {
  NonZero, NonEmpty, Sorted, Validated, Branded, Refined, NonNull,
} from "@provekit/ir";

function divide(a: number, b: NonZero<number>): number {
  return a / b;
}

function topOf<T>(arr: NonEmpty<T[]>): T {
  return arr[0];
}

function sortedSearch<T>(haystack: Sorted<T[]>, needle: T): number {
  // implementation uses the Sorted<T> brand to assume ordering
}

type EmailAddress = Validated<string, EmailSchema>;
type SanitizedHtml = Branded<string, "sanitized">;
type PositiveInt = Refined<number, "is positive integer">;
```

The branded type machinery is standard TypeScript:

```typescript
declare const __brand: unique symbol;

export type Branded<T, BrandName extends string> = T & {
  readonly [__brand]: BrandName;
};

export type NonZero<T extends number | bigint> = Branded<T, "non-zero">;
export type NonEmpty<T> = T extends readonly (infer _U)[] ? Branded<T, "non-empty"> : never;
export type Sorted<T> = T extends readonly (infer _U)[] ? Branded<T, "sorted"> : never;
export type NonNull<T> = Exclude<T, null | undefined> & Branded<T, "non-null">;

export type Validated<T, Schema> = Branded<T, "validated"> & {
  readonly __schema: Schema;
};

export type Refined<T, Description extends string> = Branded<T, `refined:${Description}`>;
```

Constructing branded values requires a constructor that performs the
runtime check or trusts an upstream proof:

```typescript
export function nonZero<T extends number>(x: T): NonZero<T> | null {
  return x === 0 ? null : x as NonZero<T>;
}

export function assertNonZero<T extends number>(x: T): NonZero<T> {
  if (x === 0) throw new Error("expected non-zero");
  return x as NonZero<T>;
}

export function nonEmpty<T>(arr: T[]): NonEmpty<T[]> | null {
  return arr.length === 0 ? null : arr as NonEmpty<T[]>;
}
```

The constructor's runtime check IS the proof for the type-dialect
memento. tsserver verifying the brand at every consumption site IS
the type-check-pass memento for that consumption site.

### Library-dialect surface

Property authoring as TypeScript values. Verified by formal producers
(Z3, Datalog, behavioral test runners, LLM cross-validation) that
read the IR formula data structure and translate to their backend
input format.

```typescript
import {
  property, forAll, exists, implies, and, or, not,
  assert as Assert,
  scope, function_, module_, transition,
  Int, Bool, Ref,
} from "@provekit/ir";

const denominatorNonZero = property({
  name: "denominator-nonzero",
  scope: function_("calculate"),
  bindings: {
    b: Int,
  },
  formula: forAll((b: Int) => Assert.notEqual(b, 0)),
});

const inputSanitizedBeforeSink = property({
  name: "user-input-sanitized-before-execSync",
  scope: module_("api"),
  bindings: {
    input: Ref(),
    sink: Ref(),
  },
  formula: forAll((input: Ref, sink: Ref) =>
    implies(
      and(
        Assert.dataFlowsTo(input, sink),
        Assert.kindOf(sink, "execSync"),
      ),
      exists((path: Ref) => and(
        Assert.onPath(path, input, sink),
        Assert.kindOf(path, "sanitize"),
      )),
    ),
  ),
});

const counterMonotone = property({
  name: "counter-only-increases",
  scope: transition("step"),
  bindings: {
    pre: Int, post: Int,
  },
  formula: forAll((pre: Int, post: Int) =>
    implies(
      Assert.transitionFrom(pre).to(post),
      Assert.greaterThanOrEqual(post, pre),
    ),
  ),
});
```

The library-dialect formula evaluates to an `IrFormula` data structure
(see "Internal representation" below) at runtime. The AST
canonicalizer reads this structure and emits the canonical FOL form.

## Required exports

Every IR library implementation exposes (at minimum) the following
logical exports. Surface syntax varies by host language; semantics
must match.

### Sorts

- `Bool`, `Int`, `Real`, `String`, `Ref` — the FOL primitive sorts.
- `Set<T>`, `Tuple<...>`, `Function<...>` — the constructed sorts.
- `Node`, `Edge`, `Region` — the SAST/graph sorts (for properties
  over code structure).

### Type-dialect brands

- `NonZero<T>`, `NonEmpty<T>`, `Sorted<T>`, `NonNull<T>` — common
  brands.
- `Branded<T, Name>` — generic brand constructor for kit-specific
  brands.
- `Validated<T, Schema>` — schema-validated brand.
- `Refined<T, Predicate>` — predicate-refined brand.
- `Range<T, lo, hi>` — bounded numeric brand.

### Quantifiers

- `forAll<T>(predicate: (x: T) => Formula): Formula`.
- `exists<T>(predicate: (x: T) => Formula): Formula`.
- `forSome<T>(domain: Set<T>, predicate: (x: T) => Formula):
  Formula` — bounded quantifier.

### Connectives

- `and(...formulas: Formula[]): Formula`.
- `or(...formulas: Formula[]): Formula`.
- `not(formula: Formula): Formula`.
- `implies(antecedent: Formula, consequent: Formula): Formula`.
- `iff(a: Formula, b: Formula): Formula`.

### Assertions

The `assert` namespace exposes comparison and predicate primitives:

- `assert.equal(a, b)`, `assert.notEqual(a, b)`.
- `assert.lessThan(a, b)`, `assert.lessThanOrEqual(a, b)`,
  `assert.greaterThan(a, b)`, `assert.greaterThanOrEqual(a, b)`.
- `assert.true(b)`, `assert.false(b)`.
- `assert.subset(a, b)`, `assert.member(x, set)`.
- `assert.kindOf(node, kind)` — SAST predicate.
- `assert.dataFlowsTo(a, b)` — SAST predicate.
- `assert.dominates(a, b)` — SAST predicate.
- `assert.transitionFrom(pre).to(post)` — temporal predicate.
- Domain-specific extensions are kit-defined.

### Scope helpers

- `function_(name: string)` — scope to a named function.
- `module_(path: string)` — scope to a module.
- `class_(name: string)` — scope to a class definition.
- `transition(name: string)` — scope to a state transition.
- `region(start, end)` — scope to a code region.
- `whenever(predicate)` — scope to all sites where a predicate holds.

### The `property` constructor

```typescript
function property<TBindings>(spec: {
  name: string;
  scope: BindingScope;
  bindings: TBindings;
  formula: (bindings: TBindings) => Formula;
  hint?: "datalog-friendly" | "requires-smt" | "behavioral" | "auto";
}): Property;
```

The `hint` field is the compilation hint the producer registry uses
to route the property to the appropriate producer. `auto` lets the
framework decide based on the formula's structure.

## Internal representation

Every formula evaluates to an `IrFormula` value at runtime. This is
the data structure the AST canonicalizer consumes.

```typescript
type IrFormula =
  | { kind: "forall"; sort: Sort; predicate: IrFormulaLambda }
  | { kind: "exists"; sort: Sort; predicate: IrFormulaLambda }
  | { kind: "and"; conjuncts: IrFormula[] }
  | { kind: "or"; disjuncts: IrFormula[] }
  | { kind: "not"; body: IrFormula }
  | { kind: "implies"; antecedent: IrFormula; consequent: IrFormula }
  | { kind: "atomic"; predicate: AtomicPredicate; args: IrTerm[] };

type IrTerm =
  | { kind: "var"; name: string; sort: Sort }
  | { kind: "const"; value: unknown; sort: Sort }
  | { kind: "ctor"; name: string; args: IrTerm[]; sort: Sort };

type AtomicPredicate =
  | "=" | "≠" | "<" | "≤" | ">" | "≥"
  | "kind-of" | "data-flows-to" | "dominates"
  | "transition-from-to"
  | string;  // kit-defined extensions

type IrFormulaLambda = {
  kind: "lambda";
  varName: string;
  sort: Sort;
  body: IrFormula;
};

type Sort =
  | { kind: "primitive"; name: "Bool" | "Int" | "Real" | "String" | "Ref" | "Node" | "Edge" }
  | { kind: "set"; element: Sort }
  | { kind: "tuple"; elements: Sort[] }
  | { kind: "function"; domain: Sort[]; range: Sort };
```

This structure is the canonical FOL representation. The AST
canonicalizer hashes it (with bound-variable alpha-renaming for
hash stability across naming conventions) to produce the
propertyHash.

## Compilation hints

The `hint` field on `property()` informs the producer registry which
backend to route to. The hints are open (kit-defined extensions
allowed) but the framework ships with these:

- `datalog-friendly` — pattern-style; first-order Horn-clause
  expressible. Routed to a Datalog backend.
- `requires-smt` — symbolic; requires a satisfiability solver
  (Z3, CVC5). Routed to an SMT backend.
- `behavioral` — runtime property; routed to a test-case generator
  + executor.
- `auto` — framework analyzes the formula's structure and decides:
  Horn-clause-only formulas → datalog; arithmetic with integers
  → SMT; quantifier-free with scoped state → behavioral.

A property may carry multiple producers (cross-validation): a
`requires-smt` hint with a secondary `behavioral` hint produces two
mementos for the same property — Z3 verdict and behavioral verdict.
Disagreement surfaces as a quality signal.

## Package layout

```
@provekit/ir/
├── package.json
├── README.md
├── src/
│   ├── index.ts                 # main entrypoint; re-exports below
│   ├── brands.ts                # NonZero, NonEmpty, Sorted, etc.
│   ├── sorts.ts                 # Bool, Int, Real, Ref, Node, etc.
│   ├── formulas.ts              # IrFormula type + builders
│   ├── property.ts              # property() constructor
│   ├── quantifiers.ts           # forAll, exists, forSome
│   ├── connectives.ts           # and, or, not, implies, iff
│   ├── assert.ts                # assertion namespace
│   ├── scopes.ts                # function_, module_, transition, etc.
│   └── canonicalize.ts          # public re-export of canonicalizer
├── evidence-schemas/
│   ├── z3-model.schema.json
│   ├── pattern-match.schema.json
│   ├── type-check-pass.schema.json
│   └── ...                      # all standard variant schemas
└── tests/
    └── ir-formula-roundtrip.test.ts
```

Published to npm as `@provekit/ir`. Consumers `npm install @provekit/ir`
and start authoring properties. No additional setup.

## Forward compatibility

Adding a new primitive (a new assertion, a new connective, a new
quantifier, a new sort) is a minor version bump. The IR formula
structure absorbs the new variant; the AST canonicalizer learns the
new variant; producers that don't recognize the variant fall back to
producing `verdict: undecidable` mementos rather than throwing.

Adding a new branded type is a patch bump. Brands are
content-addressed via their string name; new brands are zero-cost
additions to the type system.

Removing a primitive is a major version bump. The IR library's
versioning rides the same content-addressed semver-as-memento
machinery as any other package.

## Cross-language equivalence

Two IR formulas authored in different host languages (TypeScript and
Rust, say) are *cross-language equivalent* when their canonicalized
FOL representations are AST-byte-identical, modulo bound-variable
renaming.

The AST canonicalizer's job is to produce that byte-identical form.
The IR library's job is to give the host-language developer an
ergonomic, idiomatic surface that produces an `IrFormula` value of
the right shape.

For TypeScript: a function call like `forAll(b => assert.notEqual(b, 0))`.
For Rust: a proc-macro like `forall!(b => b != 0)`.
For COBOL: a `PERFORM ASSERT-DENOMINATOR-NONZERO` with a 88-level
condition name `DENOMINATOR-IS-ZERO VALUES ZERO`.
For Lisp: `(forall (b) (not (zerop b)))`.

All four canonicalize to:

```
forall(b: Int).¬(b = 0)
```

Hash that structure → same propertyHash → same memento slot →
producer-fungible cross-validation.

## Acceptance test

The IR library is correct when:

1. A TypeScript developer can author both type-dialect and
   library-dialect properties using only the `@provekit/ir` exports.
2. The runtime IR formula data structure round-trips through
   serialization (JSON canonicalize → string → parse) without
   semantic loss.
3. tsserver can verify type-dialect properties at edit time without
   any framework-specific extension.
4. The library's exports match the kit-standard's required logical
   surface — every required quantifier, connective, assertion, sort,
   and brand is present.
5. Adding a new primitive is a minor-version bump that doesn't break
   existing consumers.
6. The same logical claim authored in Rust's `provekit_ir` and in
   TypeScript's `@provekit/ir` produces canonicalized FOL forms with
   matching propertyHashes.

When these six hold, the IR library is the operational authoring
surface for the framework's TypeScript dialect, and the pattern is
proven for replication in every other host language.

## Implementation notes

- The library has zero runtime dependencies beyond TypeScript itself.
  Brands are zero-cost (compile-time only); IR formulas are plain
  data; quantifiers are higher-order functions that build the data.
- Property declarations can live anywhere in the codebase. The
  framework's CLI walks the source tree, finds `property(...)`
  invocations, evaluates them at parse time (sandboxed), and ingests
  the resulting IR formulas into the property registry.
- The library is itself content-addressed — its CID enters every
  memento's `bindingHash` so a kit-version bump invalidates stale
  mementos cleanly.
- The reference implementation lives in `packages/ir/` in the
  framework repo; consumers depend on the published npm package.
