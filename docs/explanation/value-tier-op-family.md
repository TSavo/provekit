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

## The substrate is algebraic; non-algebraic domains get description without algebraic claims

The substrate is fundamentally an algebraic system. The Heyting category
(paper 07), the structural-elimination theorem (paper 16), the substrate
trinity {terms, contracts, implications}, the cycle invariance theorem
(paper 24), the compose primitive's correctness — every load-bearing claim
grounds in algebraic structure. `compare_op_with` and `propagate_effects`
are algebraic mechanisms. Cycle invariance asserts that compositions of
loss-functions compose to identity post-formatter; that assertion REQUIRES
the operations being composed to admit algebraic inversion.

Quantum computation HAS NO ALGEBRA in the substrate's sense:

- Quantum operations are unitary transformations on Hilbert space, not
  algebraic operations on substrate terms.
- States do not compose algebraically — superposition is linear
  combination over complex amplitudes; entanglement creates joint states
  that do not factor through the substrate's product operation.
- Measurement is irreversible: collapse breaks the algebraic round-trip
  property cycle invariance underwrites.
- The no-cloning theorem forbids the duplication that classical term
  composition assumes.

The substrate's correctness move with quantum is **honest refusal, not
false algebraic claim**. Specifically:

- The substrate ADMITS quantum representations (via the value-tier op
  family: `concept:superposition`, `concept:basis-state`, etc., minted by
  the quantum kit). This is descriptive capability.
- The substrate's algebraic primitives (compare_op_with's "Same" verdict;
  cycle invariance theorem; loss-record composition) DO NOT apply to
  quantum operations. The trichotomy correctly returns NoOpinion or
  Uncharacterizable; the refuse-leg ruling routes the receipt to refuse.
- The substrate provides ZERO algebraic correctness proofs about quantum
  operations. Users relying on quantum kits must obtain correctness from
  external proof systems (physics, quantum information theory,
  category-theoretic models like dagger-compact categories that DO admit
  quantum). The substrate just carries the structure honestly.

This is "Supra omnia, rectum" at its sharpest: the substrate never claims
more than it can prove. Quantum cannot be proved algebraically inside the
substrate's machinery; therefore the substrate makes no algebraic claims
about quantum. It just describes the structure.

### The same principle applies to other non-algebraic domains

- **Stochastic computation.** Probabilistic operations on a sample space
  do not admit algebraic inversion. Cycle invariance claims do not hold
  across stochastic transitions; the substrate describes the operations
  via concept mints and refuses algebraic claims about their composition.
- **Cellular automata with non-reversible rules.** Conway's Life is
  non-reversible (forward evolution is unique; backward is not). The
  substrate's value-tier op family handles cell-state representation;
  algebraic round-trip claims through non-reversible cellular rules are
  refused.
- **Approximate computation (neural networks, fuzzy logic, analog
  computation).** Floating-point arithmetic itself is non-associative on
  finite-precision machines; neural networks compose non-linear
  activations that admit no algebraic inverse. The substrate carries the
  topology of the computation honestly; correctness claims about the
  approximation belong to external numerical-analysis frameworks.
- **Dataflow with shared mutable state.** Race conditions, lock-free
  algorithms, and CRDT-style eventual consistency operate outside
  classical algebraic discipline. The substrate carries the operations'
  structure; correctness claims about their interleaving belong to
  external concurrency-theoretic frameworks (linearizability proofs,
  TLA+, etc.).

For each of these domains, the substrate's role is: ADMIT the
representation (via value-tier op family + sort catalog growth + dimension
declarations), REFUSE the algebraic claims (via trichotomy's NoOpinion /
Uncharacterizable / Refuse verdicts), and route to external proof systems
for the domain-specific correctness questions.

### Discipline for kit authors at non-algebraic boundaries

When filing a kit for a non-algebraic domain:

1. Mint your value-tier ops and sorts as usual (per `Discipline` section
   below).
2. Declare the operations your kit recognizes via SortAdmission and
   domain-specific dimensions.
3. Where appropriate, declare your operations' algebraic LIMITS via
   contract mementos. Example: a `concept:measure` op might carry a
   forbidden-effect declaration `MeasurementCollapse` that the substrate's
   `propagate_effects` reads to route migrations through refuse-leg.
4. Document the EXTERNAL proof systems your users must rely on for
   domain-specific correctness (cite the paper, the framework, the
   theorem). The substrate does not pretend to subsume them; it just
   carries representations honestly.
5. Do NOT extend the substrate's algebraic primitives to "support"
   non-algebraic domains. The substrate's algebra is its load-bearing
   property; weakening it to admit non-algebraic claims breaks the entire
   cycle invariance machinery.

The substrate's open-extensibility is for REPRESENTATION; the substrate's
algebraic discipline is for CORRECTNESS. Both hold simultaneously by
keeping them separate.

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
