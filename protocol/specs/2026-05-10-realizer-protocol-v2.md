# Realizer Protocol v0.2

**Status:** v0.2.0 draft extension protocol
**Date:** 2026-05-10
**Layer:** extension protocol over ProofIR, ORP v0.1, LSP, AMP, CBP, lifters, and host kits
**Related:**
- `2026-05-06-obligation-realizer-protocol.md` - ORP v0.1 obligation realizer modes
- `2026-05-06-proofir-realization-compiler.md` - proof-first obligation realization and artifact synthesis
- `2026-05-06-checker-bytecode-protocol.md` - checker bytecode carriers
- `2026-05-09-language-signature-protocol.md` - language signatures, morphisms, and homomorphism discharge
- `2026-05-09-algorithm-memento-protocol.md` - operation and algorithm mementos
- `docs/papers/09-lossy-boundary-compression.md` - contract-boundary loss and output constraint
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md` - languages as content-addressed algebras
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md` - CIDs and receipts as the portable correctness bundle
- `docs/papers/15-after-civilization-why-the-author-doesnt-matter.md` - verification by bytes, receipts, and policy

## Section 0. Purpose

ORP v0.1 defines obligation realizers: witnessers, droppers, and future
monitors. Those modes operate at the obligation and contract stratum.

This v0.2 document generalizes the protocol name from Obligation
Realizer Protocol to Realizer Protocol, while preserving ORP v0.1
unchanged. It adds two modes:

```
compile         : Term * Target -> ConcreteCode | Refusal
realize-witness : Witness * Target -> RunnableTest | Refusal
```

The new modes do not turn contracts into implementations. They introduce
the term stratum as the implementation-preserving stratum and the witness
stratum as the sampled-contract stratum. ORP v0.1 remains the protocol
for realizing obligations as evidence, repair candidates, or monitors.

The v0.2 slogan:

```
lift up to the term algebra and contract boundary;
realize down through discharged morphisms;
keep CIDs and receipts in the middle.
```

## Section 1. Relationship to ORP v0.1

This document is additive. It does not replace
`2026-05-06-obligation-realizer-protocol.md`.

ORP v0.1 remains authoritative for:

- `attest` mode;
- `transform` mode;
- future `monitor` mode;
- `RealizerPlan`, `RealizerOutput`, `WitnessResult`,
  `TransformResult`, and `RefusalResult` for obligation modes;
- the rule that droppers are accepted only after re-lift and closure
  witness;
- the rule that checker bytecode is checker bytecode, not application
  bytecode;
- the non-goal that ProofIR contracts do not compile into application
  implementations.

ORP v0.2 adds:

- the two-strata framing of ProofIR terms and ProofIR contracts;
- `compile` mode for term realization;
- `realize-witness` mode for witness realization;
- catalog placement of compile realizers as LSP morphisms;
- correctness theorems for lift followed by compile;
- witness portability through runnable tests.

Mode taxonomy:

| Stratum | Mode | Authority | Output |
|---|---|---|---|
| Contract or obligation | `attest` | Non-mutating evidence | `WitnessResult` or `RefusalResult` |
| Contract or obligation | `transform` | Mutating candidate, accepted only after re-lift closure | `TransformResult` or `RefusalResult` |
| Contract or obligation | `monitor` | Future long-lived attestation | `MonitorResult` or `RefusalResult` |
| Term | `compile` | Deterministic homomorphic realization | `CompileResult` or `RefusalResult` |
| Witness | `realize-witness` | Deterministic test realization | `RunnableTestResult` or `RefusalResult` |

## Section 2. The two strata of ProofIR

ProofIR has two distinct strata in this protocol.

### Section 2.1 Term stratum

A ProofIR term is an AST over operation CIDs.

The operation CIDs are the universal alphabet:

```
seq, if, while, call, return, eq, deref, add, store, load, alloc, trap, ...
```

Each operation is minted as an `AlgorithmMemento` or a related language
operation memento under AMP and LSP. A language signature is the bundle
of its sorts, operations, equations, and effect signatures. A term is a
finite tree over that signature, modulo the signature equations.

A term is not lossy. It fully determines the implementation at the
abstraction level named by its operations. It is the implementation,
expressed over abstract operation CIDs rather than concrete syntax.

Every imperative language construct maps to applications of these
operations, possibly after expansion through that language's signature.
For example, a C `if`, a Rust `if`, a Java conditional, and a JVM
conditional branch can all map through discharged morphisms to the
appropriate operation-CID terms. A lifter that supports a source language
therefore produces a term, plus contract projections derived from the
term.

The term stratum is where ProofIR represents programs.

### Section 2.2 Contract stratum

The contract stratum contains:

- preconditions;
- postconditions;
- invariants;
- effects;
- resource states;
- signer claims;
- implication edges;
- gaps.

The contract stratum is the lossy boundary projection of the term. The
usual projection is weakest-precondition propagation, strongest
postcondition propagation, effect extraction, resource-state extraction,
or another accepted boundary extractor over the term.

Paper 9's claim that ProofIR is universal because it forgets applies to
this stratum. Loss is the feature. Two different terms can have the same
contract CID. At the contract layer, the substrate compares the contract
CIDs and implication edges, not the term CIDs.

The contract stratum is where ProofIR represents obligations.

### Section 2.3 Why the distinction is load-bearing

The following direction remains out of scope:

```
ProofIR contract + values -> host implementation
```

That is synthesis. A contract states a boundary obligation. Many
different implementations can satisfy it.

The following direction is in scope:

```
ProofIR term -> host code
```

That is compilation. The term already is the implementation, written over
abstract operations instead of concrete syntax.

ORP v0.1's `attest`, `transform`, and `monitor` modes operate on the
contract and obligation stratum. ORP v0.2's `compile` mode operates on
the term stratum. ORP v0.2's `realize-witness` mode operates on the
witness stratum, which is a sampled specialization of the contract
stratum tied to concrete bindings.

## Section 3. `compile` mode

`compile` mode is the term realizer. It is the dual of lift.

```
compile : Term * Target -> ConcreteCode | Refusal
```

A `compile` mode realizer consumes a ProofIR term CID and emits concrete
target code. It is deterministic for byte-equal inputs, byte-equal
target descriptors, byte-equal evaluation-model hints, byte-equal ABI
bindings, and byte-equal policy.

The output is application code only because the input is a term. The
output is not synthesized from a contract.

### Section 3.1 Algebraic interpretation

A `compile` mode realizer is a homomorphism from the term algebra to the
target representation.

In LSP terms, it is represented by a `LanguageMorphismMemento`:

```
M_ProofIR_Target : ProofIRTermSignature -> TargetSignature
```

The morphism's body CID names the compiler implementation, translation
table, verified backend, or other deterministic lowering mechanism. The
morphism's postcondition is the LSP homomorphism obligation:

```
for each operation op in the ProofIR term signature:
  compile(apply(op, args))
  =
  target_apply(image(op), compile(args))

for each equation eq in the source signature:
  the target entails compile(eq.lhs) = compile(eq.rhs)
```

The morphism is accepted only with a `MorphismDischargeReceipt` produced
under LSP Section 4.3. The receipt certifies that the homomorphism
obligation holds under the accepted prove portfolio and policy.

### Section 3.2 Contract preservation obligation

The homomorphism discharge must imply contract preservation.

Let:

- `t` be a ProofIR term;
- `contract(t)` be the accepted contract projection of `t`;
- `R_T` be a `compile` mode realizer targeting language or machine `T`;
- `lift_T` be the accepted lifter from target artifacts back to ProofIR;
- `repr_T` be the target representation morphism for contracts, if the
  target has representation-specific encodings.

Then a conformant `compile` mode realizer must discharge:

```
contract(lift_T(R_T(t))) = repr_T(contract(t))
```

When `repr_T` is identity, this is:

```
contract(lift_T(R_T(t))) = contract(t)
```

The equality is CID equality when the projected contract bytes are
canonical and byte-identical. Otherwise it is equality through accepted
contract morphism receipts.

### Section 3.3 `CompilePlan`

A `CompilePlan` is the input to `compile` mode.

```json
{
  "kind": "CompilePlan",
  "schemaVersion": "1",
  "mode": "compile",
  "termCid": "blake3-512:...",
  "sourceSignatureCid": "blake3-512:...",
  "target": {
    "descriptor": "rust/1.75/source",
    "signatureCid": "blake3-512:...",
    "artifactKind": "source-file",
    "mediaType": "text/rust"
  },
  "evaluationModel": {
    "shape": "ssa",
    "memoryModelCid": "blake3-512:...",
    "effectModelCid": "blake3-512:..."
  },
  "abi": {
    "callingConventionCid": "blake3-512:...",
    "bindingCid": "blake3-512:..."
  },
  "morphismCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"CompilePlan"`. |
| `schemaVersion` | MUST be `"1"` for this draft shape. |
| `mode` | MUST be `"compile"`. |
| `termCid` | CID of the ProofIR term to realize. |
| `sourceSignatureCid` | CID of the ProofIR term signature used by the term. |
| `target` | Target descriptor, signature CID, artifact kind, and media type. |
| `evaluationModel` | Register, stack, SSA, memory, and effect-model hints required by the target. |
| `abi` | Calling convention and binding ABI for entrypoints, values, traps, and effects. |
| `morphismCid` | `LanguageMorphismMemento` CID for source to target lowering. |
| `policyCid` | Policy governing accepted morphisms, receipts, target forms, and determinism. |
| `inputCids` | Prior artifacts, catalogs, or receipts the plan composes against. |

### Section 3.4 `CompileResult`

```json
{
  "kind": "RealizerOutput",
  "schemaVersion": "2",
  "mode": "compile",
  "status": "compiled",
  "planCid": "blake3-512:...",
  "termCid": "blake3-512:...",
  "target": {
    "descriptor": "rust/1.75/source",
    "signatureCid": "blake3-512:...",
    "artifactKind": "source-file",
    "mediaType": "text/rust"
  },
  "emittedArtifactCid": "blake3-512:...",
  "morphismCid": "blake3-512:...",
  "morphismDischargeReceiptCid": "blake3-512:...",
  "contractProjectionCid": "blake3-512:...",
  "postLiftCid": "blake3-512:...",
  "contractPreservationReceiptCid": "blake3-512:...",
  "realizer": {
    "name": "sugar-ir-compiler-rust",
    "version": "0.2.0",
    "kit": "rust"
  },
  "diagnostics": []
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"RealizerOutput"`. |
| `schemaVersion` | MUST be `"2"` for this draft shape. |
| `mode` | MUST be `"compile"`. |
| `status` | MUST be `"compiled"` for successful outputs. |
| `planCid` | CID of the `CompilePlan`. |
| `termCid` | CID of the realized term. |
| `emittedArtifactCid` | CID of the target concrete code bytes. |
| `morphismCid` | CID of the LSP morphism used by the realizer. |
| `morphismDischargeReceiptCid` | CID of the LSP discharge receipt for the morphism. |
| `contractProjectionCid` | CID of `contract(termCid)`, if projected during the run or cited from the plan. |
| `postLiftCid` | Optional post-lift output CID for target code, required when policy requires empirical round-trip evidence. |
| `contractPreservationReceiptCid` | Receipt that target code preserves the source term's contract projection. |
| `realizer` | Realizer identity and version. |
| `diagnostics` | Deterministic diagnostics. |

If the realizer cannot compile the term under the target, ABI, morphism,
or policy, it returns ORP v0.1's `RefusalResult` with:

```
mode = "compile"
status = "rejected"
```

### Section 3.5 Round-trip correctness theorem

**Theorem (round-trip correctness for verified cross-compilation).** Let
`L_A : SourceA -> Term` be a lifter whose lift-homomorphism obligation
has a discharge receipt. Let `R_B : Term -> SourceB` be a `compile` mode
realizer whose realize-homomorphism obligation has a discharge receipt.
Let `contract` be the accepted contract projection from terms or source
artifacts to the contract stratum. Then for every source artifact `a` in
the accepted domain of `L_A`:

```
contract(R_B(L_A(a))) = contract(a)
```

up to the target representation morphism and accepted contract
equivalence receipts.

The proof is the composition of:

1. `L_A`'s lift-homomorphism discharge, establishing that lifting
   preserves the source artifact's term and contract meaning;
2. `R_B`'s realize-homomorphism discharge, establishing that compiling
   the term into `SourceB` preserves the term algebra and its contract
   projection;
3. the LSP composition rule, which says discharged morphisms compose and
   their composed discharge factors through the input receipts;
4. the substrate hub property: the ProofIR term algebra and contract
   projection sit at the content-addressed colimit where source and
   target morphisms meet.

Therefore `R_B . L_A : SourceA -> SourceB` is verified
cross-compilation. No pairwise re-verification of `SourceA -> SourceB` is
needed beyond the accepted lifter receipt, compile-realizer receipt, and
their catalog composition.

### Section 3.6 N plus M factoring

Without the term-algebra hub, cross-compilation requires point-to-point
components:

```
N sources * M targets
```

With v0.2:

```
N source lifters + M target realizers
```

Adding a source language requires one lifter to the term algebra. Adding
a target requires one `compile` mode realizer from the term algebra to
that target. All source-target pairs compose through the same
content-addressed hub.

This is not a cache trick. It is the LSP composition rule applied to the
initial term algebra and discharged morphisms.

### Section 3.7 Differential testing against native compilers

`compile` mode also gives a substrate-native differential testing loop
for existing compilers.

Example:

```
source C
  -> lift_C
  -> ProofIR term
  -> compile_to_x86
  -> x86 artifact
  -> lift_x86
  -> ProofIR term or contract
```

In parallel:

```
source C
  -> gcc
  -> x86 artifact
  -> lift_x86
  -> ProofIR term or contract
```

The comparison is at the contract-CID level unless policy demands term
CID equality:

```
contract(lift_x86(compile_to_x86(lift_C(source))))
=
contract(lift_x86(gcc(source)))
```

If the CIDs match, the native compiler preserved the lifted behavior
under the accepted lifters and policies. If they differ, the result is a
miscompile, a target-lifter bug, a source-lifter bug, an underspecified
contract projection, or an unsupported target feature. The substrate does
not guess which one. It records the mismatch and the receipts needed to
triage it.

### Section 3.8 Catalog placement

`compile` mode realizers register as LSP morphisms:

```
protocol/language-catalog/morphisms/
  proofir-term:<version>__to__<target>:<version>.<morphism_cid>.json
```

Each registered compiler must provide or cite:

- source signature CID;
- target signature CID;
- `LanguageMorphismMemento` CID;
- implementation body CID;
- accepted `MorphismDischargeReceipt` CID;
- supported target descriptor family;
- supported evaluation-model hints;
- supported calling convention and ABI binding CIDs;
- policy requirements;
- deterministic serialization rules for emitted artifacts.

The realizer catalog entry for ORP v0.2 points at the same morphism CID
and receipt CID. The LSP catalog is the source of truth for morphism
correctness. ORP v0.2 is the mode and wire protocol that invokes it.

### Section 3.9 Bootstrap path

The existing prover-output crates:

```
sugar-ir-compiler-coq
sugar-ir-compiler-smt-lib
sugar-ir-compiler-maude
sugar-ir-compiler-lean
```

are compile-mode-adjacent, but they are not automatically `compile` mode
realizers under this v0.2 spec. Their usual input is a formula or
obligation and their target is a prover query or proof assistant input.
They belong primarily to the IR compiler and discharge pipeline.

They may become formal `compile` mode realizers only for cases where:

1. the input is a ProofIR term, not only an obligation;
2. the target prover language is modeled as an LSP target signature;
3. the lowering is registered as a `LanguageMorphismMemento`;
4. the homomorphism obligation is discharged by a
   `MorphismDischargeReceipt`;
5. the output artifact is the concrete target representation of the term.

Until then, they should be described as sibling `discharge` or `lower`
mode components, not as executable-code `compile` mode realizers.

The new executable-code crates are `compile` mode realizers:

```
sugar-ir-compiler-x86-64
sugar-ir-compiler-wasm
sugar-ir-compiler-jvm-bytecode
sugar-ir-compiler-c
sugar-ir-compiler-rust
```

Each implements `CompilePlan -> CompileResult | RefusalResult` and
registers the corresponding LSP morphism and discharge receipt.

## Section 4. `realize-witness` mode

`realize-witness` mode is the witness realizer.

```
realize-witness : Witness * Target -> RunnableTest | Refusal
```

It is deterministic for byte-equal inputs, target descriptors, bindings,
compiler choices, and policy.

### Section 4.1 Witnesses as specialized contracts

A witness is a specialized contract. For example:

```
shape S on input i produces output o and no trap occurs
```

This can be represented as a contract with a singleton precondition:

```
pre  = input == i
post = output == o and no_trap
```

A witness is admitted as a specialization of a general contract only
when the specialization edge is discharged:

```
general_contract -> witness_contract
```

or when policy accepts the witness as sampled evidence without promoting
it to a universal obligation.

### Section 4.2 Realizing a witness

Realizing a witness to a target means emitting a runnable test in that
target.

The standard lowering is:

1. compile the witness's term or entrypoint to the target using
   `compile` mode;
2. bind the singleton input values;
3. run or prepare a target-native harness;
4. assert the bound output values, traps, effects, and resource states;
5. emit a `RunnableTestResult`.

If the witness does not contain a term or entrypoint, policy may allow a
target-native test that calls an existing realization named by CID. The
test must still cite the witness CID and binding CID.

### Section 4.3 `WitnessRealizePlan`

```json
{
  "kind": "WitnessRealizePlan",
  "schemaVersion": "1",
  "mode": "realize-witness",
  "witnessCid": "blake3-512:...",
  "witnessContractCid": "blake3-512:...",
  "specializationReceiptCid": "blake3-512:...",
  "termCid": "blake3-512:...",
  "target": {
    "descriptor": "junit/5/java/17",
    "signatureCid": "blake3-512:...",
    "artifactKind": "test-source-file",
    "mediaType": "text/x-java-source"
  },
  "binding": {
    "inputBindingCid": "blake3-512:...",
    "outputBindingCid": "blake3-512:...",
    "effectBindingCid": "blake3-512:..."
  },
  "compilePlanCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"WitnessRealizePlan"`. |
| `schemaVersion` | MUST be `"1"` for this draft shape. |
| `mode` | MUST be `"realize-witness"`. |
| `witnessCid` | CID of the witness to realize. |
| `witnessContractCid` | CID of the singleton or sampled contract expressed by the witness. |
| `specializationReceiptCid` | Receipt proving the witness specializes a general contract, when policy requires it. |
| `termCid` | CID of the term or entrypoint under test, when known. |
| `target` | Target test framework, signature, artifact kind, and media type. |
| `binding` | Input, output, effect, trap, and resource-state bindings. |
| `compilePlanCid` | Optional or required plan for lowering the term to the target, according to policy. |
| `policyCid` | Policy governing portability, target harnesses, and witness strength. |
| `inputCids` | Prior artifacts, catalogs, or receipts the plan composes against. |

### Section 4.4 `RunnableTestResult`

```json
{
  "kind": "RealizerOutput",
  "schemaVersion": "2",
  "mode": "realize-witness",
  "status": "test-emitted",
  "planCid": "blake3-512:...",
  "witnessCid": "blake3-512:...",
  "testArtifactCid": "blake3-512:...",
  "target": {
    "descriptor": "junit/5/java/17",
    "signatureCid": "blake3-512:...",
    "artifactKind": "test-source-file",
    "mediaType": "text/x-java-source"
  },
  "compileResultCid": "blake3-512:...",
  "realizer": {
    "name": "sugar-witness-realizer-java",
    "version": "0.2.0",
    "kit": "java"
  },
  "diagnostics": []
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"RealizerOutput"`. |
| `schemaVersion` | MUST be `"2"` for this draft shape. |
| `mode` | MUST be `"realize-witness"`. |
| `status` | MUST be `"test-emitted"` for successful outputs. |
| `planCid` | CID of the `WitnessRealizePlan`. |
| `witnessCid` | CID of the witness realized by the test. |
| `testArtifactCid` | CID of the emitted runnable test artifact. |
| `target` | Target test framework and artifact descriptor. |
| `compileResultCid` | Compile result used to lower the witness term, if any. |
| `realizer` | Realizer identity and version. |
| `diagnostics` | Deterministic diagnostics. |

If the realizer cannot emit a runnable test under the target or policy,
it returns ORP v0.1's `RefusalResult` with:

```
mode = "realize-witness"
status = "rejected"
```

### Section 4.5 Verified test portability

Let:

```
W
```

be a witness CID, and let:

```
RW_T : Witness -> RunnableTest_T
```

be a `realize-witness` mode realizer for target `T`. Let `lift_test_T`
be a lifter from target test artifacts back to witness contracts. Then:

```
lift_test_T(RW_T(W)) = W
```

up to accepted witness-contract equivalence receipts.

This is verified test portability. The same witness can be realized to
Java JUnit, Rust test, Python pytest, TypeScript Vitest, KUnit, WASM
harness, or another target. Each runnable test is a different host
artifact, but all of them point back to the same witness CID through
their lift or execution receipts.

### Section 4.6 Differential testing across languages

Given two targets:

```
RW_A(W) -> RunnableTest_A
RW_B(W) -> RunnableTest_B
```

the tests are different realizations of the same witness. Running both
against their respective compiled or native implementations gives an
empirical cross-check of the symbolic morphism:

```
run(RunnableTest_A) -> WitnessResult_A
run(RunnableTest_B) -> WitnessResult_B
```

If both witness results attest the same witness CID under accepted
policy, the targets agree on that sampled point. This is not a proof of
universal equivalence. It is portable sampled evidence that complements
the morphism discharge.

### Section 4.7 Connection to ORP v0.1 `attest`

A `RunnableTest` is a host artifact. Running it produces an ORP v0.1
`WitnessResult` in `attest` mode.

Therefore:

```
realize-witness produces inputs for attest
```

The witness corpus feeds and is fed by:

- KUnit lifters;
- future test lifters;
- property-test lifters;
- regression-test lifters;
- differential-test harnesses;
- witness mining from historical bug fixes.

The runnable test is not the witness by itself. The witness is the
content-addressed contract sample. The runnable test is one target
realization of it.

## Section 5. Reconciliation with existing specs

### Section 5.1 ORP v0.1

ORP v0.1's modes are unchanged:

```
attest
transform
monitor
```

They remain obligation-realizer modes. Their trust rules, wire shapes,
and non-goals remain in force for obligation realization.

ORP v0.2 adds:

```
compile
realize-witness
```

These are term and witness realizer modes. They do not weaken the
requirement that droppers are accepted only after re-lift and closure.

### Section 5.2 CBP

CBP governs checker bytecode carriers.

When ORP v0.1 lowers a predicate or edge to executable checker bytecode,
CBP defines the carrier, binding ABI, checker memento, checker witness,
and checker refusal shapes.

`compile` mode may target CBP checker bytecode only when the input is a
term whose intended output is checker behavior. In that case:

- ORP v0.2 governs the `CompilePlan` and `CompileResult`;
- LSP governs the term-to-target morphism and homomorphism discharge;
- CBP governs the checker bytecode carrier and execution receipts.

For the ordinary predicate-to-checker case, use ORP v0.1 plus CBP and
the realization-compiler spec. Do not reclassify predicate-to-checker
lowering as term compilation unless the input artifact is a term.

### Section 5.3 ProofIR Realization Compiler

`2026-05-06-proofir-realization-compiler.md` remains the design for:

```
obligation + target constraints -> satisfying artifact candidate
```

This v0.2 spec does not supersede it.

Instead, v0.2 is the protocol-level wrapper that places realization
activities in the broader realizer taxonomy:

- obligation-to-checker, obligation-to-guard, obligation-to-schema, and
  obligation-to-repair remain realization-compiler problems used by ORP
  v0.1 `attest`, `transform`, and `monitor`;
- term-to-target lowering is ORP v0.2 `compile` mode and is represented
  by LSP morphisms;
- witness-to-test lowering is ORP v0.2 `realize-witness` mode and may
  call `compile` mode internally.

If the realization-compiler design later adds a term-to-target backend,
this v0.2 spec defines the ORP transport and trust envelope for that
backend.

### Section 5.4 LSP

LSP is the authority for language signatures, language morphisms, and
morphism discharges.

ORP v0.2 `compile` mode is an invocation protocol for a discharged LSP
morphism. The compile realizer is not accepted because it emitted code.
It is accepted because:

1. its morphism is registered as a `LanguageMorphismMemento`;
2. its homomorphism obligation has a `MorphismDischargeReceipt`;
3. its emitted artifact is content-addressed;
4. its contract preservation receipt satisfies policy;
5. optional post-lift evidence agrees with the source term's contract
   projection when policy requires empirical round-trip evidence.

### Section 5.5 Paper 9 non-goal preservation

Paper 9 and ORP v0.1 reject:

```
ProofIR + values -> host implementation
```

This v0.2 spec preserves that rejection for the contract stratum.

The clarifying rule is:

```
contract -> implementation  = synthesis, out of ORP compile scope
term -> host code           = compilation, in ORP compile scope
```

Synthesis may still happen through the ORP v0.1 `transform` and dropper
loop, but the output is accepted only after re-lift and closure witness.
Compilation happens only when the input term already contains the
implementation.

## Section 6. Trust rules

1. **Contracts do not compile to implementations.** Only terms compile
   to target code. Contracts can constrain, witness, or guide synthesis,
   but they do not determine a unique implementation.

2. **Compile realizers are morphisms.** A `compile` mode realizer is
   accepted only through a registered LSP morphism and accepted
   homomorphism discharge.

3. **Contract preservation is explicit.** A compile result must cite a
   receipt or policy-accepted evidence that the emitted artifact
   preserves the source term's contract projection.

4. **Target compilers are not trust anchors.** A native compiler,
   assembler, bytecode validator, or parser is a target constraint. It
   does not by itself prove term or contract preservation.

5. **Post-lift evidence strengthens, but does not replace, morphism
   discharge.** Policy may require lifting emitted artifacts back into
   ProofIR. That empirical or structural round trip is evidence in
   addition to the morphism discharge.

6. **Witness tests are realizations of witness CIDs.** A runnable test is
   accepted as portable evidence only when it cites the witness CID and
   its execution or lift result returns to that witness under policy.

7. **Refusals fail closed.** Unsupported operations, target descriptors,
   ABI bindings, morphisms, witness bindings, or policies return
   `RefusalResult`.

8. **Determinism is required.** Byte-equal plans under byte-equal
   catalogs and policies must produce byte-equal successful artifacts, or
   else record all nondeterminism as explicit provenance accepted by
   policy.

9. **Core verification does not execute generated artifacts.** Core
   verification checks CIDs, signatures, receipts, and references.
   Execution belongs to extension-aware tooling under explicit policy.

## Section 7. Kit responsibilities

A conformant v0.2 kit implementing `compile` mode:

1. Accepts `CompilePlan` inputs and returns exactly one `CompileResult`
   or `RefusalResult`.
2. Registers or cites a `LanguageMorphismMemento` from ProofIR term
   signature to target signature.
3. Cites an accepted `MorphismDischargeReceipt`.
4. Emits target artifacts with canonical bytes and CIDs.
5. Records target descriptor, evaluation-model hints, ABI binding, and
   policy CID.
6. Emits or cites contract preservation evidence.
7. Refuses unsupported operations, target descriptors, ABIs, or policies.
8. Produces byte-stable output for byte-equal inputs unless policy
   permits explicit nondeterministic provenance.

A conformant v0.2 kit implementing `realize-witness` mode:

1. Accepts `WitnessRealizePlan` inputs and returns exactly one
   `RunnableTestResult` or `RefusalResult`.
2. Cites the witness CID and witness contract CID in the emitted test
   artifact metadata or associated memento.
3. Cites specialization receipts when policy requires the witness to be
   derived from a general contract.
4. Uses `compile` mode when a witness term must be lowered to the target.
5. Emits runnable test artifacts with canonical bytes and CIDs.
6. Produces tests whose execution can emit ORP v0.1 `WitnessResult`
   outputs.
7. Refuses unsupported test targets, bindings, or policies.

## Section 8. Worked examples

### Section 8.1 C to Rust via the term hub

Input:

```
C source artifact CID
```

Lift:

```
lift_C(source) -> ProofIR term CID
```

Realize:

```
compile(term CID, rust/1.75/source) -> Rust source CID
```

Correctness:

```
contract(Rust source CID) = contract(C source CID)
```

The proof is not a new C-to-Rust compiler proof. It is the composition of
the C lifter receipt, the ProofIR-to-Rust morphism discharge, and the
contract preservation receipt.

### Section 8.2 WASM executable target

Input:

```
ProofIR term CID
```

Target:

```
wasm32-wasi module
```

Plan:

```
CompilePlan(term, target=wasm32-wasi, evaluationModel=stack,
            abi=wasi-binding-cid)
```

Result:

```
CompileResult(emittedArtifactCid=<wasm module cid>,
              morphismDischargeReceiptCid=<receipt cid>)
```

The WASM validator constrains the output shape. The LSP morphism receipt
and contract preservation receipt carry correctness.

### Section 8.3 Predicate checker bytecode

Input:

```
predicate CID
```

Target:

```
checker-bytecode
```

This is normally not ORP v0.2 `compile` mode. It is an ORP v0.1
obligation realization that uses CBP as the bytecode carrier.

Input:

```
ProofIR checker term CID
```

Target:

```
checker-bytecode
```

This may be ORP v0.2 `compile` mode, with CBP governing the carrier.

### Section 8.4 Witness to JUnit and pytest

Witness:

```
input:  amount = 100
output: accepted = true
effect: no_trap
```

Realizations:

```
realize-witness(W, junit/5/java/17) -> JUnit test CID
realize-witness(W, pytest/python/3.12) -> pytest test CID
```

Executions:

```
run(JUnit test) -> WitnessResult(W)
run(pytest test) -> WitnessResult(W)
```

The tests are not source-equivalent. They are witness-equivalent because
they return to the same witness CID under policy.

## Section 9. Open questions

1. Should `CompilePlan` be folded into `RealizerPlan` with
   `schemaVersion = "2"`, or should mode-specific plan kinds remain
   separate?
2. What is the canonical ProofIR term memento shape for executable terms
   over operation CIDs?
3. Which target signatures should be minted first: x86-64, WASM, JVM
   bytecode, C, Rust, or prover targets?
4. How much post-lift evidence should policy require when an LSP morphism
   already has a formal discharge?
5. Should `realize-witness` require a specialization receipt for every
   witness, or allow sampled evidence classes that do not promote to
   general contract edges?
6. Should prover backends become formal `compile` mode realizers when
   their targets are modeled as LSP signatures, or remain a sibling
   `discharge` mode?
7. How should target-specific undefined behavior, traps, and resource
   states be represented in the term signature so compile realizers
   cannot erase them?
8. What is the first policy profile for differential testing against
   native compilers?

## Section 10. Conformance

A Realizer Protocol v0.2 implementation is conformant for `compile` mode
if it:

1. Implements `CompilePlan -> CompileResult | RefusalResult`.
2. Requires a registered `LanguageMorphismMemento`.
3. Requires an accepted `MorphismDischargeReceipt`.
4. Emits canonical target artifact bytes and an `emittedArtifactCid`.
5. Cites policy, target descriptor, evaluation model, ABI binding, and
   realizer identity.
6. Emits or cites contract preservation evidence.
7. Refuses unsupported terms, target descriptors, morphisms, ABIs, or
   policies.
8. Preserves deterministic output under byte-equal inputs or records
   policy-accepted nondeterministic provenance.

A Realizer Protocol v0.2 implementation is conformant for
`realize-witness` mode if it:

1. Implements
   `WitnessRealizePlan -> RunnableTestResult | RefusalResult`.
2. Emits runnable test artifacts with canonical bytes and CIDs.
3. Cites witness CID, witness contract CID, target descriptor, bindings,
   policy, and realizer identity.
4. Uses `compile` mode when term lowering is required by the plan.
5. Produces test artifacts whose execution can emit ORP v0.1
   `WitnessResult` outputs.
6. Refuses unsupported witnesses, targets, bindings, or policies.

An implementation may conform to either new mode independently. ORP
v0.1 conformance remains defined by ORP v0.1 Section 12.

## Section 11. Citation

Cite as:

> Sugar Protocol Working Notes (2026). *Realizer Protocol v0.2: Term and Witness Realizers*. Draft extension protocol v0.2.0.
