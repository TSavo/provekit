# Formula DSL extension survey

Some hidden predicates in real languages can't be encoded with the current formula DSL. This doc surveys the gaps, decides extend-pre-launch vs defer-post-launch per gap, and locks the predicate floor for the v1.0.0 baseline catalogs.

## The principle

**The DSL ships at v1.0.0 with a complete-enough surface that the baseline rubric's predicate density floor (2-5 per builtin) is reachable for every kit.** Predicates beyond that floor (research-grade extensions, language-specific exotica) defer to post-launch. The baseline disclaimer addendum names what's missing per language so consumers see the gap; the steward's signature can fill it later.

This is the operational form of: ship the substrate, not perfection.

## Current DSL surface

What every kit's slab can express today (per `Slab.java`, `slab.h/c`, equivalents):

| Constructor / operator | Shape | Example |
|------------------------|-------|---------|
| `forall(binder, sort, body_fn)` | universal quantification | `forall("s", SORT_STRING, s => eq(ctor("len", s), num(0)))` |
| `eq(lhs, rhs)` | equality | `eq(ctor("type_of", x), strConst("int"))` |
| `gte(lhs, rhs)` | greater-than-or-equal | `gte(ctor("len", s), num(1))` |
| `ctor(op, args...)` | function / constructor application | `ctor("blake3_512", b)` |
| `num(n)` | integer constant | `num(64)` |
| `strConst(s)` | string constant | `strConst("ed25519:")` |
| `startsWith(haystack, prefix)` | string prefix check | `startsWith(s, strConst("blake3-512:"))` |

Sorts: only `SORT_STRING` is wired through. The DSL accepts additional sorts at the type level but no kit currently quantifies over them.

Operations (`ctor` ops): a kit-extensible set. The cross-kit shared subset includes `len`, `len_bytes`, `blake3_512`, `blake3_digest`, `jcs_encode_utf8`, `cbor_encode_*`, `ed25519_sign_*`, `proof_envelope_*`, etc. Each kit can add its own ops.

The DSL is shape-bound: it expresses type signatures, length bounds, identifier-prefix patterns, equality / lower-bound comparisons of constructed values. This is enough for most stdlib predicates that real programs depend on.

## The gaps

For each gap: a description, an example builtin it blocks, the impact, and the decision (extend / defer).

### G1: Numeric range bounds (lt, lte, between)

The DSL has `gte` but no `lt` / `lte` / `between(lo, hi)`. Bounds like "result is non-negative and fits in i64" can be expressed with `gte` only on one side.

**Example builtins blocked**: `String::len` returning a value in `[0, usize::MAX]`; `Math.floor()` returning something in `[INT64_MIN, INT64_MAX]`; `array_pop()` reducing length by exactly 1 (need `lte` to bound the change).

**Impact**: ~20% of stdlib builtins across all 12 languages have a natural upper bound that's currently inexpressible.

**Decision: EXTEND pre-launch.** Cheap addition (mirrors `gte`'s implementation), broad applicability, no substrate-level changes. Add `lt`, `lte`, `between(lo, hi)` to the formula DSL across all kits.

### G2: Finite-set membership

No predicate for "x is in {a, b, c}" beyond chained `eq` with `or` (and there's no `or`). Useful for finite enums and coercion classes.

**Example builtins blocked**: PHP's falsy class: `is_falsy(x)` iff `x ∈ {false, 0, 0.0, "", "0", null, []}`. Without finite-set membership, the predicate is inexpressible.

**Impact**: PHP baseline is partial without it (~15-20 builtins involve type juggling). Smaller impact on other languages.

**Decision: EXTEND pre-launch.** Cheap addition. The implementation is `member_of(value, [a, b, c, ...])`, array of constants on the right. Closes the PHP type-juggling gap at floor density.

### G3: Logical disjunction (or)

The DSL has implicit conjunction (a Slab is a conjunction of contracts) but no explicit `or`. Predicates like "result is null OR positive" need `or`.

**Example builtins blocked**: nullable returns: `Map.get(key)` returns `Option<V>` (Some-or-None). Currently expressible by handling the two cases in separate contracts, but the predicate-per-builtin density inflates.

**Impact**: cross-cutting; affects ~10-15 builtins per kit that have nullable returns.

**Decision: EXTEND pre-launch.** Pairs naturally with G1 / G2. `or(a, b)` and `or_n(a, b, c, ...)`. Cheap, broad value.

### G4: Negation (not)

No `not` in the DSL. Predicates like "x is not null" or "f doesn't throw" need it.

**Decision: EXTEND pre-launch.** Trivial addition. Often paired with `or` to form full propositional logic.

After G1-G4, the DSL covers propositional logic over equality / range / membership predicates. That's enough for the vast majority of stdlib contracts.

### G5: Structural typing (TypeScript)

TypeScript types are structural: `type Foo = { x: number }` is anything-with-an-x-of-type-number. No nominal identity. The DSL has `eq` over constructed values but no notion of "shape compatibility."

**Example builtins blocked**: most TS stdlib methods that take object parameters with structural types (`Array.from(arrayLike)`, `Object.keys(obj)`, etc.). Their preconditions are "argument has these fields with these types", not expressible as `eq(type_of(arg), strConst("Foo"))` because TS has no nominal Foo.

**Impact**: severe for TypeScript. ~30+ ts builtins blocked at floor density.

**Decision: DEFER post-launch.** Structural typing is research-grade: it interacts with subtyping, variance, intersection / union types. Foundation TS baseline ships with `any`-typed predicates (loose: "argument is an object") and notes the gap in the disclaimer addendum. Filed as follow-up: `[spec] formula DSL: structural typing predicates for TypeScript`.

### G6: Effect / monad tracking (async, throws, IO)

"This function returns Promise<T>", "this function throws E", "this function reads from FS." No first-class effect predicate.

**Example builtins blocked**: async builtins (Promise, async/await), throwing builtins (anything in Result<T, E>), IO builtins (file/network/process).

**Impact**: cross-cutting. Affects ~15-20% of stdlib across all languages. Without it, many predicates collapse to "this function returns", true but vacuous.

**Decision: DEFER post-launch.** Effect tracking is research-grade and interacts with the IR's totality model. Foundation baselines ship without effect predicates; the disclaimer notes "side-effect properties not encoded." Filed as follow-up: `[spec] formula DSL: effect predicates (async / throws / IO)`.

### G7: Aliasing / ownership (rust, c, c++, zig)

"This slice doesn't alias that slice." "This pointer is uniquely owned." Critical for systems-language safety.

**Example builtins blocked**: `slice::split_at_mut` (returns two non-overlapping mutable references); `memcpy(dst, src, n)` (precondition: dst and src don't overlap); `Box::leak`.

**Impact**: severe for systems-language baselines (rust, c, c++, zig). Probably ~10-15 builtins per language that have non-aliasing as a precondition.

**Decision: DEFER post-launch.** Aliasing predicates are research-grade: separation logic, region inference, etc. Foundation baselines for systems languages ship without aliasing predicates; the disclaimer addendum names the gap explicitly ("Rust unsafe operations and pointer-aliasing preconditions are not encoded; the rust-lang team's signature can add them"). Filed as follow-up: `[spec] formula DSL: aliasing / ownership predicates`.

### G8: Dynamic dispatch / reflection (Python __getattr__, JS Proxy, Ruby method_missing)

Attribute access depends on runtime state via `__getattr__`, `Proxy`, `method_missing`, etc. The IR has no notion of "dispatched-to method depends on runtime."

**Example builtins blocked**: most Python builtins are statically typed at the Python level; the gap mostly affects user code, not stdlib. Same for Ruby: most core methods are statically dispatched.

**Impact**: minor for stdlib baselines. Major for codebases that lean on dynamic dispatch heavily.

**Decision: DEFER post-launch.** Mostly affects code USING dynamic dispatch, not stdlib builtins. Foundation Python / Ruby baselines aren't materially impacted. Filed as follow-up: `[spec] formula DSL: dynamic dispatch / reflection predicates`.

### G9: Side-effect ordering / temporal logic

"f(x) happens-before g(x)." "After f returns, this resource is closed." Temporal logic.

**Decision: DEFER post-launch.** Temporal logic is research-grade. Probably never appears in stdlib floor predicates. Filed as follow-up: `[spec] formula DSL: temporal predicates`.

### G10: Numeric precision / fixed-width modulus

"u32 + u32 wraps at 2^32." Fixed-width arithmetic semantics.

**Decision: DEFER post-launch.** Most stdlib floor predicates don't need precise modular arithmetic. Filed as follow-up: `[spec] formula DSL: fixed-width arithmetic`.

## Summary

| Gap | Pre-launch | Decision |
|-----|------------|----------|
| G1: lt / lte / between | YES | extend |
| G2: member_of | YES | extend |
| G3: or | YES | extend |
| G4: not | YES | extend |
| G5: structural typing | NO | defer; TS disclaimer notes gap |
| G6: effect tracking | NO | defer; all-language disclaimers note gap |
| G7: aliasing | NO | defer; systems-lang disclaimers note gap |
| G8: dynamic dispatch | NO | defer; minor stdlib impact |
| G9: temporal | NO | defer; research-grade |
| G10: fixed-width arith | NO | defer; minor stdlib impact |

**Pre-launch DSL extensions: 4** (G1-G4). All cheap, all complete propositional logic over what's already there. No substrate-level (CDDL / Rust canonical / cross-kit envelope) changes: just adding constructors to each kit's slab DSL.

**Deferred to post-launch: 6** (G5-G10). Each gets a follow-up issue and an explicit note in the relevant per-language baseline disclaimer addendum.

## Implementation plan for G1-G4

Each is a per-kit code addition to the formula DSL. The shape across kits should match (cross-kit byte-equivalence depends on it). Suggested order:

1. **Spec the new constructors** in `protocol/specs/`: JCS Value tree shape for `lt`, `lte`, `between`, `member_of`, `or`, `or_n`, `not`. Land as a contract memento in the catalog format spec. (~1 PR)

2. **Reference impl in rust**: extend `sugar-self-contracts/src/lib.rs` slab DSL helpers. Mint a test catalog using each new constructor. Validate byte-equivalence stays clean. (~1 PR)

3. **Mirror across the other 11 kits**: go, cpp, ts, csharp, swift, java, python, ruby, zig, c, php each get their slab DSL extended with the same 4 helpers. Each kit's `mint-X-self-contracts` orchestrator gets a regression test. (~12 PRs, parallelizable per the per-kit agent pattern.)

4. **Per-kit baseline catalogs use the new constructors**: once all 12 kits speak the extended DSL, baseline catalog authoring uses G1-G4 freely. The rust pilot (#257) validates the loop.

## Disclaimer addendum template

For each per-language baseline, the disclaimer addendum (per the [baseline catalog rubric](baseline-catalog-rubric.md)) names which deferred predicate shapes are NOT encoded for that language:

```
Predicate gaps in this baseline (deferred to post-launch):
  - [G6 effect tracking]: side-effect properties (async, throws, IO) not encoded
  - [G7 aliasing]: pointer-aliasing preconditions not encoded
  - ...

The authoritative signer for this language can add these predicates;
the foundation baseline ships at the floor density only.
```

The list is per-language. TypeScript's addendum names G5 (structural typing). Rust / c / c++ / zig name G7 (aliasing). Python / Ruby may name G8 (dynamic dispatch) if their baseline is impacted in practice. All baselines name G6 (effect tracking) since it's universal.

## What this rubric is NOT

- It is not a full type system. The DSL captures operational predicates programs rely on; it doesn't model the full semantics of any language.
- It is not the predicate ceiling. It's the floor for v1.0.0; future DSL extensions can land per the deferred-issue follow-ups.
- It is not language-agnostic. Each language has predicate shapes only it cares about; this rubric covers the cross-cutting gaps.

## See also

- #253 launch v1.0.0 epic
- #254 / `baseline-catalog-rubric.md`: what counts as a basic catalog
- #255 / `signing-your-own-catalog.md`: federation mechanism
- #257 rust pilot: first-mover validating the extended DSL
