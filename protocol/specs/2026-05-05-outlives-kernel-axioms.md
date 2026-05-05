# Outlives Kernel Axioms and Region-Quantifier Composition — Normative Spec

**Status:** v1.0 normative
**Date:** 2026-05-05
**Closes:** #403
**Related:**
- `2026-04-30-ir-formal-grammar.md` — IrFormula and IrSort definitions; `Sort::Region` is the region sort
- `2026-04-30-contract-merge-semantics.md` — `compose_function_contracts` procedure this spec augments
- `2026-05-03-substrate-layers-envelope-header-body.md` — substrate layer framing
- `2026-04-29-the-proof-substrate.md` — kernel theory layer overview
- `2026-05-05-loop-invariant-memento.md` — sibling discharge memento for `Effect::OpaqueLoop`
- `2026-05-05-try-branch-memento.md` — sibling discharge memento for `Effect::EarlyReturn`

---

## §0. Purpose

Rust functions carry lifetime parameters that constrain how long references remain valid. These constraints are part of a function's semantic contract: a caller that violates them triggers undefined behavior. ProvekIt must represent and discharge these constraints at the substrate level, not merely treat them as syntactic annotations.

This spec defines:

1. `Outlives` as a kernel predicate — how it is encoded, what axioms govern it, and where it lives in the theory layer.
2. The `where 'a: 'b` lifting rule — how where-clause outlives constraints become FACTS in a function's pre-condition.
3. The region-quantifier composition rule — how the substrate discharges `Outlives` demands when composing caller and callee contracts at a callsite.
4. The `'static` top-element axiom and its encoding.
5. The out-of-scope boundary for late-bound (HRTB) regions in v1.

`Outlives` is NOT a memento-discharged effect. It is a built-in atomic predicate that the substrate prover evaluates directly from the fact set in scope. No certificate is required; the prover either discharges the demand from the available axioms and facts, or it refuses composition.

---

## §1. Predicate signature

```
Outlives(r1: Region, r2: Region) -> Bool
```

`Outlives(r1, r2)` means: the region `r1` lives at least as long as the region `r2`. In Rust notation, this is `'r1: 'r2`.

`Region` is a sort in the kernel theory: `Sort::Region { name: <string> }`. Regions are first-class terms at the theory layer; they are not erased before the substrate sees them unless Charon monomorphizes them away (see §7).

`Outlives` is a built-in atomic predicate, equivalent in theory-layer status to `=` (equality) and `>=` (integer ordering). It is NOT defined by axiom schemas in a user-extensible way; the three axioms in §2 are the complete closed-world definition.

### §1.1 Region encoding

A region named `'a` is encoded as `Sort::Region { name: "'a" }`. The `'static` lifetime is encoded as `Sort::Region { name: "'static" }`. There is no separate sort variant for `'static`; it is a named region constant that the axioms treat specially.

A region appearing in a function signature as a lifetime parameter (e.g. `<'a>`) is encoded as a universally quantified variable of sort `Region` in the lifted `FunctionContractMemento`. The contract's formals list includes all early-bound region parameters alongside value parameters.

---

## §2. Axioms

The kernel theory asserts the following three axioms unconditionally. A conforming substrate prover MUST treat these as built-in rewrite rules that fire without external evidence.

### §2.1 Axiom 1: Reflexivity

**Formal statement:**

```
∀ r: Region. Outlives(r, r)
```

**Gloss:** Every region outlives itself. A reference to region `r` is always valid within that same region. This axiom fires automatically; no fact needs to establish `Outlives(r, r)` explicitly.

**Prover rule:** When the prover checks `Outlives(r, r)` for any region term `r`, it succeeds immediately without consulting the fact set.

### §2.2 Axiom 2: Transitivity

**Formal statement:**

```
∀ a, b, c: Region. Outlives(a, b) ∧ Outlives(b, c) → Outlives(a, c)
```

**Gloss:** If `'a` outlives `'b` and `'b` outlives `'c`, then `'a` outlives `'c`. The prover may chain any number of steps through the fact set.

**Prover rule:** When the prover checks `Outlives(a, c)` and cannot discharge it directly from a base fact, it searches the fact set for any intermediate region `b` such that both `Outlives(a, b)` and `Outlives(b, c)` are dischargeable. This is a DAG reachability check over the region ordering graph (see §8).

### §2.3 Axiom 3: `'static` top element

**Formal statement:**

```
∀ r: Region. Outlives("'static", r)
```

**Gloss:** `'static` outlives every region. In Rust's lifetime model, `'static` is the longest-lived region, lasting for the entire program execution. Under this spec's definition, `Outlives(r1, r2)` means `r1` lasts at least as long as `r2`, so `Outlives("'static", r)` holds for all `r` because `'static` lasts at least as long as anything else.

**Prover rule:** When the prover checks `Outlives("'static", r)` for any region `r`, it succeeds immediately.

**Encoding note:** `'static` is `Sort::Region { name: "'static" }`, not a separate sort variant. The prover identifies it by name equality when applying this axiom.

---

## §3. Lifting `where 'a: 'b` clauses

When the Rust lifter processes a function declaration with lifetime where-clauses, each clause of the form `where 'a: 'b` is lifted to a FACT in the function's pre-condition.

### §3.1 Lifting procedure

Given a function:

```rust
fn f<'a, 'b>(...) where 'a: 'b { ... }
```

The lifter emits a `FunctionContractMemento` where:

1. The formals include `'a: Region` and `'b: Region` as region parameters alongside value parameters.
2. The `pre` formula includes `Outlives("'a", "'b")` as a conjunct.

The function body sees `Outlives("'a", "'b")` as a free fact. The caller is obligated to establish it at every callsite.

### §3.2 FACT semantics

A where-clause fact is a FACT, not a REQUIRE: the substrate does not ask the body to prove it. The body may use it freely in proofs. The burden of establishing it lies entirely with the caller at composition time (see §4).

### §3.3 Signature-implied Outlives facts

Some Outlives constraints are implicit in the signature rather than explicit in a where-clause. For example:

```rust
fn id<'a>(x: &'a u32) -> &'a u32
```

The return type `&'a u32` implies that the output reference lives for `'a`. The lifter does NOT auto-generate an `Outlives` fact from return-type annotations; those constraints flow from the calling context. The lifter only emits explicit `Outlives` facts for explicit `where 'a: 'b` clauses. Implicit constraints are handled by the caller substituting concrete regions and checking the three axioms.

---

## §4. Region-quantifier composition rule

This section defines the procedure the substrate follows when composing a caller's `FunctionContractMemento` with a callee's `FunctionContractMemento` at a callsite. It augments the base `compose_function_contracts` procedure in `2026-04-30-contract-merge-semantics.md`.

### §4.1 Inputs

- `caller_facts`: the set of `Outlives(r_i, r_j)` facts in scope at the callsite, derived from the caller's own pre-condition (its where-clause facts plus the three axioms).
- `callee_pre_outlives`: the set of `Outlives(r_callee_p, r_callee_q)` demands in the callee's pre-condition.
- `region_subst`: a substitution mapping each callee region parameter to a caller region. This substitution is established by the callsite: each callee lifetime argument maps to a concrete region in the caller's scope.

### §4.2 Substitution

Apply `region_subst` to every demand in `callee_pre_outlives`:

```
substituted_demands = { Outlives(subst(r_callee_p), subst(r_callee_q))
                        | Outlives(r_callee_p, r_callee_q) ∈ callee_pre_outlives }
```

After substitution, all region terms in `substituted_demands` are drawn from the caller's region vocabulary.

### §4.3 Discharge attempt

For each `Outlives(a, b) ∈ substituted_demands`:

1. If `a == b` (syntactic equality of region names): discharge by Reflexivity (§2.1). Continue to next demand.
2. If `a == "'static"`: discharge by `'static` top (§2.3). Continue to next demand.
3. Search `caller_facts` and the closure of `caller_facts` under Transitivity (§2.2) for `Outlives(a, b)`. If found: discharge. Continue to next demand.
4. If none of the above applies: the demand CANNOT be discharged. Record it as a `RegionCompositionFailure { demanded: Outlives(a, b) }`.

### §4.4 Composition outcome

- If `substituted_demands` is empty after substitution: region composition succeeds trivially. No region checking needed.
- If all demands are discharged: region composition succeeds. Proceed with the rest of `compose_function_contracts`.
- If any demand is NOT discharged: composition fails. The substrate returns `CompositionError::OutlivesNotDischarged { failures: Vec<RegionCompositionFailure> }`. No partial composition is emitted.

### §4.5 Region-only functions

A function with no region parameters in its signature (after Charon monomorphization) has an empty `callee_pre_outlives` set. Composition succeeds trivially for region demands; other composition checks proceed normally.

---

## §5. Worked examples

### §5.1 Simple case: identity over a reference

```rust
fn id<'a>(x: &'a u32) -> &'a u32 { x }
```

**Lifted contract:**

- Formals: `x: &'a u32` (with `'a: Region` as a region parameter), return: `&'a u32`.
- Pre: `true` (no where-clauses; no explicit Outlives facts).
- Post: `result = x`.
- `callee_pre_outlives`: empty set (no where-clauses).

**At a callsite:**

```rust
let v: u32 = 42;
let r: &'b u32 = id::<'b>(&v);
```

Callsite establishes `region_subst = { 'a -> 'b }`. The callee's `callee_pre_outlives` is empty. Composition succeeds trivially; no Outlives discharge needed.

The return type substitutes to `&'b u32`, which is what the caller expects.

### §5.2 Where-clause case: longer

```rust
fn longer<'a, 'b: 'a>(x: &'a u32, y: &'b u32) -> &'a u32 { x }
```

**Lifted contract:**

- Formals: `x: &'a u32`, `y: &'b u32` (with `'a: Region`, `'b: Region`).
- Pre: `Outlives("'b", "'a")` (from `where 'b: 'a`).
- Post: `result = x`.
- `callee_pre_outlives`: `{ Outlives("'b", "'a") }`.

**At a callsite where caller has `'c: 'short` and `'long: 'short`:**

```
region_subst = { 'a -> 'short, 'b -> 'long }
```

Step 1: Substitute: `{ Outlives("'long", "'short") }`.

Step 2: Search `caller_facts`. Caller has `Outlives("'long", "'short")` as a direct fact (from its own where-clause `'long: 'short`). Discharge by direct fact lookup.

Composition succeeds.

### §5.3 Negative case: unrelated regions, no Outlives available

```rust
fn f<'a, 'b>(x: &'a u32, y: &'b u32) -> &'a u32 { x }
```

**Lifted contract:**

- Formals: `x: &'a u32`, `y: &'b u32`.
- Pre: `true` (no where-clauses).
- `callee_pre_outlives`: empty.

This function makes NO `Outlives` demand. Composition succeeds for any caller. The body's use of `x` as the return value is consistent because the return type is `&'a u32` and `x` has type `&'a u32`; no cross-region constraint is needed.

**Negative variant:** If the body were instead:

```rust
fn g<'a, 'b>(x: &'a u32, y: &'b u32) -> &'a u32 { y }
//                                                   ^ wrong: &'b u32, not &'a u32
```

Rust would reject this at compile time (lifetime mismatch). The lifter would either (a) refuse to emit a contract for this function because the Rust compiler already rejects it, or (b) emit a contract with an `Outlives("'b", "'a")` demand in the pre to make the body sound. Since Charon only processes well-typed programs, case (a) applies: this function never reaches the lifter.

**Explicit negative composition case:** Suppose a function `h<'a, 'b: 'a>` (with `where 'b: 'a`) is called from a context where the caller has `'x` and `'y` with NO `Outlives("'y", "'x")` fact established:

```
region_subst = { 'a -> 'x, 'b -> 'y }
callee demand after substitution: Outlives("'y", "'x")
caller_facts: {} (empty — no where-clauses in caller)
```

Step 1: `'y != 'x`, not reflexivity.
Step 2: `'y != "'static"`, not `'static` top.
Step 3: `Outlives("'y", "'x")` not in `caller_facts` or its transitive closure.
Step 4: Record `RegionCompositionFailure { demanded: Outlives("'y", "'x") }`.

Composition returns `CompositionError::OutlivesNotDischarged { failures: [Outlives("'y", "'x")] }`.

---

## §6. Out of scope (v1): late-bound regions (HRTB)

Higher-Ranked Trait Bounds (HRTB) introduce regions that are quantified at the trait level rather than the function level. In Rust syntax: `for<'a> Fn(&'a u32)`. These regions are NOT early-bound; they are instantiated fresh at each call, not at the function's generic instantiation site.

**v1 decision:** This spec covers early-bound region behavior only. Late-bound regions are flagged as out of scope for v1.

**Lifter behavior:** When the Rust lifter (provekit-walk) encounters a late-bound region in Charon's IR, it emits `Effect::HigherRankedRegion { binder_id: <string>, region_name: <string> }` into the function's effects list and does NOT emit an `Outlives` fact for that region. The function contract is marked opaque at that point.

**Substrate behavior:** The substrate composition guard refuses to compose any `FunctionContractMemento` that contains an `Effect::HigherRankedRegion` effect. The error returned is `CompositionError::HigherRankedRegionUndischarged { binder_id, region_name }`. No discharge certificate is defined in v1; there is no `HigherRankedRegionMemento`.

**Rationale:** HRTB requires a full universal introduction rule at the proof level. The v1 substrate operates on a fixed region vocabulary per callsite. Extending to HRTB is reserved for a future spec revision. The refused-on-effect mechanism ensures v1 does not silently accept unsound region reasoning.

**Scope:** This refusal applies to direct HRTB occurrences in the lifted IR. If Charon monomorphizes a HRTB-bearing function before lifting (which may happen for concrete instantiations of trait objects), the monomorphized form has no late-bound regions and composes normally under the rules in §4.

---

## §7. RFC questions resolved

### §7.1 Region constants beyond `'static`

The v1 spec covers `'static` only. Other named region constants (e.g. `'erased` in trait object contexts) are treated as ordinary region variables. There is no special axiom for them. If a Charon-emitted IR uses `'erased`, the lifter emits it as a region variable; the prover treats it identically to any other region name and attempts discharge via the three axioms plus the available fact set.

If a future version needs special constants beyond `'static`, a new axiom (e.g. "Axiom 4: `'erased` top") can be added to this spec via the standard protocol versioning process.

### §7.2 Composition with monomorphization

When Charon monomorphizes a generic function, it substitutes concrete types for type parameters and may also substitute or erase region parameters. When regions are erased by monomorphization, the lifter does NOT emit `Outlives` predicates for those erased regions. The lifted contract of the monomorphized form simply has fewer or no region formals.

No special handling is needed: the spec defines what happens for regions that DO appear in the lifted IR. If a region does not appear, no `Outlives` fact or demand exists for it, and the composition rule in §4 handles the empty-demand case trivially (§4.4).

### §7.3 HRTB

As specified in §6: refused-on-effect in v1. The `Effect::HigherRankedRegion` emission is the boundary marker.

---

## §8. Substrate prover implementation guidance

This section specifies the data structures and algorithms an implementation MUST provide to evaluate `Outlives` demands at composition time.

### §8.1 Region fact set representation

The prover maintains a **region fact set** per function context. It is a directed graph `G = (V, E)` where:

- `V`: the set of region names appearing in the current function's formals and pre-condition, plus `"'static"`.
- `E`: a directed edge `(r1, r2)` if `Outlives(r1, r2)` is a direct fact in the pre-condition.

The three axioms are NOT stored as edges in `G`; they are evaluated on-the-fly:
- Reflexivity: checked syntactically before any graph lookup.
- `'static` top: checked by testing `r1 == "'static"` before any graph lookup.
- Transitivity: evaluated as reachability in `G`.

### §8.2 Reachability for Transitivity

To discharge `Outlives(a, c)`:

1. If `a == c`: succeed (Reflexivity).
2. If `a == "'static"`: succeed (`'static` top).
3. Run a depth-first or breadth-first reachability search from node `a` in `G`. If `c` is reachable from `a`: succeed (Transitivity).
4. Otherwise: fail.

The region ordering graph is acyclic in well-typed Rust (cycles would imply all regions are equivalent, which Charon does not emit). Implementations MAY assume acyclicity; a conforming implementation SHOULD detect and reject cycles in the fact set as malformed input.

### §8.3 Incremental fact addition

When composing a chain of calls, the prover may accumulate facts across composition steps. The graph `G` grows monotonically within a single function analysis context. No fact is ever retracted.

### §8.4 Equivalence classes

Region equivalence is NOT part of the v1 spec. Two distinct region names `'a` and `'b` are never identified unless an explicit `Outlives('a, 'b)` AND `Outlives('b, 'a)` both hold (a cycle, which Charon does not emit for well-typed programs). Implementations MUST NOT merge region names.

### §8.5 Demanded-Outlives index

For composition performance, the prover SHOULD maintain an index keyed by the demanded region pair `(r1, r2)`. On a cache hit the discharge is O(1). On a miss, transitivity search is O(V + E) over the region graph, which is bounded by the number of region parameters in scope (typically small: fewer than 10 in real Rust functions).

### §8.6 Error reporting

When composition fails, the error payload MUST include every undischarged demand, not just the first. The full list of `RegionCompositionFailure` entries allows the caller to surface all missing constraints to the engineer, rather than requiring iterative recompilation.

---

## §9. Cross-references

- The region sort is defined in `2026-04-30-ir-formal-grammar.md` as `Sort::Region { name: tstr }`.
- The `compose_function_contracts` procedure lives in `implementations/rust/provekit-walk/src/contract.rs`. The region composition check defined in §4 MUST be inserted as a pre-check before the existing pre/post substitution steps.
- The `Effect::HigherRankedRegion` variant is NEW. It must be added to the `Effect` enum in `implementations/rust/provekit-walk/src/contract.rs` and recognized by `EffectSet::check_opacity`.
- The `FunctionContractMemento` formals list is defined in `2026-04-30-memento-envelope-grammar.md`. Region parameters are added as formals of sort `Sort::Region { name: ... }`.
- `CompositionError::OutlivesNotDischarged` and `CompositionError::HigherRankedRegionUndischarged` are NEW error variants. They must be added to the `CompositionError` enum alongside the existing `OpacityError` variants.
- For the implementation tracking issue, see #403 and #384 (C.9 theory work).
