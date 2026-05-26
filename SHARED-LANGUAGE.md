# ProvekIt — Shared Language

> The dictionary. T defines each term, plainly, once. Claude records exactly what
> was said — no reverse-engineered models, no additions, no running ahead.

## Lift

Lift has **two parts**:
1. It lifts **contracts** into **ProofIR**.
2. It lifts **sugar** into **body_text**.

## Boundary

A boundary is a **realization site of a sugar**. It's a **client of a library**.
- The **library author supplies the sugar**.
- The **user of the library has boundaries** at which the sugar is materialized.

## Concept

A concept is a **name for a shared tag amongst sugars**.
Example: `json_encode`. Or `json_decode`.

## Contract

A contract is a **ProofIR first-order logic**. It's **tied to a specific sugar**.
- When you materialize the sugar, the **contract propagates with the sugar** to the
  materialize site **at the boundary**.
- So: **library authors declare "sugar x gets contract y."**
- Users get **red squigglies when they violate a contract at compile time.**

## Kit

A kit is a **language-specific implementation of these ideas**. The **Java kit**:
- lifts **sugar bodies**
- lifts **contracts from JUnit tests**
- emits **witnesses as JUnit tests from contracts**
- writes **sugars into boundary bodies**
- handles **side-effect propagation** like import statements

## Concept equivalence — vendor's call

Whether several sugars sharing a concept mean the *same* FOL is **entirely the vendor's
decision. ProvekIt sets nothing.** (Mechanism: three-axis pinning — below.)

## Emitter

Turns ProofIR (a contract) into a concrete native artifact — a JUnit test, a Spring
annotation, a gate that throws, etc. A kit hosts **many** emitters; **which to emit is a
kit-time decision and you can invoke many** (one contract → stub + test + annotation +
gate, all at once). Inverse of the lift-from-native-test path: the emitter writes the
contract OUT as a test/annotation; lift reads it back IN.

## The lift/emit asymmetry (load-bearing)

- **Emit is plural** (a relation): one contract → N faithful native forms, simultaneously.
  Safe, because every form is a *projection* of the one truth; none can contradict it.
- **Lift is singular** (a function): one surface → exactly one contract. Forced by
  content-addressing — if a surface could lift two ways, the ingested truth is ambiguous,
  the CID is unstable, pinning is meaningless, federation collapses. Two parties lifting
  the same surface MUST get the same contract.
- Plainly: **truth has one source but many expressions.** Lifting establishes truth (must
  be a function); emitting expresses truth (may be a relation).
- Refinement: a kit may have *many lifters* (one per distinct surface — tests, proptest,
  bean-validation, JML, …); the rule is per-surface determinism (one surface → one
  contract). Multiple surfaces on the same code each lift once and **compose/conjoin**.
  Forbidden thing = one surface → two contracts. (This is why `.invariant` died: a second
  lifting path competing with the canonical one.)

## Solvers — DERIVED FROM T ("verifies those contracts against solvers"), confirm

The discharge engine for contracts. A contract is ProofIR FOL; the solver composes,
refutes, or accepts it. The clean discharge receipt is what the **witness axis** pins.
(Z3/CVC5/Vampire/CeTA/Lean/Maude/Coq — `provekit-verifier/src/solvers/`.)

## LSP — DERIVED FROM T ("red squigglies when they violate a contract at compile time"), confirm

The per-language editor face that DELIVERS the contract to the user. It sees the boundaries
(sugar usages) in the user's code, knows the contract each sugar carries, and surfaces
violations live as diagnostics. (Per-kit `provekit-lsp-<lang>`.)
- OPEN: does the LSP drive the solver live at the boundary, or is the squiggle a cheaper
  structural check with full solver discharge at verify/CI time? (T to settle.)

## Three axes of pinning (paper 03 §8; docs/security/what-binaryCid-does-not-catch.md)

A proof bundle binds **three independent CIDs**, each pinnable (frozen) or floatable
(track latest acceptable at verify time) → 2³ = **8 trust postures**:
1. **Contract** — what it conforms to. (Pinned: contract identity stable across re-attestation.)
2. **Witness** — the chain that endorses it. (Pinned: the contract is what it claims to be.)
3. **Binary** — what it asserts about. (Pinned: the binary you run is the one the witness was minted against.)

The substrate **picks none of it** — the consumer/vendor decides per axis (security team:
tight witness; dev team: tight binary; compliance: all three). This is *why* concept
equivalence / trust is the vendor's call. Together the three axes close the supply-chain
attack class (authenticated betrayal): a correctly-signed package still can't swap
behavior, forge endorsement, or swap bytes without breaking a pin.

## Loss record — IN SCOPE (100%, T)

Produced **per emitter / per materializer / per solver**. The honest accounting of
lossiness: when emitting a contract, materializing a sugar, or discharging against a
solver cannot be **exact**, the loss is **recorded** — content-addressed and named, not
silent. This is what makes the trichotomy honest — *exact / loudly-bounded-lossy /
refuse* — because "loudly-bounded-lossy" is only honest if the bound is written down.
Silent loss would be a lie; the loss record is the substrate keeping correctness-above-all.
(Lift is not in this list — lift is the singular truth-ingestion; it refuses or
sugar-carries rather than loses.)

## Catalog (concept/realization) — FICTION

There is no central concept/realization catalog. A concept is a shared tag nobody owns;
everything a catalog would hold already lives, content-addressed + signed, in vendor
`.proof`s (paper 23). "The catalog" is at most the **ephemeral union of the vendor .proofs
a consumer has resolved** (`union(resolve(dependencies).proof)`) — a query/view at resolve
time, per-consumer, never a stored global object. A central catalog is the registry
anti-pattern this content-addressed, vendor-pinned substrate exists to abolish.
- INFECTION (delete): the central `menagerie/concept-shapes/catalog/realizations/` store
  the realizer walks up to find. Replace with: resolve the dependency .proofs, union them.

## Protocol record (currently MISNAMED "protocol catalog") — REAL

A living, signed, content-addressed artifact: the record of the **protocol's own
evolution** (lift-plugin protocol, PEP transitions, IR version, extension surface). NOT a
registry of vendor content — the substrate's own ledger of how its rules changed.
"Catalog" mis-frames a living *record* as a static *index*. **Rename: protocol record.**
- Re-signing it on a protocol change (e.g. tonight's `agent-plugin-protocol` de-list) is
  legitimate record-keeping, NOT feeding the fiction. Keep the artifact; fix the name.

## CLI

The CLI is the **orchestration**.
- It talks **RPC to the kits**, and handles **all computation over the data**.
- The kits do things like **resolve the `.proof` file in jar files or pip packages** and
  feed the **rust CLI** through a **common RPC layer**.
- **All kits speak one RPC language.**

_(awaiting next term)_
