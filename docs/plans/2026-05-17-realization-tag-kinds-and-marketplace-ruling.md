# Realization Tag-Kinds and Marketplace Ruling

**Date:** 2026-05-17
**Status:** Architectural ruling. Locks R1-R13. Ratifies existing specs and identifies the migration surface.
**Authority:** T Savo (architect).
**Origin:** Conversational ruling emerging from the exam manifest arc (#1103-#1108). The realization-question cross-product bug in the v1 manifest exposed a structural conflation between concept identity and per-language emission. This document captures the corrected structural model.

---

## TL;DR

Concepts at ProofIR are universal. The catalog stays unified. What differs per language is not the concept's identity but the language's choice of EMISSION TAG LOCATION. Every language's kit owns four tag locations: first-class, composition, boundary, and sugar-carrier. Sugar dicts, witness adapters, and policy profiles are vendor-authored content-addressed plugins on top of the universal substrate. The substrate ships the protocols; the marketplace ships the per-vendor coverage.

Federation operates at three layers (concept-tier composition, first-class morphism, boundary contract) with distinct trichotomy semantics at each. Round-trip preservation across languages that lack native realization is guaranteed by concept-citation-comment-sugar, which carries concept identity through code that has no syntactic form for the concept.

The exam manifest's role narrows. It asks structural questions about substrate coverage. Vendor specifics live in vendor-shipped plugins.

---

## §1. The conflation that prompted this ruling

The v1 exam manifest's realization-question category emitted a naive cross-product of `(target_language, target_library)`. The result: 110 realization questions, of which roughly 99 are nonsensical (e.g., "how does C11 render concept:http-request via python-requests?").

Underneath the cross-product noise was a deeper conflation. The substrate had been treating `concept:add` (which every language realizes via native syntax) and `concept:http-request` (which every language realizes via library boundary) as structurally equivalent catalog entries. They are not. But neither are they structurally different in the way I initially proposed (splitting the catalog into "concepts" and "boundaries").

The correct structural model:

- Concepts at ProofIR are universal. concept:add is one entity. concept:list is one entity. concept:http-request is one entity. concept:free is one entity. They have algebra. They do not depend on language.

- What differs per language is HOW the language emits the concept. Each language's kit declares per concept where the tag for that concept lives in the language: a native syntactic form, a composition of language primitives, a library API binding to a boundary contract, or a sugar-carrier comment for concepts with no native realization.

The exam manifest's job is to enumerate the substrate's coverage of concepts × languages, with the tag-kind embedded in the realization answer rather than fanning out into separate question categories.

---

## §2. The ruling: R1 through R13

### R1: Concepts at ProofIR are universal.

The concept catalog is unified. No split into "concept vs boundary." A boundary is a SHAPE OF REALIZATION (tag kind) that a language chooses for a given concept; it is not a separate catalog tier.

concept:add stays in the catalog as concept:add. concept:http-request stays in the catalog as concept:http-request. Both are concepts. Both have algebra. They differ in how languages tag them, not in what they are.

### R2: Every kit owns four tag locations per concept.

For each (concept, language) pair, the language's kit declares ZERO-OR-MORE realization tags, each of one of four kinds:

- `tag_first_class(concept, syntactic_pattern)`: language has direct syntax for this concept. Example: Rust kit tags concept:add with `+` operator; Rust kit tags concept:drop with `drop(x)` call.

- `tag_composition(concept, composition_tree)`: language builds the concept from concept-tier primitives. The tree is content-addressed in ProofIR. Example: C kit tags concept:list with a composition of concept:pointer + concept:malloc + concept:array.

- `tag_boundary(concept, library, api, boundary_contract)`: language realizes the concept via a specific library API. The library implements a boundary contract. Example: Python kit tags concept:http-request with library `python-requests`, api `requests.get`, boundary contract `boundary:http-1.1`.

- `tag_sugar_carrier(concept)`: implicit. When the language has no first-class, composition, or boundary realization for the concept, the lower emits a concept-citation comment carrier per `2026-05-15-concept-citation-comment-sugar.md`. This is the round-trip preservation mechanism. Example: Java kit has no realization for concept:free (Java has GC; no manual memory release). When lowering ProofIR containing concept:free to Java, the lower emits `// provekit:concept:free({"$ptr":"x"})`.

ZERO mementos for a (concept, language) pair = explicit refusal (the language declines to realize the concept at all). Federation excludes this concept in this language.

MULTIPLE mementos = the language has multiple valid realizations. Lower may pick among them based on user preference, policy, or default. Example: Python kit could tag concept:http-request with BOTH `python-requests` (library) AND `python-urllib` (library). Both are valid; user/lower picks.

### R3: Concept-citation-comment-sugar is the round-trip preservation mechanism.

Spec at `protocol/specs/2026-05-15-concept-citation-comment-sugar.md` is already in main. R2 ratifies it as the fourth tag location. Every kit must own this emission path; without it, concepts that lack native realization would be lost on lower and unrecoverable on re-lift.

The carrier format: `// provekit:concept: {JCS-JSON payload}` followed optionally by `// provekit:concept-payload-cid: {cid}`. The payload contains concept_cid, concept_name, args_jcs, term_position, sugar_dict_cid, kit metadata. The lifter recovers concept identity from the carrier; round trip closes byte-identically against the carrier's CID.

### R4: Sugar dicts are vendor-authored content-addressed plugins.

Spec at `protocol/specs/2026-05-12-sugar-dict-memento.md` is already in main. The plugin kind is `"sugar"` per PEP 1.7.0 §2.1. R4 ratifies the sugar dict as the marketplace mechanism for per-(language, idiom) contract surfaces.

The marketplace dynamic: library authors ship sugar dict plugins for their library's idioms. Spring ships `spring-provekit-contract-sugars-emitter.jar` mapping `@Valid` to concept:non-null, `@RequestMapping` to boundary:http-handler, `@Transactional` to boundary:database-transaction-scope. JUnit ships `junit-provekit-sugars.jar` mapping `assertEquals` to concept:gate-equality. Pydantic ships `pydantic-provekit-sugars.whl`. Each vendor authors their library's sugar dict and ships it as a plugin.

The substrate does not author Spring's sugar dict. Spring does. The substrate respects loaded sugar dicts via the PEP 1.7.0 plugin protocol.

### R5: Observations come in three shapes: Monitor, Witness, Gate.

These have distinct signing and runtime semantics:

- **Monitor**: a passive observer that records what was checked. A running JUnit test is a monitor. An instrumented assertion log is a monitor. Output: monitor records. Unsigned.

- **Witness**: a signed memento attesting that an observation occurred and what was observed. A signed JUnit run result is a witness. A signed formal proof is a witness. A signed regulator attestation is a witness. Each witness has a signer; the substrate's witness registry indexes by (subject, fixture_state_cid). Signed.

- **Gate**: an active runtime enforcement. `assert(x > 0)` is a gate. A precondition check that throws is a gate. A boundary refusal at PEP plugin loading is a gate. Output: control flow (continue or throw). Active.

The substrate models all three. Different memento families:
- MonitorMemento (or instrumentation record, depending on monitor type)
- WitnessMemento (already in `provekit-ir-types`)
- GateMemento (for gate declarations; the runtime enforcement is the gate's behavior)

JUnit-as-monitor: the running test observes. JUnit-as-witness: the signed test result attests. JUnit-as-gate: a JUnit assertion that throws (terminates the test) is enforcing.

### R6: Promotion decisions are policy-mediated per consumer.

PolicyProfileMemento per `protocol/specs/2026-05-14-policy-profile-memento.md` is already in main. R6 ratifies it as the per-consumer acceptance mechanism.

Different organizations have different policy profiles:
- Org A: accepts witness from any signer at 1-witness threshold
- Org B: requires 3 independent signers + 1 formal proof witness
- Org C: requires regulatory attestation

PromotionDecisionMemento (per `provekit-ir-types` + `promotion_decision_registry.rs`) records WHICH policy was applied at promotion time. Two organizations may promote different concepts based on their local policy. Cross-org federation requires policy compatibility or explicit translation.

Witness consensus per `protocol/specs/2026-05-14-witness-consensus-promotion.md` provides the consensus_vector machinery; PolicyProfileMemento provides the threshold rules. The substrate combines them at promotion time.

### R7: Migration is multi-axis.

When primitives evolve, the substrate emits a PropagationPlan per `libprovekit/src/effect_propagation.rs`. Each migration axis has its own propagation algorithm:

- **Sort migration** (i32 → i64): per-operation LossRecord. Widening is exact; narrowing is loudly-bounded-lossy at the range boundary. Existing witnesses against the narrower sort remain valid for the wider sort (sign-extension is exact). The substrate adds a sort-migration witness chained to the original.

- **Effect migration** (sync → async): effect propagation walks the call graph. Functions admitting the new effect HALT. Functions not admitting widen via WIDEN. Functions explicitly forbidding via contract REFUSE. User reviews the plan, consents at each refusal.

- **Boundary contract migration** (sqlite-dialect → postgres-dialect): per-callsite LossRecord at the boundary contract layer. SQL syntax differences, type coercion differences, transaction isolation differences. Each callsite's contract reauthored against the new boundary contract; LossRecord characterizes the divergence.

- **Library migration** (requests → reqwest): per-(concept, language) realization tag changes. New tag points at the new library. Existing code lifts identically (concept-tier is unchanged); new lower emits via the new library.

Each migration axis emits its own PropagationPlan. User reviews all of them before commit.

### R8: Federation operates at three layers.

- **Concept-tier composition CID equality**: programs federate when their concept-tier compositions hash identically. This is the universal federation surface.

- **First-class realization morphism CID equality**: when two languages tag a concept as first-class, federation works because the morphism graph captures both languages' native forms pointing at the same concept CID.

- **Boundary contract CID equality**: when two languages tag a concept as boundary, federation works because both libraries implement the same boundary contract. The library is per-language; the contract is universal.

These are three federation surfaces with distinct trichotomy semantics:
- Concept-tier: exact (identical composition) or refuse (different compositions are different programs)
- First-class: exact (matching morphism CIDs) or loudly-bounded-lossy (per-language semantic divergence captured in LossRecord) or refuse
- Boundary: exact (matching boundary contract CIDs) or loudly-bounded-lossy (contract version divergence) or refuse

The trichotomy operates AT EACH layer independently.

### R9: Vendors participate as first-class authors.

The substrate is a federated marketplace. Each authoring surface has its own vendor:

- Substrate authors: concept catalog, universal protocols (PEP 1.7.0, witness consensus, effect propagation, policy profile format), reference kits per language
- Language kit authors: per-language realization tags (substrate ships reference; third parties can ship better)
- Library vendors: sugar dicts for their library's idioms (Spring, JUnit, Pydantic, serde, etc.)
- Test framework authors: witness adapters (junit-witness-adapter, pytest-witness-adapter, etc.)
- Regulators: policy profiles for compliance attestations (SOC2, HIPAA, etc.)
- Organizations: PolicyProfileMementos selecting which policies their consumers run

Every plugin is content-addressed, signed, PEP-1.7.0-loadable. Federation works because everyone references the same concepts and protocols.

### R10: IDE integration is downstream.

Contracts surface as LSP feedback. The substrate's contract catalog is the IDE's contract type system. The Node.js parseInt contract authored in Node's library travels through the substrate's catalog and surfaces in TypeScript as a red squiggle in the IDE when the contract is violated.

This is the user-facing pitch: real-time multi-language contract enforcement via content-addressed catalogs. The substrate is not just a verification platform; it is a development environment.

LSP integration is downstream of R1-R9. It does not change the substrate's architecture; it consumes the contract catalog.

### R11: The exam manifest's role narrows.

The manifest asks STRUCTURAL questions about substrate coverage:

- Per (concept, language): "Is there a realization tag? Which kind (first-class | composition | boundary | sugar-carrier)? What's the realization data?"
- Per (boundary contract): "What libraries in this language realize this boundary?"
- Per (composition pattern): "What's the canonical algebra at the concept tier?"

The manifest does NOT enumerate:

- Per (library, language, api): sugar dicts (vendors ship their own)
- Per (policy, witness): acceptance rules (orgs ship their own)
- Per (contract, IDE): feedback rendering (LSP integration is downstream)

The manifest is bounded. Vendor work is bounded. The substrate's marketplace handles the rest.

The exam manifest schema gets ONE question per (concept, language) pair, with the tag-kind embedded in the answer. The cross-product noise from the v1 manifest disappears.

### R12: Per-(concept, language) realization metadata gains a tag-kind enum.

The RealizationMemento becomes a tagged enum:

```rust
pub enum RealizationMemento {
    FirstClass(FirstClassRealization),       // syntactic pattern
    Composition(CompositionRealization),     // composition tree
    Boundary(BoundaryRealization),           // library + api + contract
    SugarCarrier(SugarCarrierRealization),   // implicit; no native form
}
```

Each variant has its own data. Federation handlers know how to compare each variant (per R8).

### R13: Concept API gains tagging primitives.

Each language's Concept API SDK exposes four tagging primitives:

```
tag_first_class(concept_op, syntactic_pattern)
tag_composition(concept_op, composition_tree)
tag_boundary(concept_op, library, api, boundary_contract)
tag_sugar_carrier(concept_op)  # implicit; auto-applied when no other tag
```

The kit author chooses per concept which primitive to use. The kit's exam answer is a collection of tags. The substrate's exam administrator collects them and emits per-(concept, language) realization mementos.

---

## §3. Walking through the load-bearing cases

### §3.1 Rust drop → C free → Java emission/lift

Rust source: `drop(x)`.

Rust kit tags: `tag_first_class(concept:free, "drop(${$ptr})")`.

Lifter recognizes `drop(x)` and emits concept:free node with $ptr = x.

Lower to C: C kit tags `tag_first_class(concept:free, "free(${$ptr})")`. Emission: `free(x);`.

Lower to Java: Java kit has NO tag for concept:free. The fourth tag location auto-applies. Emission:

```java
// provekit:concept: {"args_jcs":[{"kind":"var","name":"x"}],"concept_cid":"blake3-512:...","concept_name":"concept:free","term_position":[0,0]}
// provekit:concept-payload-cid: blake3-512:...
```

This is the sugar-carrier emission. The carrier preserves concept identity.

Re-lift Java back to ProofIR: Java's lifter reads the carrier. Recovers concept:free($x). Round trip byte-identical at the carrier's CID.

Re-lower to Rust: emit `drop(x)`. Re-lower to C: emit `free(x);`. Round trip closes regardless of intermediate hop, because the carrier persists identity across languages with no native form.

### §3.2 i32 → i64 sort migration

Program originally uses i32 throughout. Migration to i64.

Concept catalog: no change. concept:add stays concept:add. concept:i32 and concept:i64 are both in the sort family.

Per-language realization tag metadata changes: each operation that takes i32 inputs and produces i32 outputs gets its tag re-pointed at the i64 sort version (concept:add[i32×i32 → i32] becomes concept:add[i64×i64 → i64]).

Sort migration plan:
- Every operation site using concept:i32: reclassified to concept:i64
- LossRecord per operation: `loss_dimension: integer_widening`, characterization: `exact upward (sign-extension); no information loss`
- Effect propagation walks: most operations WIDEN signatures from i32 to i64; functions explicitly asserting i32-fits-in-output REFUSE.

Witnesses against the i32 version: substrate maintains them. Sign-extension is exact; existing witnesses remain valid for the i64 type. Substrate adds a sort-migration-witness chained to each original.

User reviews the plan. Consents. Migration commits with each LossRecord chained.

### §3.3 SQLite sync TypeScript → Postgres async effect propagation

TypeScript program using `sqlite3` (synchronous Node API).

Lifter recognizes `db.run(sql, params, callback)` and emits concept:sql-query with effect signature `[DatabaseIO]`. Boundary contract: `boundary:sql-query@sqlite-dialect`.

Migration to PostgreSQL with `pg` (async). New realization tag:

```
tag_boundary(
  concept:sql-query,
  library: "pg",
  api: "pool.query(text, params)",
  boundary_contract: "boundary:sql-query@postgres-dialect",
  effects: ["DatabaseIO", "Async"]
)
```

Substrate runs TWO propagation plans:

1. **Effect propagation** (Async widening):
   - Every callsite of concept:sql-query: WIDEN (function becomes async)
   - Every caller of those functions: WIDEN propagates upstream
   - Functions admitting Async: HALT
   - Functions forbidding Async (sync-only contracts): REFUSE

2. **Boundary contract migration** (sqlite-dialect → postgres-dialect):
   - Each SQL string ported with its own LossRecord
   - Loss dimensions: SQL syntax differences (TEXT vs VARCHAR, AUTOINCREMENT vs SERIAL, type coercion, etc.)
   - Some queries port exactly; some need rewriting; some refuse

User reviews both plans. Decides which functions to widen, which to insulate behind sync adapters, which queries to rewrite. Migration commits with explicit consent at each refusal point.

### §3.4 Concept promotion through witness consensus

A new concept candidate emerges. Library author tags it: "my library implements concept:debounced-retry."

Substrate sees the boundary tag. For concept promotion:
- N witnesses needed (signed observations of behavior)
- M independent signers needed
- Consensus vector must align (per `2026-05-14-witness-consensus-promotion.md`)
- Org's policy profile must accept the witness class and threshold

Different consumers have different policies:
- Org A: 1 witness from any signer → promoted
- Org B: 3 witnesses + 1 formal proof → pending until met
- Org C: regulator attestation required → pending until regulator signs

PromotionDecisionMemento records WHICH policy was applied. Each org's catalog has its own promotion history.

Cross-org federation: Org A and Org B may have different concept catalogs. Federation works on the overlap; refuses where they diverge. Explicit policy-translation memos can bridge.

### §3.5 Bridgeworks parse_int → TypeScript LSP red squiggle

Node.js library defines parseInt with a contract: `parse-int(x: string of digits) → integer | NaN`. The contract is authored in Node.js (via JSDoc, native annotation, or vendor-shipped sugar dict).

Substrate lifts the contract from the library's source. Contract is content-addressed at a specific CID.

TypeScript developer imports the Node library and uses `parseInt(123)` (passing integer, not string).

TypeScript kit's lifter recognizes the call site. The contract attaches to the call. Lifter computes argument type (integer) and compares against contract precondition (string). Mismatch.

LSP server receives the contract violation. Emits a red squiggle in the IDE: "parseInt expects a string, got integer".

Developer fixes: `parseInt(String(123))`. Substrate re-evaluates. Contract satisfied. Squiggle disappears.

Contract authored in Node.js. Travels through the substrate's content-addressed catalog. Surfaces in TypeScript as IDE feedback. The substrate is the editor's contract type system.

---

## §4. What this ratifies

The ruling RATIFIES (does not supersede) the following existing specs:

- `protocol/specs/2026-05-12-sugar-dict-memento.md` v1.0.0: sugar dicts as content-addressed plugins. R4 names this as the marketplace mechanism.
- `protocol/specs/2026-05-15-concept-citation-comment-sugar.md`: concept-citation-comment-sugar as the round-trip preservation mechanism. R3 names this as the fourth tag location.
- `protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md`: TransportGapMemento + LossRecord at the operation-semantic boundary. R7 names this as the per-axis migration record.
- `protocol/specs/2026-05-14-witness-consensus-promotion.md` v1.1: consensus_vector machinery. R5 names this as the witness-consensus mechanism (combined with policy profiles).
- `protocol/specs/2026-05-14-policy-profile-memento.md`: PolicyProfileMemento per-consumer. R6 names this as the acceptance mechanism.
- `protocol/specs/2026-05-13-promotion-decision-memento.md`: PromotionDecisionMemento. R6 names this as the policy-mediated promotion record.
- `protocol/specs/2026-05-12-plugin-protocol.md` PEP 1.7.0: universal plugin protocol. R4, R5, R6 ratify the per-kind plugins running on this protocol.
- `protocol/specs/2026-05-16-exam-manifest-memento.md` v1.0.0: ExamManifestMemento. R11 narrows its scope to structural questions; the v1 manifest's schema needs refinement per R12.
- `libprovekit/src/effect_propagation.rs`: effect propagation algorithm. R7 names this as the migration analyzer.

These specs collectively constitute the substrate's first-principle commitment. R1-R13 unify them under a coherent vision: concepts universal, per-language tags decide emission, vendors ship sugar dicts, observations come in three shapes, policy mediates promotion, federation operates at three layers, migration is multi-axis, IDE integration is downstream.

---

## §5. What this changes

The ruling identifies the following migration work:

### §5.1 RealizationMemento schema

Refine `RealizationMemento` in `provekit-ir-types` to a tagged enum with four variants (first-class, composition, boundary, sugar-carrier). Each variant has its own data structure. Backward compat: existing realization mementos in `concept-shapes/catalog/realizations/` get classified per their existing content (most are boundary; some are first-class; rare are composition).

### §5.2 Per-(concept, language) classification metadata

Each entry in `concept-shapes/specs/` gains optional per-language realization-classification metadata. This is the audit data the v1 manifest was implicitly fanning out into question categories. Making it explicit narrows the manifest's schema (R11).

### §5.3 Concept API tagging primitives per language

Each language's Concept API SDK (provekit-ir in Java; libprovekit_py in Python; provekit-ir-types in Rust; sibling crates per language) gains four tagging primitives (R13). Kit authors use them per concept.

### §5.4 Exam manifest schema v1.1

Regenerate the exam manifest with the structurally correct schema:
- One realization question per (concept, language) pair
- Tag-kind embedded in answer, not in question categorization
- Boundary contract questions separated as their own category
- Cross-product noise eliminated

Question count drops significantly. Expected: ~500-700 questions (down from 971) once cross-product noise is removed.

### §5.5 Boundary contract catalog

Boundary contracts become a first-class catalog sibling to the concept catalog. Existing concepts with library/boundary realizations get linked to their boundary contracts. Library tags become `(library, api, boundary_contract, host_language)`.

### §5.6 Re-dispatch #1106 against v1.1

The citation-wiring work paused in #1106's worktree gets re-applied against the v1.1 manifest's question CIDs. The mechanism is sound; only the cited CIDs refresh.

---

## §6. Migration umbrella issue

Per the ruling, the migration sequence is structured as follows. Each item is a sub-issue under the umbrella:

1. **Mint RealizationMemento as tagged enum.** Refine the struct in `provekit-ir-types`. Add backward-compat serde. Tests for round-trip per variant.

2. **Classify existing concept-shapes entries.** Per (concept, language), declare which tag-kind currently applies. This is an audit pass over the existing morphism + realization catalog entries; reclassify per the four-variant taxonomy.

3. **Add Concept API tagging primitives.** Per language SDK. Add the four primitives (tag_first_class, tag_composition, tag_boundary, tag_sugar_carrier). Kit authors will use them in subsequent kit exams.

4. **Mint boundary-contract catalog.** New catalog directory. Seed with existing boundary-flavored concepts that need boundary contracts (concept:http-request → boundary:http-1.1; concept:sql-query → boundary:sql-92 + dialect variants).

5. **Refine exam manifest schema.** Update the shape spec. v1.1 schema.

6. **Regenerate v1.1 exam manifest.** Run the generator with the refined schema. Verify question count drops; verify no cross-product noise.

7. **Re-dispatch #1106 against v1.1.** Cite the v1.1 question CIDs. Merge.

8. **Document the marketplace dynamics.** Operator's guide or paper articulating the vendor-shipped sugar dict story. The Bridgeworks parse_int → TS LSP demo is the load-bearing exhibit.

9. **(Deferred to future)** LSP integration concrete implementation. Substrate exposes contract catalog via LSP extension protocol; IDE renders contract violations as inline feedback.

10. **(Deferred to future)** Witness adapter framework. Per-test-framework (JUnit, pytest, RSpec) adapter that emits WitnessMementos from test runs.

---

## §7. Open questions

The ruling does NOT lock the following. They remain open for future architectural work:

- The boundary contract memento family's exact CDDL shape (Spec to be authored as a separate document)
- The cross-org policy-profile translation mechanism (when Org A's policy and Org B's policy diverge, what's the federation protocol?)
- The LSP extension protocol for contract feedback (downstream; out of scope for this ruling)
- The witness-adapter plugin specification (PEP 1.7.0 plugin kind: `"witness-adapter"`; details TBD)
- The bridgeworks demo arc (a future demonstration: contract authored in language X surfaces as IDE feedback in language Y; the user-facing pitch)

---

## §8. Closing

The substrate's first principle (Supra omnia, rectum: above all, correctness) requires honest stratification. The v1 exam manifest's cross-product noise was a symptom of conflated stratification. The corrected model:

- Concepts at ProofIR are universal.
- Per-language emission has four tag locations.
- Sugar dicts are vendor-authored.
- Observations come in three shapes.
- Promotion is policy-mediated.
- Federation operates at three layers.
- Migration is multi-axis.
- IDE integration is downstream.

Every authoring surface is content-addressed. Every plugin is signed. Every promotion decision records its policy. Every migration emits a reviewable plan. The substrate is a federated marketplace operating on the universal vocabulary of concepts and the universal protocols of PEP 1.7.0, witness consensus, effect propagation, and policy profiles.

This ruling locks R1-R13 as the architectural contract. The migration sequence in §6 operationalizes it. The work product paused in #1106 will be re-applied against the refined manifest schema once the migration sequence merges.

---

*End of ruling.*
