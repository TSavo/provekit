# Value-tier ops are a family, not THE concept

Date: 2026-05-19
Status: Active. Establishes the architectural principle that future kit
authors (classical, quantum, cellular, dataflow, neuromorphic,
stack-machine, combinator, or any platform not yet imagined) must follow.

Companion to `docs/explanation/substrate-uniform-pattern.md` (the rules
doc) section 10 (open-extensibility).

## The principle

The substrate is fundamentally about Term composition. Every "value
appearing in code" is a Term node; the concept-CID on the Term names what
KIND of value-tier construct it is. There is NO substrate-canonical "value"
op. `concept:literal` is the CLASSICAL value-tier construct, one member of
a family that grows as new platforms register.

Each architecture's kit declares its own value-tier ops:

- **Classical platforms** (Rust, Java, Python, TypeScript, C, Go, etc.):
  `concept:literal { value, sort }`. Single concrete value at the position;
  sort drawn from the substrate's canonical sort catalog
  (`menagerie/concept-shapes/catalog/sorts/`).
- **Quantum platforms**: `concept:superposition`,
  `concept:basis-state`, `concept:complex-amplitude`, `concept:measure`,
  etc. The "value" at a position is a QUANTUM STATE, not a single
  classical value; the state composes from amplitude-and-basis pairs.
- **Cellular automata**: `concept:cell-state` (one of an enumerated
  alphabet per rule), `concept:cell-neighborhood` (composing adjacent
  cells).
- **Dataflow architectures**: `concept:stream-element`,
  `concept:trigger-event` (data arrival triggers; no positional
  literal).
- **Neuromorphic substrates**: `concept:spike-train`,
  `concept:synaptic-weight`.
- **Stack machines (Forth-like)**: `concept:stack-element`,
  `concept:return-stack-element`.
- **Combinator/lambda machines**: `concept:combinator` (pure substitution
  semantics).
- **Future architectures not yet imagined**: their kits mint value-tier
  ops that fit their domain.

All family members share the same substrate machinery: composition via
`platform_semantics_for_binding`, characterization via `compare_op_with`,
propagation via `propagate_effects` (or domain-specific propagation
primitives filed via ruling when problem-domain boundaries cross), realize
via `dispatch_realize`. None of those primitives know "literal" specially.
They operate on Term composition uniformly.

## Worked example 1: classical literal

Source code (Rust): `let x: i32 = 42;`

Lifted term (in term_shape):

```
Term::Op(concept:literal, [], {
  value: 42,
  sort: <CID of menagerie/concept-shapes/catalog/sorts/Int.json>
})
```

The literal has no operand args (it's a leaf). `value` and `sort` are
JSON properties of the leaf node. The substrate sees a single concrete
value with a single canonical sort. The kit's SortAdmission tag on
`concept:literal` declares that Rust admits `concept:Int` at this position.

Per-platform realization: Rust kit's `SortMorphismMemento` exam answer maps
substrate-canonical `concept:Int` to Rust-native `i32` / `i64` / etc. The
concept:literal node carries only the substrate-canonical sort; the kit's
exam answer carries the language-specific realization.

## Worked example 2: quantum superposition

Source code (hypothetical quantum DSL):
`let q: Qubit = 0.707|0> + 0.707|1>;`

Lifted term (in term_shape):

```
Term::Op(concept:superposition, [
  Term::Op(concept:amplitude-basis-pair, [
    Term::Op(concept:complex-amplitude, [
      Term::Const { value: 0.707, sort: <Float CID> },
      Term::Const { value: 0.0,   sort: <Float CID> }
    ]),
    Term::Op(concept:basis-state, [
      Term::Const { value: 0, sort: <Int CID> }
    ])
  ]),
  Term::Op(concept:amplitude-basis-pair, [
    Term::Op(concept:complex-amplitude, [
      Term::Const { value: 0.707, sort: <Float CID> },
      Term::Const { value: 0.0,   sort: <Float CID> }
    ]),
    Term::Op(concept:basis-state, [
      Term::Const { value: 1, sort: <Int CID> }
    ])
  ])
], {
  sort: <CID of menagerie/concept-shapes/catalog/sorts/Qubit.json>
})
```

What is happening:

- Classical Float literals (`Term::Const`) at the leaves carry the real
  and imaginary parts of amplitudes and the basis-state indices.
- `concept:complex-amplitude` composes two Floats (real, imag) into a
  complex number.
- `concept:basis-state` marks which basis vector the amplitude attaches
  to.
- `concept:amplitude-basis-pair` pairs an amplitude with its basis state.
- `concept:superposition` sums amplitude-basis pairs into the quantum
  state.
- The whole expression has sort `Qubit`. A 2-qubit register would have
  basis indices 0-3 and sort `QubitRegister<2>` via the existing
  parameterized-sort pattern (sibling to `List<T>`, `Map<K,V>`).

What is NOT happening:

- No new Term variant. The existing `Term::Op` and `Term::Const`
  primitives compose to express the quantum state.
- No new IR primitive. The existing IR (Term + IrFormula) admits the
  composition.
- No extension to `concept:literal` to accommodate qubits. Quantum has its
  own value-tier ops; concept:literal stays a classical construct.
- No special-case dispatcher logic. `compare_op_with` and
  `propagate_effects` operate on the Term composition uniformly.

### Quantum-specific constraints carried by ordinary substrate primitives

- **Normalization** (|α|^2 + |β|^2 = 1) is a contract expressible as an
  IrFormula predicate over the superposition term. Existing contract
  machinery handles it.
- **Measurement** (`Term::Op(concept:measure, [<qubit-term>])`) returns a
  classical bit and has effect `MeasurementCollapse`. The effect
  propagates via existing trichotomy primitives.
- **No-cloning** (cannot duplicate a qubit) is a contract on
  `concept:assign` or related ops when the operand has Qubit sort. The
  substrate's existing forbidden-effect machinery enforces it.

## Worked example 3: hypothetical cellular automaton cell

Source code (hypothetical cellular DSL):
`cell @ (3, 4) = ALIVE`

Lifted term:

```
Term::Op(concept:cell-assignment, [
  Term::Op(concept:cell-position, [
    Term::Const { value: 3, sort: <Int CID> },
    Term::Const { value: 4, sort: <Int CID> }
  ]),
  Term::Op(concept:cell-state, [
    Term::Const { value: 1, sort: <Int CID> }
  ])
], {
  sort: <CID of menagerie/concept-shapes/catalog/sorts/Cell.json>
})
```

Classical Int literals at leaves; cellular concepts compose them. No
special machinery.

## Implication: SortAdmission applies per value-tier op, not globally

Each kit's SortAdmission dimension is declared PER value-tier op the kit
recognizes:

- Classical kit declares SortAdmission on `concept:literal`.
- Quantum kit declares SortAdmission on `concept:complex-amplitude`
  (admits Float at amplitude positions), `concept:basis-state` (admits
  Int at basis-index positions), `concept:superposition` (no scalar
  position; the position-admission concern is structural).
- Cellular kit declares SortAdmission on `concept:cell-state` (admits
  Int from the rule's state alphabet), `concept:cell-position` (admits
  Int at coordinate positions).

The same dimension (SortAdmission) appears in many tags across many
value-tier ops. The substrate composes them via existing
`platform_semantics_for_binding` machinery.

## Cross-family migration: classical-to-quantum and back

Migrate classical -> quantum: a classical Int literal at a position the
target quantum kit expects a `concept:basis-state` requires lifting the
Int through `concept:basis-state(Int)`. The substrate characterizes the
divergence: target's value-tier op at this position is
`concept:basis-state`; source has `concept:literal`. Different ops;
`compare_op_with` returns Uncharacterizable. Migration EITHER refuses (the
substrate doesn't know how to wrap the Int as a basis state without
additional information) OR a per-(source-op, target-op) admission
declaration is added to the kit (e.g., "classical-int-at-this-position
lifts as basis-state-index").

Migrate quantum -> classical: a `concept:superposition` term cannot
collapse to a classical literal without measurement. The substrate's
`compare_op_with` returns Uncharacterizable; the trichotomy routes to
refuse. The substrate is honest: superposition is not classical-literal-
equivalent under any cycle. Lossy migrations explicitly carry a
`concept:measure` op that produces a classical bit; the receipt records
the measurement-collapse loss.

## Algebraic discipline is per-operation, not per-domain

Earlier framings of "the substrate is algebraic; quantum/cellular/stochastic
is not" overstate the boundary. The accurate framing is finer-grained:
the substrate's trichotomy fires **per operation** based on whether that
specific operation admits algebraic characterization. Quantum unitary
operations admit characterization (linear algebra over Hilbert space;
dagger-compact categories formalize this); quantum measurement does not.
Synchronous deterministic cellular evolution admits; asynchronous
nondeterministic cellular evolution does not. Exact arithmetic admits;
floating-point order-dependent reduction does not.

The substrate's algebra is its primary mode. The trichotomy is the
substrate's response to operations that fall outside algebraic
characterization at any point. Both hold simultaneously: the substrate is
algebraic where the operations admit it, refuses where they do not, all
within the same machinery (compare_op_with's NoOpinion /
Uncharacterizable / Same / Divergent verdicts, propagate_effects' Widen /
Halt / Refuse decisions).

### Worked example: cellular automata

Cellular automata are the cleanest exemplar because they show the
per-operation discipline at three layers within a single domain:

**Cell state algebra (admits).** Each cell's state-transition rule is a
deterministic function from neighborhood configuration to new state. The
rule itself IS algebraic. The substrate's `concept:cell-rule` mints
naturally; `compare_op_with` characterizes rule-vs-rule divergence; the
substrate makes algebraic claims about rule composition.

**Sort ordering over cells (admits, conditionally).** If the CA is
synchronous deterministic (Conway's Life: every cell evaluates its rule
against the same snapshot; all states update simultaneously, then advance
the time-step), the substrate characterizes the ordering canonically. The
algebra of time-evolution is well-defined. The substrate makes algebraic
claims about temporal composition.

**Sort ordering over cells (refuses, conditionally).** If the CA is
asynchronous nondeterministic (each cell fires at an arbitrary moment;
which-cell-first affects the result), the ordering is undetermined.
`compare_op_with` returns Uncharacterizable for ordering-claims on this
class of CA; the trichotomy routes to refuse-leg for any migration that
depends on canonical evaluation order. The substrate STILL admits the
per-cell rule claim; it REFUSES the ordering claim.

A single CA kit's declaration is therefore: algebraic at the rule level,
algebraic at the ordering level IF synchronous, refusing at the ordering
level IF asynchronous. Same machinery; per-op verdict.

### The same per-op discipline covers all the previously-listed domains

- **Quantum operations** decompose into unitary (algebraic; admits
  dagger-compact categorical claims) and measurement (refuses; collapse
  is irreversible). A quantum kit's declaration admits unitary
  compositions, refuses measurement-coupled migrations.
- **Stochastic computation** decomposes into the structural shape of the
  probabilistic operation (admits; the operation's space is well-defined)
  and the particular sample (refuses; the realized value is not algebraic
  in the substrate's sense). A stochastic kit admits operation-shape
  claims, refuses sample-value claims.
- **Approximate computation** decomposes into exact-algorithm structure
  (admits) and finite-precision realization (refuses for
  associativity-dependent claims). A floating-point kit might admit
  individual op characterizations and refuse claims about
  reduction-order-dependent compositions.
- **Dataflow with shared state** decomposes into the dataflow graph
  (admits algebraic structural claims) and the runtime interleaving
  (refuses; depends on scheduling).

For each domain, the substrate distinguishes per-operation: admit where
the operation grounds in algebraic structure, refuse where it does not.
The substrate's representational openness is universal; the algebraic
claims are precise about where they apply.

### Discipline for kit authors at mixed-algebraic boundaries

When filing a kit for a domain with mixed algebraic / non-algebraic
operations:

1. Mint your value-tier ops and sorts as usual.
2. Declare the operations your kit recognizes via SortAdmission and
   domain-specific dimensions.
3. **For each operation, declare whether it admits algebraic
   characterization or refuses.** This is the load-bearing kit-level
   discipline: each op's `PlatformSemanticTag` either populates
   dimensions enabling Same/Divergent verdicts (algebraic admission) or
   declares a forbidden-effect that routes to refuse-leg (algebraic
   refusal). Example: a quantum kit's `concept:measure` op carries a
   forbidden-effect `MeasurementCollapse` that `propagate_effects` reads
   to route migrations through refuse.
4. Document the EXTERNAL proof systems your users must rely on for
   domain-specific correctness claims that fall outside substrate algebra
   (physics, quantum information theory, dagger-compact categories,
   numerical-analysis frameworks, concurrency theorems, etc.). The
   substrate does not pretend to subsume them; it carries representations
   and makes algebraic claims only where its discipline applies.
5. Do NOT extend the substrate's algebraic primitives to "support"
   non-algebraic operations. The substrate's algebra is its load-bearing
   property; weakening it to admit non-algebraic claims breaks the entire
   cycle invariance machinery.

The substrate's representational openness is universal; its algebraic
discipline is per-operation precise. Both hold simultaneously by being
separate concerns.

### The two-axis statement (rectum-correct)

- **Open for representation, universally.** Any architecture mints its
  value-tier op family, registers its sorts, declares its dimensions.
  Classical, quantum, cellular, dataflow, neuromorphic, combinator,
  future-unimagined.
- **Algebraic for correctness, per-operation.** The substrate's trichotomy
  fires per-operation. Operations that admit algebraic characterization
  get Same/Divergent verdicts and participate in cycle invariance.
  Operations that do not admit algebraic characterization get NoOpinion /
  Uncharacterizable verdicts and route to refuse-leg.

Both axes hold simultaneously by being separate. Supra omnia, rectum.

## Discipline

When filing a new kit for a new platform:

1. Mint your value-tier ops in `menagerie/concept-shapes/catalog/algorithms/`.
   Name them per your platform's natural vocabulary (concept:superposition,
   concept:cell-state, concept:stack-push, etc.).
2. Mint any new sorts you need in
   `menagerie/concept-shapes/catalog/sorts/` (Qubit, QuantumRegister<N>,
   Cell, StackFrame, etc.). Cite sibling pattern at
   `Int.<cid>.json`.
3. Declare SortAdmission (and any platform-specific dimensions like
   MeasurementCollapse, CellNeighborhood, etc.) per value-tier op in your
   kit's `PlatformSemanticsDeclaration`. Use kit-minted open-keyed
   dimension names per
   `docs/plans/2026-05-18-dimension-naming-conventions.md`.
4. Answer the per-language exam questions
   (`sort-classification` for each substrate-canonical sort your
   architecture admits; new `literal-encoding`-style answers if needed for
   your value-tier ops).
5. If your platform's control flow is genuinely non-call-graph (dataflow,
   cellular, neuromorphic, combinator), file a ruling defining a
   domain-specific propagation primitive. The substrate's protocol admits
   multiple propagation primitives as a planned capability (per rules doc
   section 10.1, second bullet).

## What this principle prevents

- DO NOT extend `concept:literal` to carry quantum state. Quantum has its
  own value-tier ops; conflation breaks the cycle-invariance claim.
- DO NOT add architecture-specific fields to Term variants. The existing
  Term IR is universal; new architectures compose via new concepts, not
  via Term modifications.
- DO NOT propose a "value envelope" struct that wraps different value
  types. Term composition IS the envelope; new concepts ARE the wrapping.
- DO NOT assume classical-platform concepts (concept:literal,
  concept:assign, concept:conditional, etc.) are universal. They are the
  classical baseline. Future platforms may declare their own analogs or
  decline to admit them entirely.

## Cross-references

- Rules of engagement: `docs/explanation/substrate-uniform-pattern.md`,
  particularly section 10 (open-extensibility) and section 2 (two layers,
  one machine).
- Naming conventions: `docs/plans/2026-05-18-dimension-naming-conventions.md`.
- Trichotomy ruling: `docs/plans/2026-05-18-op-coverage-verdict-trichotomy-ruling.md`.
- Refuse-leg ruling: `docs/plans/2026-05-18-refuse-leg-short-circuit-ruling.md`.
- Cycle invariance theorem: project memory `provekit_cycle_invariance`.
- Sort catalog: `menagerie/concept-shapes/catalog/sorts/`.
- Concept algorithm catalog: `menagerie/concept-shapes/catalog/algorithms/`.
- Term IR: `implementations/rust/provekit-ir-types/src/lib.rs`.
- Composition primitive: `implementations/rust/libprovekit/src/core/platform_semantics.rs:124`.
- Comparison primitive: `implementations/rust/libprovekit/src/core/types.rs:874`.
- Effect propagation: `implementations/rust/libprovekit/src/effect_propagation.rs:111`.
