# ProvekIt: AST canonicalizer

> Author: shared session 2026-04-29 (T + Claude). The byte-identical-
> FOL-hash machinery that makes cross-language equivalence operational.

## Why this spec exists

The kit standard requires every kit to implement an AST canonicalizer.
The IR library spec defines `IrFormula` as a host-language data
structure. The verification IR spec asserts that two formulas in
different host languages expressing the same logical claim produce
matching `propertyHash`. None of those documents specify *how* the
match happens.

This spec specifies it. The canonicalizer's job is twofold:
- Take a host-language IR formula (a TypeScript object, a Rust value,
  a Go struct, a COBOL paragraph) and produce a **canonical FOL AST**
  whose bytes are identical regardless of host language.
- Hash that canonical form deterministically to produce the
  `propertyHash` that downstream producers consume.

Without this spec, "matching propertyHashes across host languages" is
aspirational rhetoric. With it, the cross-language equivalence claim
becomes a mechanical property of the canonicalization algorithm,
verifiable by test.

This spec fixes:
- The canonical FOL AST grammar (every node kind, every field).
- Bound-variable handling (alpha-renaming via de Bruijn indices for
  hash stability).
- Sort canonicalization rules.
- Predicate canonicalization rules.
- AC (associative-commutative) normalization for connectives.
- Negation-normal-form rewriting.
- The `bindingHash` construction (separate from `propertyHash`).
- The serialization-and-hash construction.
- The `AstCanonicalizer` interface every kit implements.
- The cross-language equivalence test that every kit must pass.

## The canonical FOL AST

The canonical form is a tagged-union of nodes, expressed in a
language-agnostic schema. Each kit's canonicalizer produces this
form from its host-language IR-formula representation.

```
CanonicalFolAst :=
  | Quantifier
  | Connective  
  | Atomic

Quantifier := {
  kind: "forall" | "exists";
  sort: CanonicalSort;
  body: CanonicalFolAst;        // body uses de Bruijn index 0 to refer to the bound var
}

Connective := {
  kind: "and" | "or" | "not" | "implies";
  // For and/or:
  operands?: CanonicalFolAst[]; // sorted; AC-normalized; flattened
  // For not:
  body?: CanonicalFolAst;
  // For implies:
  antecedent?: CanonicalFolAst;
  consequent?: CanonicalFolAst;
}

Atomic := {
  kind: "atomic";
  predicate: CanonicalPredicate;
  args: CanonicalTerm[];
}

CanonicalTerm :=
  | Var
  | Const
  | Ctor

Var := {
  kind: "var";
  index: number;                 // de Bruijn index
  sort: CanonicalSort;
}

Const := {
  kind: "const";
  sort: CanonicalSort;
  value: <typed primitive>;
}

Ctor := {
  kind: "ctor";
  name: string;
  args: CanonicalTerm[];
  sort: CanonicalSort;
}
```

This is the structural form. A serializer renders it as bytes (see
"Serialization" below); the bytes are hashed.

## Bound-variable handling

Two formulas that differ only in bound-variable names must
canonicalize identically. TypeScript's `forAll(b => ...)` and Rust's
`forall!(x => ...)` and Go's `ForAll("denominator", ...)` all bind
their variable to a different name; the canonicalized form must
erase the name.

**Mechanism: de Bruijn indices.** Bound variables are referenced by
their binder's depth from the use site. The outermost binder is
index 0 from its body's perspective; the next binder inward is
index 0 from *its* body, but the outer binder is now index 1 from
that inner body, and so on.

Example:

```
Surface (TypeScript):
  forAll<Int>(b => assert.notEqual(b, 0))

Surface (Rust):
  forall!(x: Int => x != 0)

Surface (Go):
  ir.ForAll("denominator", ir.Int, func(d ir.Term) ir.Formula {
      return assert.NotEqual(d, ir.IntConst(0))
  })

Canonical (all three produce):
  {
    kind: "forall",
    sort: { kind: "primitive", name: "Int" },
    body: {
      kind: "atomic",
      predicate: "≠",
      args: [
        { kind: "var", index: 0, sort: { kind: "primitive", name: "Int" } },
        { kind: "const", sort: { kind: "primitive", name: "Int" }, value: 0 }
      ]
    }
  }
```

The variable name (`b`, `x`, `denominator`) does not appear in the
canonical form. The de Bruijn index 0 refers unambiguously to the
nearest enclosing binder.

**Nested binders.**

```
Surface:
  forAll<Int>(a => forAll<Int>(b => assert.equal(a, b)))

Canonical:
  {
    kind: "forall",
    sort: Int,
    body: {
      kind: "forall",
      sort: Int,
      body: {
        kind: "atomic",
        predicate: "=",
        args: [
          { kind: "var", index: 1, sort: Int },   // outer `a`
          { kind: "var", index: 0, sort: Int }    // inner `b`
        ]
      }
    }
  }
```

Index 0 always refers to the nearest enclosing binder; index 1 refers
to the next outer binder; etc.

The canonicalizer's de Bruijn pass walks the IR formula, tracking the
binder stack, and replaces named-variable references with indices.

## Sort canonicalization

Sorts are canonicalized to a fixed grammar:

```
CanonicalSort :=
  | { kind: "primitive"; name: PrimitiveSort }
  | { kind: "set"; element: CanonicalSort }
  | { kind: "tuple"; elements: CanonicalSort[] }      // ordered
  | { kind: "function"; domain: CanonicalSort[]; range: CanonicalSort }

PrimitiveSort ∈
  { "Bool", "Int", "Real", "String", "Ref",
    "Node", "Edge", "Region",
    "Time" }
```

Kit-defined sort extensions are allowed but must be content-hashed
(the sort's name includes a CID prefix to identify the kit's
extension namespace). For example, a Rust kit's `LifetimeSort` would
canonicalize to `{ kind: "primitive", name: "rust-kit:Lifetime@<cid>" }`.

Kits must NOT redefine the meaning of standard primitive sorts.
`Int` always means mathematical integers; `Real` always means
mathematical reals; etc. The canonicalizer enforces this by rejecting
kit definitions that shadow the standard names.

## Predicate canonicalization

Predicates are canonicalized to standard names. The kit standard
defines a baseline set; kits can extend.

**Standard predicates:**

```
"="     // equality (any sort)
"≠"     // inequality
"<"     // less-than (Ordered sort)
"≤"     // less-than-or-equal
">"     // greater-than
"≥"     // greater-than-or-equal
"true"  // truthy (Bool sort)
"false" // falsy
"member" // x ∈ S
"subset" // A ⊆ B
"kind-of"   // SAST: node has kind
"data-flows-to"     // SAST: data flows from a to b
"dominates"         // SAST: a dominates b
"post-dominates"    // SAST: a post-dominates b
"on-path"           // SAST: node is on a path between two others
"transition-from-to" // temporal: state transition
```

Kit-defined predicates use the same content-addressed extension
mechanism as sorts: the predicate name includes a `<kit-name>:<cid>`
prefix.

Aliases canonicalize to standard names: `assert.notEqual` (TS),
`b != 0` (Rust), `assert.NotEqual` (Go), `IF B NOT = ZERO` (COBOL)
all canonicalize to the predicate `"≠"`.

## AC normalization (and / or)

`and` and `or` are associative and commutative. Canonical form:

1. **Flatten.** `and(and(p, q), r)` → `and(p, q, r)`. Recursively.
2. **Sort.** Operands sorted by their canonical hash (so the order
   of TS's `and(p, q)` and Rust's `and(q, p)` collapses to the same
   form).
3. **Deduplicate.** `and(p, p, q)` → `and(p, q)`.
4. **Identity removal.** `and(true, p)` → `p`. `and(false, p)` →
   `false`. `or(false, p)` → `p`. `or(true, p)` → `true`.
5. **Empty handling.** `and()` → `true`. `or()` → `false`.

`implies` is rewritten: `implies(a, c)` → `or(not(a), c)`. This
brings everything into a normal form built from `and`, `or`, `not`,
quantifiers, and atomics.

## Negation-normal form

After AC normalization, push negations inward via De Morgan:

- `not(and(p, q))` → `or(not(p), not(q))`
- `not(or(p, q))` → `and(not(p), not(q))`
- `not(not(p))` → `p`
- `not(forall(s, body))` → `exists(s, not(body))`
- `not(exists(s, body))` → `forall(s, not(body))`
- `not(p ≠ q)` → `p = q`
- `not(p < q)` → `p ≥ q`  (and similarly for ≤, >, ≥)
- `not(true)` → `false` and vice versa

The result is in **negation-normal form (NNF)**: negations appear
only on atomic predicates. After this pass, `not` nodes only wrap
atomic formulas, and the rest of the structure is built from `and`,
`or`, quantifiers, and atomics.

NNF is the canonical normal form for hashing because:
- It's structurally simpler than the surface (fewer node types in
  the tree's interior).
- It's prover-friendly (most SMT and Datalog backends consume NNF
  natively).
- It collapses syntactic variations (`not(a < b)` and `a >= b`)
  to the same form.

## Predicate-specific normalization

Some atomic predicates have additional normalization rules:

- **Equality is sorted.** `=(a, b)` and `=(b, a)` canonicalize to
  the form where `args[0]` has the smaller canonical hash.
- **Inequality is similarly sorted.**
- **Constants prefer the right.** `a < 5` and `5 > a` both
  canonicalize to `a < 5` (right operand is a constant when one
  exists).
- **Constructor terms are recursively canonicalized.** A `ctor`
  term's args are canonicalized first; then the ctor's hash is
  computed; then its position in any AC-sorted parent is
  determined by that hash.

These rules are mechanical; the spec lists them so all kits
implement them identically.

## Serialization

The canonical AST is rendered to bytes for hashing using a
deterministic encoding:

**Format: canonical CBOR** (RFC 8949). CBOR was chosen over JSON
because:
- Deterministic encoding rules in the standard (RFC 8949 §4.2).
- Native byte-string and integer types (avoids JSON's "everything
  is a string" tax).
- Compact (smaller hash inputs are easier to debug).
- Cross-language libraries are mature and well-tested.

The CBOR encoding uses the deterministic encoding rules: keys in
maps sorted by their CBOR-encoded form; integers in their shortest
representation; floats normalized; etc. Implementations that follow
RFC 8949 §4.2 produce byte-identical output for the same canonical
AST.

**Why not JSON sorted-keys?** JSON has well-known canonicalization
hazards: number representation (1 vs 1.0 vs 1e0), string escape
choices, the lack of integer/float distinction in the spec. CBOR's
deterministic-encoding rules eliminate these. JSON canonical
encodings exist (RFC 8785) but CBOR's tooling and determinism
guarantees are stronger.

(Implementation choice: a kit MAY use JSON canonical encoding RFC
8785 if its host language's CBOR support is poor. The choice is
recorded in the kit's manifest. Mementos produced under the JSON
serialization are NOT cross-comparable with CBOR-serialized
mementos for the same logical claim. The default is CBOR; JSON is a
fallback for legacy host environments.)

## Hash construction

```
canonicalAst = canonicalize(formula)
canonicalBytes = cborEncode(canonicalAst)
propertyHash = sha256(canonicalBytes)[:16 hex chars]
```

`propertyHash` is a 16-character hex string. The first 64 bits of
the sha256 are sufficient for the corpus scales the framework
operates at; collision-resistance at adversarial scale is forward-
compatible (extend to 32 chars for high-assurance deployments;
hash-prefix-32 is the spec's reservation for that case).

## bindingHash construction

The `bindingHash` identifies *what code this property is about*. It
is computed separately from the `propertyHash` because two different
codebases might satisfy the same property; the property hash is the
same in both, but the binding hashes differ.

```
canonicalScope = canonicalize(scope, bindings)
scopeBytes = cborEncode(canonicalScope)
bindingHash = sha256(scopeBytes)[:16 hex chars]
```

Where `canonicalScope` is:

```
CanonicalScope := {
  kind: "function" | "module" | "class" | "method" | "transition" | "region" | "whenever" | <kit-extension>;
  identifier: <type-specific>;
  filePath: string;       // canonical path relative to repo root
  bindings: { [name: string]: { sort: CanonicalSort; nodeRef: NodeRef } };
}
```

The `nodeRef` references the SAST node (or AST node) the binding
points at. The kit's canonicalizer constructs this from its host-
language AST.

For functions: `identifier` is the qualified function name. For
modules: a canonical module path. For methods: receiver-type +
method-name. For regions: start and end positions.

Kit-defined scope kinds are content-addressed extensions following
the same pattern as sorts and predicates.

## Cross-language equivalence — the contract

Two formulas in different host languages are *equivalent* iff their
canonical ASTs are CBOR-byte-identical.

**Sufficient conditions for equivalence** (for two formulas
authored in any two host languages):

1. The IR-formula data structures both produce the same kind of
   top-level node (forall, exists, and, etc.).
2. Sorts canonicalize to the same canonical sort.
3. Bound variables resolve to the same de Bruijn indices.
4. Predicates canonicalize to the same predicate name.
5. Boolean structure normalizes to the same NNF / AC form.
6. Constants serialize to byte-identical CBOR (integers, strings,
   floats all unambiguous).

The kit standard requires each kit's canonicalizer to honor every
rule in this spec. Kits that drift produce non-comparable mementos —
the framework detects this via hash mismatches in cross-language
test fixtures and refuses to certify the kit.

## The `AstCanonicalizer` interface

```typescript
interface AstCanonicalizer {
  /**
   * Compute the bindingHash for a code scope.
   * 
   * Inputs:
   * - scope: the BindingScope (function name, module path, etc.)
   * - bindings: the typed variable bindings the scope provides
   * - hostAst: the host language's AST node(s) the scope refers to
   * 
   * Returns: 16-char hex (sha256-prefix-16 of canonicalScope CBOR).
   */
  bindingHashFromAst(input: {
    scope: BindingScope;
    bindings: Bindings;
    hostAst: HostAstNode;
  }): string;

  /**
   * Compute the propertyHash for an IR formula.
   * 
   * Inputs:
   * - formula: the host's IR-formula data (an IrFormula in TS,
   *   a Formula struct in Go, etc.)
   * 
   * Returns: 16-char hex (sha256-prefix-16 of canonicalAst CBOR).
   */
  propertyHashFromFormula(formula: HostIrValue): string;

  /**
   * Produce the canonical FOL AST for a given formula. Used by
   * downstream producers (Z3 translator, Datalog translator, etc.)
   * to consume the formula in a host-agnostic form.
   */
  formulaToCanonicalAst(formula: HostIrValue): CanonicalFolAst;

  /**
   * Identify the canonical scope of a host AST node.
   */
  scopeOf(hostAst: HostAstNode): BindingScope;

  /**
   * Identify which version of the spec this canonicalizer
   * implements. Mementos produced under a canonicalizer reference
   * this version; cross-canonicalizer compatibility is guaranteed
   * across versions of the same major.
   */
  specVersion(): { major: number; minor: number; patch: number };
}
```

Kits implement this interface in their host language's idiomatic form.
The TypeScript reference lives in `@provekit/ast-canonicalizer`. The
Go reimplementation lives in `provekit/canonicalizer`. Each is a
small library (~1000 LOC) containing the canonicalization rules; the
heavy lifting is the host-language AST walk.

## Implementation strategy

Implementing a canonicalizer is a multi-pass walk over the host's
IR-formula representation:

1. **Pass 1: bound-variable replacement.** Walk the formula tree;
   replace named-variable references with de Bruijn indices.
2. **Pass 2: predicate normalization.** Map host-specific
   predicate names to canonical names. Apply predicate-specific
   normalizations (equality argument sorting, etc.).
3. **Pass 3: sort canonicalization.** Map host-specific sort
   representations to canonical sorts.
4. **Pass 4: implies-removal.** Rewrite `implies(a, c)` →
   `or(not(a), c)`.
5. **Pass 5: NNF.** Push negations inward via De Morgan.
6. **Pass 6: AC normalization.** Flatten `and`/`or`; sort operands;
   deduplicate; remove identity elements.
7. **Pass 7: serialization.** Encode the canonical AST as CBOR
   following RFC 8949 §4.2.
8. **Pass 8: hash.** sha256 the bytes; take prefix-16.

Each pass is independently testable. Test fixtures: pairs of
formulas authored in different host languages that must produce
identical canonical ASTs. The cross-language equivalence test (see
"Acceptance test" below) runs these fixtures through every kit's
canonicalizer.

Implementations may choose to compile multiple passes into a single
walk for performance (the reference TypeScript implementation does
this); the resulting bytes must still match the multi-pass
specification.

## Test corpus

Every kit's canonicalizer must pass the cross-language equivalence
test corpus shipped at `tests/canonicalizer-equivalence/`. The
corpus contains:

- ~50 representative formulas authored in TypeScript, Rust, Go,
  and (where applicable) other supported host languages.
- For each formula, the expected canonical AST (in JSON for
  human-readability) and the expected `propertyHash`.
- Pair-wise equivalence assertions: every formula in language A
  must canonicalize to the same hash as its counterpart in
  languages B, C, D, etc.

The corpus covers:

- Pure quantifier-free formulas (atomic predicates, simple booleans).
- Single-quantifier formulas (`forall x. P(x)`).
- Multi-quantifier formulas (`forall x. exists y. P(x, y)`).
- AC-normalization cases (`and(p, q, r)` ↔ `and(r, q, p)`).
- De Morgan cases (`not(and(p, q))` ↔ `or(not(p), not(q))`).
- Implies-removal cases.
- Equality argument sorting.
- Constructor-term canonicalization.
- Mixed sort cases (Bool, Int, Ref).
- SAST-predicate cases (kindOf, dataFlowsTo, dominates).
- Negative cases — formulas that DIFFER and must produce different
  hashes (same shape but different sort, different predicate, etc.).

A kit that fails any case in the corpus is not spec-compliant and
its mementos are not interoperable with other kits' mementos.

## Acceptance test

The canonicalizer is correct when:

1. A formula authored in TypeScript and the equivalent formula
   authored in Rust both produce byte-identical canonical AST CBOR
   encodings, and therefore matching `propertyHash`.
2. The same holds for Rust ↔ Go, TypeScript ↔ Go, and any
   pair-wise combination of supported host languages.
3. Bound-variable renaming does not affect the canonical form —
   `forAll(b => P(b))` and `forAll(x => P(x))` produce identical
   hashes.
4. AC reordering does not affect the form — `and(p, q)` and
   `and(q, p)` produce identical hashes.
5. De Morgan rewrites collapse — `not(and(p, q))` and `or(not(p),
   not(q))` produce identical hashes.
6. Sort drift between kits is detected — if a kit redefines `Int`
   to mean something other than mathematical integers, the test
   corpus surfaces the incompatibility.
7. Cross-corpus mementos: a memento produced by a TypeScript
   canonicalizer in 2026 is still cross-comparable with a memento
   produced by a Rust canonicalizer in 2030, given both
   canonicalizers implement spec major version 1.

When all seven hold, cross-language equivalence is operational. The
framework's "host language IS the IR" claim becomes verifiable
mechanics rather than aspirational rhetoric.

## Versioning and migration

The canonical-AST grammar is itself versioned via the spec's content
hash. Major versions are breaking (a new node kind that older
canonicalizers can't produce). Minor versions are additive (a new
predicate name that older canonicalizers reject in their host
language but newer ones produce). Patch versions are bug fixes
(e.g., a missed De Morgan rewrite that older canonicalizers got
wrong; mementos under the patched version produce different hashes,
which is the right behavior — old hashes were buggy).

Migration: a memento produced under spec major 1 is comparable with
mementos produced under any spec major-1.x.y. Cross-major-version
mementos are NOT comparable; the framework's swarm-distribution
protocol surfaces version mismatches and routes accordingly.

Kits declare which spec version they implement. Consumers verify
versions match before composing mementos.

## What this enables

With the canonicalizer specified:

- The IR library specs become operational. `@provekit/ir` and
  `provekit/ir` (Go) and equivalent libraries in any host language
  produce comparable IR-formula data, which produces matching hashes.
- Cross-language cross-validation is mechanical, not aspirational.
- The kit-standard's acceptance test (formulas in two host languages
  produce matching propertyHashes) becomes a verifiable property.
- The whitepaper's central claim — *the framework rides every host
  language ever made and the artifacts compose* — is empirically
  defensible against the corpus.
- The fungibility claim (ProvekIt / ProvegIt / ProverIt produce
  interchangeable artifacts) is testable: their canonicalizers must
  pass the same corpus.

The canonicalizer is the keystone. Without it the cross-language
story is rhetoric. With it the story is engineering.
