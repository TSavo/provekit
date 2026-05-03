# ProvekIt compared to Dafny, TLA+, Alloy (specification languages)

Dafny, TLA+, and Alloy are specification and verification languages with different sweet spots. Like Coq/F\*/Lean, they are formal verification tools, not protocols. ProvekIt is a substrate over which their outputs can become portable.

## Quick comparison

| Tool | Category | Sweet spot |
|---|---|---|
| **Dafny** | Imperative-with-specs language | Functional correctness of algorithms; small TCB |
| **TLA+** | Temporal/state-machine language | Distributed systems; concurrency; protocol design |
| **Alloy** | Lightweight relational logic | Architecture exploration; bounded checks |
| **ProvekIt** | Protocol for content-addressing the above | Federation, distribution, cross-tool composition |

If you're modeling a distributed protocol, use TLA+. If you're verifying an algorithm, use Dafny. If you're exploring an architecture, use Alloy. None of those is replaced by ProvekIt; ProvekIt's role is downstream.

## Dafny

Dafny is a language with built-in pre/postcondition specifications and an integrated SMT-backed verifier. You write functions with annotations:

```dafny
method Sort(a: array<int>)
    requires a != null
    modifies a
    ensures multiset(a[..]) == old(multiset(a[..]))
    ensures forall i, j :: 0 <= i < j < a.Length ==> a[i] <= a[j]
{
    ...
}
```

Dafny's verifier (Boogie + Z3) checks the annotations.

**Sweet spot**: classical algorithm verification, small TCB, cleaner than Coq for algorithm-style proofs.

**Limitation**: Dafny's compilation is more limited; integration with mainstream language ecosystems (Rust, Python, etc.) is rough.

**ProvekIt complement**: `provekit-lift-dafny` (not yet in the roadmap, but structurally simple) could lift Dafny's `requires`/`ensures` into canonical IR. The Dafny-verified function's contracts then become portable across languages via bridges to reference contracts.

## TLA+

TLA+ is a specification language for state-machine and temporal properties. It models systems as state machines and specifications as temporal logic formulas:

```tla
Init == counter = 0
Next == counter' = counter + 1
Spec == Init /\ [][Next]_counter
TypeInvariant == counter \in Nat
```

TLA+'s tools (TLC model checker, TLAPS prover) verify against these specs.

**Sweet spot**: distributed system design, concurrency, protocol-level properties (deadlock freedom, eventual consistency, refinement). Used at Amazon (Dynamo, S3), Microsoft (Azure Cosmos DB), and others for high-stakes distributed systems.

**Limitation**: TLA+ models don't typically connect to actual code. The model is abstract; matching code is a separate practice.

**ProvekIt complement**: TLA+ models trafficking in temporal predicates need IR primitives that capture temporal logic. The current IR doesn't support these (would be a spec change). With temporal IR primitives, TLA+'s verified specifications could become portable contracts.

The use case is narrower than Dafny / Coq / Lean: not every codebase has TLA+ specifications. But for the codebases that do (mostly distributed systems), federating TLA+ verification via ProvekIt would be high-value.

## Alloy

Alloy is a relational logic language. Specs are written in a constraint-style:

```alloy
sig User { email: one Email, age: one Int }
sig Email {}

fact ValidAge { all u: User | u.age > 0 and u.age < 150 }

run { some User } for 5
```

The Alloy Analyzer searches a bounded universe for instances satisfying or violating the spec.

**Sweet spot**: lightweight architectural exploration. Bounded checks for "could this scenario occur?" Cheap to write, cheap to run, gives quick design feedback.

**Limitation**: bounded checks. Alloy doesn't prove unbounded properties; it explores small instances.

**ProvekIt complement**: Alloy specs that hold within a documented bound could become canonical IR contracts with the bound captured. The IR has bounded-quantifier shapes; Alloy specs lift cleanly.

Less load-bearing than Dafny / TLA+ because Alloy is typically used for exploration, not for shipping verified components.

## When to use each

```
Algorithmic verification (sorting, search, data structures)
  ↓
Pure-Rust? → Kani / Prusti / Creusot / Flux
Pure-Coq/Lean stack? → Coq / Lean
Cross-language? → Dafny + ProvekIt OR Coq/Lean + ProvekIt

Distributed system design
  ↓
TLA+ (alone, until ProvekIt has temporal IR primitives)

Architecture exploration
  ↓
Alloy (alone)

Behavioral contracts on existing annotation-style codebases
  ↓
ProvekIt + your existing annotation library (zod, pydantic, Bean Validation, etc.)
```

The decision of which framework to use is independent from the decision of whether to add ProvekIt. ProvekIt is a federation layer; it does not replace any of these.

## What ProvekIt would need to integrate with each

The integration story per framework:

### Dafny
- Lift adapter: walk Dafny `requires`/`ensures`, emit canonical IR.
- Backend: Dafny → Boogie → Z3; ProvekIt accepts Z3 evidence terms.
- Status: not on the roadmap; structurally feasible.

### TLA+
- Lift adapter: walk TLA+ specifications, emit canonical IR. Requires temporal IR primitives.
- Backend: TLAPS or TLC. TLAPS produces constructive proofs; TLC produces counterexamples.
- Status: substantial spec change required; not currently planned.

### Alloy
- Lift adapter: walk Alloy signatures and facts, emit canonical IR. Bounded quantifiers map cleanly.
- Backend: Alloy Analyzer (Z3 underneath).
- Status: not on the roadmap; structurally feasible.

The pattern across all three: lifting the source language's specifications and accepting the source language's verifier output as evidence. None of the integrations is unique; each follows the same shape as the planned Kani / Prusti adapters.

## What ProvekIt does not provide for any of these tools

ProvekIt does not improve the tool's verification capabilities. It does not help Dafny prove what Dafny couldn't prove. It does not extend TLA+'s temporal logic. It does not increase Alloy's exploration depth.

What ProvekIt provides is downstream: once you've used the tool to produce a verification, ProvekIt makes the verification portable. This is exactly the shape of the protocol's role across all formal verification frameworks.

## Read next

- [coq-fstar-lean.md](coq-fstar-lean.md) — interactive theorem provers.
- [kani-prusti-creusot.md](kani-prusti-creusot.md) — Rust-specific provers.
- [`../../contributing/proposing-a-spec-change.md`](../../contributing/proposing-a-spec-change.md) — adding new IR primitives (e.g., temporal primitives for TLA+).
- [`../boundaries.md`](../boundaries.md) — what ProvekIt is NOT.
