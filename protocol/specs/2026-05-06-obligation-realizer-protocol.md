# Obligation Realizer Protocol (ORP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-06
**Layer:** extension protocol over the ProofIR/memento substrate
**Related:**
- `2026-05-06-extension-protocols.md` - extension-protocol doctrine and executable metadata DAG ordering
- `2026-05-06-truth-discharge-protocol.md` - positive witness discharge as unit truth over a body-claim
- `2026-05-06-grammar-conformance-protocol.md` - ORP plan/result grammar and invariant conformance
- `2026-04-30-lift-plugin-protocol.md` - host artifact -> canonical ProofIR
- `2026-04-30-agent-plugin-protocol.md` - agent proposal surface
- `2026-05-06-provenance-memento.md` - discharge memento shape and signing discipline
- `2026-05-06-effect-discharge-classification.md` - effect discharge taxonomy
- `2026-05-06-proofir-realization-compiler.md` - proof-first realization and artifact synthesis
- `docs/superpowers/specs/2026-05-06-bug-zoo-design.md` - exposed/dropped lifecycle and re-lift acceptance rule
- `docs/papers/09-lossy-boundary-compression.md` - paper-grade argument for boundary-domain loss and output constraint

## Section 0. Purpose

ORP defines the shared protocol shape for host-space realizers of ProofIR obligations: non-mutating witnessers, mutating droppers, and future monitor/checker bytecode targets.

ORP is an extension protocol. It does not add a core substrate primitive. It specifies a conventional host-space workflow over existing ProofIR obligations, mementos, CIDs, signatures, policies, and lift results.

The main protocol is assumed complete enough that ORP can operate entirely on top of it. A consumer that does not understand ORP can still verify the underlying mementos, edges, signatures, and CIDs that ORP outputs reference. ORP adds interoperability among producers of realization artifacts; it does not change core verification.

ORP plans and outputs MAY be represented as executable extension metadata evaluated over the memento DAG per `2026-05-06-extension-protocols.md`. Witnessers, droppers, and monitors are extension interpreters or interpreter targets. They are not core verifier behavior. Core verification validates the signed bytes and references; ORP-aware tooling executes or interprets realization metadata to produce optional witness, transform, or monitor results.

ORP positive attestations MAY use the Truth Discharge Protocol (TDP): the realizer emits a signed witness that says the body-claim named by the result is true under the cited policy. Transform results are accepted only when the post-transform body-claim receives such a witness or an equivalent policy-accepted closure witness.

ORP plan and result bodies SHOULD be witnessable under the Grammar Conformance Protocol (GCP). A policy MAY require GCP witnesses for `RealizerPlan`, `WitnessResult`, `TransformResult`, and `RefusalResult` bodies before accepting ORP workflow claims.

ProofIR is a canonical language for boundary obligations. Host software is where those obligations become concrete. ORP is the seam between the two:

```
ProofIR obligation + host realization context -> evidence, transformation, or refusal
```

The protocol exists because witnessers and droppers are not separate architectural species. They share obligation IDs, binding maps, host kit metadata, policy CIDs, provenance, result envelopes, refusal shapes, and closure semantics. They differ in authority and effect.

The slogan:

```
Witnesser: show me it holds.
Dropper:   make it hold, then show me.
Monitor:   keep showing me while it runs.
```

## Section 1. Core distinction

A **witnesser** observes or checks. It is epistemic.

```
attest : Obligation * HostContext -> Evidence | Refusal
```

A **dropper** changes a host artifact. It is constructive, but its construction is not trusted by itself.

```
transform : Gap * HostArtifact -> HostArtifact' | Refusal
```

A complete dropper pipeline is a composition:

```
closeByTransform = transform > lift > attest
```

or, expanded:

```
missing edge -> native repair candidate -> re-lift -> closure check
```

This direction is normative. ORP explicitly rejects:

```
ProofIR + values -> host implementation
```

ProofIR does not compile to application behavior. ProofIR MAY compile to witnesser/checker bytecode whose job is to attest admissibility at a boundary. The compiled target is a checker, not an implementation.

## Section 2. Vocabulary

**Obligation.** A ProofIR predicate, ProofIR implication edge, or gap. A gap is a missing edge together with enough context to ask a host kit to realize a closure.

**Predicate.** A canonical ProofIR formula with a predicate CID.

**Edge.** A source predicate CID, target predicate CID, and the claim that the source implies the target under an accepted witness policy.

**Gap.** A required edge absent from the accepted substrate at a particular host boundary.

**HostContext.** The host-space material needed to realize an obligation: runtime values, source artifacts, build artifacts, boundary invocation records, environment state, or a combination of these.

**BindingMap.** A mapping from ProofIR variables to host-space values, fields, source ranges, symbolic names, or runtime observation paths.

**Realizer.** Any host-kit component that implements ORP for one or more modes.

**Witnesser.** A non-mutating realizer. It observes host context and emits evidence or refusal.

**Dropper.** A mutating realizer. It emits a host artifact candidate and is accepted only after re-lift and closure verification.

**ProofPlan.** A target-neutral ORP sub-artifact that names the forbidden region, usually `p and not(q)` for an edge `p -> q`, and the eliminator strategy intended to make that region uninhabitable. A proof plan is a proof-first realization artifact, not a host-language patch.

**LanguageDropperProjection.** A target-specific ORP sub-artifact that binds a `ProofPlan` to a language, kit, surface, symbol, source artifact, output artifact, and post-lift expectation. It says how the target-neutral eliminator is projected into host source or host metadata.

**Monitor.** A future long-lived witnesser. It attaches to a boundary and emits an evidence stream over time.

**Checker bytecode.** Executable code produced from a ProofIR predicate or edge for the purpose of attestation, monitoring, or guard insertion. Checker bytecode is not application bytecode.

## Section 3. Modes

### Section 3.1 `attest`

`attest` mode is non-mutating.

Input:

```
ProofIR obligation + host context + binding map + policy
```

Output:

```
WitnessResult | RefusalResult
```

The realizer MAY run native code, interpret checker bytecode, invoke a verifier, inspect a runtime object, query a schema, inspect source metadata, or consume a signed host artifact. It MUST NOT modify the host artifact or runtime state whose predicate it is witnessing.

Examples:

- Java witnesser observes a Spring controller parameter and witnesses `1 <= amount <= 10000`.
- TypeScript witnesser evaluates a Zod validator against a boundary value and witnesses `email(x)`.
- Rust witnesser observes `Option<T>::is_some()` and witnesses `not_null(x)`.
- SQL witnesser inspects a migration and witnesses `unique(email)`.

### Section 3.2 `transform`

`transform` mode is mutating.

Input:

```
Gap + host artifact + binding map + policy
```

Output:

```
TransformResult | RefusalResult
```

The realizer emits a host artifact candidate: source patch, validator insertion, wrapper, prepared statement conversion, annotation, migration, generated checker attachment, or other native edge-closing shape.

The candidate is not evidence. A `TransformResult` is accepted only if it carries or points to:

1. the transformed artifact CID;
2. the post-transform lift output CID;
3. a closure witness showing the required edge is now discharged.

If any of these are absent, the output is a transform candidate, not an accepted closure.

### Section 3.3 `monitor` (future)

`monitor` mode is a long-lived `attest` variant.

Input:

```
ProofIR obligation + attachment point + binding map + policy
```

Output:

```
MonitorResult | RefusalResult
```

The realizer emits or installs a runtime monitor that produces a stream of witness/refusal events. Examples include eBPF probes, JVM agents, WASM API-gateway plugins, service-mesh filters, browser validators, or database admission hooks.

`monitor` is out of v0.1 conformance. It is included because the data shape should not force a later migration.

## Section 4. Shared wire shapes

ORP uses JCS-canonical JSON for all signed plans and results. Every CID below is `blake3-512:<hex>`.

### Section 4.1 `RealizerPlan`

```json
{
  "kind": "RealizerPlan",
  "schemaVersion": "1",
  "mode": "attest",
  "obligation": {
    "kind": "predicate",
    "predicateCid": "blake3-512:..."
  },
  "host": {
    "kit": "typescript",
    "contextKind": "runtime-value",
    "artifactCid": "blake3-512:...",
    "entrypoint": "POST /transfer"
  },
  "bindings": [
    {
      "proofVar": "amount",
      "hostPath": "body.amount",
      "typeHint": "integer"
    }
  ],
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"RealizerPlan"`. |
| `schemaVersion` | MUST be `"1"` for this draft shape. |
| `mode` | `"attest"`, `"transform"`, or `"monitor"`. |
| `obligation` | Predicate, edge, or gap descriptor. |
| `host` | Host kit and context descriptor. |
| `bindings` | ProofIR-to-host binding map. |
| `policyCid` | Policy used to decide admissibility. |
| `inputCids` | Prior artifacts the plan composes against. |

### Section 4.2 Obligation descriptors

Predicate obligation:

```json
{
  "kind": "predicate",
  "predicateCid": "blake3-512:..."
}
```

Edge obligation:

```json
{
  "kind": "edge",
  "sourcePredicateCid": "blake3-512:...",
  "targetPredicateCid": "blake3-512:..."
}
```

Gap obligation:

```json
{
  "kind": "gap",
  "gapCid": "blake3-512:...",
  "sourcePredicateCid": "blake3-512:...",
  "targetPredicateCid": "blake3-512:...",
  "diagnosticLocation": "src/lib.rs:42"
}
```

### Section 4.3 `ProofPlan`

`ProofPlan` is an optional but strongly preferred sub-artifact for ORP
realization. It is target-neutral. It describes why a missing edge can be
closed before any language dropper decides how to express the closure in
host code.

For an edge:

```
p -> q
```

the default proof-first form names the violation condition:

```
p and not(q)
```

and an objective:

```
unsat(p and not(q))
```

Canonical shape:

```json
{
  "kind": "ProofPlan",
  "schemaVersion": "1",
  "problem": {
    "kind": "orp-realization",
    "planCid": "blake3-512:..."
  },
  "obligation": {
    "kind": "edge",
    "sourcePredicateCid": "blake3-512:...",
    "targetPredicateCid": "blake3-512:...",
    "gapCid": "blake3-512:..."
  },
  "policy": {
    "mode": "proof_preferred",
    "policyCid": "blake3-512:...",
    "degradedEvidence": "mark"
  },
  "violationCondition": {
    "kind": "formula",
    "formulaCid": "blake3-512:..."
  },
  "objective": {
    "kind": "unsat",
    "formulaCid": "blake3-512:..."
  },
  "eliminators": [
    {
      "kind": "strengthen-precondition",
      "predicateCid": "blake3-512:..."
    }
  ],
  "proofWitnessCid": "blake3-512:..."
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"ProofPlan"`. |
| `schemaVersion` | MUST be `"1"` for this draft shape. |
| `problem` | The realization problem, ORP plan, Bug Zoo specimen, or other context that produced the plan. |
| `obligation` | The predicate, edge, or gap the plan is intended to discharge. |
| `policy.mode` | `"proof_required"`, `"proof_preferred"`, or `"proof_optional"`. |
| `policy.policyCid` | Policy governing plan acceptance. |
| `violationCondition` | Formula naming the forbidden region. |
| `objective` | Proof objective, usually unsatisfiability of the violation condition. |
| `eliminators` | One or more target-neutral strategies for eliminating the forbidden region. |
| `proofWitnessCid` | Optional witness for the plan itself. Policy decides whether this is required. |

Policy modes:

| Mode | Meaning |
|---|---|
| `proof_required` | A transform without a validating `ProofPlan` MUST be refused. |
| `proof_preferred` | Proof-first transforms are preferred; proofless transforms MAY be accepted only with degraded-evidence marking. |
| `proof_optional` | Direct candidate generation MAY be accepted when other target evidence satisfies policy. |

Initial eliminator kinds:

| Kind | Meaning |
|---|---|
| `strengthen-precondition` | Reject inputs or states for which the target predicate would fail. |
| `construct-postcondition` | Construct or transform output so the target predicate holds. |
| `preserve-invariant` | Maintain an invariant across a transition. |
| `guard-effect` | Prevent an unsafe effect unless required facts hold. |
| `adapt-boundary` | Insert or use a wrapper, annotation, schema, validator, or adapter at a boundary. |
| `attest-runtime` | Produce runtime evidence that the bound value satisfies the predicate. |
| `monitor-transition` | Emit or attach a checker that continues witnessing over time. |

### Section 4.4 `LanguageDropperProjection`

`LanguageDropperProjection` is an optional ORP sub-artifact for
`transform` mode. It binds a proof plan to one language or framework
surface. It is the formal place to say:

```
this proof plan projects into this host-language shape
```

Canonical shape:

```json
{
  "kind": "LanguageDropperProjection",
  "schemaVersion": "1",
  "proofPlanCid": "blake3-512:...",
  "kit": "java",
  "surface": "java-provekit-native",
  "targetSymbol": "lookup",
  "bindings": [
    {
      "proofVar": "name",
      "hostPath": "parameter:name"
    }
  ],
  "sourceArtifactCid": "blake3-512:...",
  "outputArtifactCid": "blake3-512:...",
  "operation": {
    "kind": "add-boundary-precondition",
    "projection": "strengthen-precondition"
  },
  "postLift": {
    "proofIrCid": "blake3-512:...",
    "expectedPredicateCid": "blake3-512:..."
  }
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"LanguageDropperProjection"`. |
| `schemaVersion` | MUST be `"1"` for this draft shape. |
| `proofPlanCid` | CID of the `ProofPlan` being projected. |
| `kit` | Host kit responsible for the projection. |
| `surface` | Host surface used by the projection. |
| `targetSymbol` | Host symbol or boundary being changed. |
| `bindings` | ProofIR-to-host binding map. |
| `sourceArtifactCid` | Pre-projection host artifact CID when known. |
| `outputArtifactCid` | Candidate or transformed artifact CID when known. |
| `operation` | Host operation and the proof-plan eliminator it realizes. |
| `postLift` | Expected post-lift ProofIR evidence. |

If `LanguageDropperProjection.proofPlanCid` is present, the referenced
`ProofPlan` MUST be available by CID or declared as an external
dependency. A projection without a proof plan is malformed; a proofless
dropper should omit `LanguageDropperProjection` and mark its result
according to policy.

### Section 4.5 `RealizerOutput`

All ORP outputs share:

```json
{
  "kind": "RealizerOutput",
  "schemaVersion": "1",
  "mode": "attest",
  "status": "witnessed",
  "planCid": "blake3-512:...",
  "realizer": {
    "name": "provekit-ts-zod-witnesser",
    "version": "0.1.0",
    "kit": "typescript"
  },
  "diagnostics": []
}
```

`status` determines the result variant.

### Section 4.6 `WitnessResult`

```json
{
  "kind": "RealizerOutput",
  "schemaVersion": "1",
  "mode": "attest",
  "status": "witnessed",
  "planCid": "blake3-512:...",
  "obligationCid": "blake3-512:...",
  "evidenceCid": "blake3-512:...",
  "observedArtifactCids": ["blake3-512:..."],
  "realizer": {
    "name": "provekit-ts-zod-witnesser",
    "version": "0.1.0",
    "kit": "typescript"
  },
  "diagnostics": []
}
```

The `evidenceCid` points to the memento or proof artifact whose bytes verify the witness. The evidence artifact MUST be independently checkable from its bytes plus accepted policy.

### Section 4.7 `TransformResult`

```json
{
  "kind": "RealizerOutput",
  "schemaVersion": "1",
  "mode": "transform",
  "status": "closed",
  "planCid": "blake3-512:...",
  "gapCid": "blake3-512:...",
  "patchCid": "blake3-512:...",
  "proofPolicyMode": "proof_preferred",
  "proofPlanCid": "blake3-512:...",
  "languageDropperCid": "blake3-512:...",
  "transformedArtifactCid": "blake3-512:...",
  "postLiftCid": "blake3-512:...",
  "closureWitnessCid": "blake3-512:...",
  "realizer": {
    "name": "provekit-rust-not-null-dropper",
    "version": "0.1.0",
    "kit": "rust"
  },
  "diagnostics": []
}
```

For accepted transform outputs, `status` MUST be `"closed"` and `closureWitnessCid` MUST be present.

A realizer MAY return `status: "candidate"` for an unapplied or unverified transform, but candidate outputs MUST NOT enter the substrate as closure evidence.

`proofPlanCid` and `languageDropperCid` are optional in the wire shape
but policy-significant. If `proofPolicyMode` is `"proof_required"`, an
accepted `TransformResult` MUST include `proofPlanCid`. If
`languageDropperCid` is present, `proofPlanCid` MUST also be present and
the language dropper projection MUST reference the same proof plan.

Under `proof_preferred`, a transform without `proofPlanCid` MAY be
accepted only if the result explicitly marks degraded evidence in a
policy-recognized field or receipt. Under `proof_optional`, policy may
accept closure evidence without a proof plan.

### Section 4.8 `RefusalResult`

```json
{
  "kind": "RealizerOutput",
  "schemaVersion": "1",
  "mode": "attest",
  "status": "rejected",
  "planCid": "blake3-512:...",
  "reasonCode": "UNSUPPORTED_PREDICATE",
  "message": "predicate safe_for_sql is not supported by this realizer",
  "counterexampleCid": null,
  "realizer": {
    "name": "provekit-ts-zod-witnesser",
    "version": "0.1.0",
    "kit": "typescript"
  },
  "diagnostics": []
}
```

Refusals are first-class. A refusal may be signed and content-addressed when policy wants durable negative evidence, but a refusal never discharges an obligation.

## Section 5. Composition law

The common abstraction:

```
Realizer : Obligation * HostContext -> RealizerOutput
```

Mode-specific functions:

```
attest    : Obligation * HostContext -> WitnessResult | RefusalResult
transform : Gap * HostArtifact -> TransformCandidate | RefusalResult
lift      : HostArtifact -> ProofIRDocument | LiftRefusal
verify    : Edge * ProofIRDocument * Policy -> WitnessResult | RefusalResult
```

Normative composition for droppers:

```
closeByTransform(gap, artifact, policy):
  proofPlan = planProof(gap, policy) | omitted-by-policy
  projection = project(proofPlan, artifact) | omitted-by-policy
  candidate = transform(gap, artifact, proofPlan, projection)
  lifted    = lift(candidate.transformedArtifact)
  witness   = verify(gap.requiredEdge, lifted, policy)
  return TransformResult(status="closed", closureWitnessCid=witness.cid)
```

If any required step fails, the transform is not accepted. A transform
candidate without post-lift closure is only a candidate. Whether
`proofPlan` and `projection` are required is policy-selectable, but
proof-first realization is the preferred ORP posture.

The same checker bytecode MAY participate in multiple compositions:

```
attest mode:
  checker bytecode + runtime value -> WitnessResult

transform mode:
  checker bytecode inserted at boundary -> post-lift closure witness

monitor mode:
  checker bytecode attached to boundary -> witness stream
```

The bytecode may be shared. The authority differs by placement.

## Section 6. RPC surface (optional v0 transport)

ORP can be implemented as an in-process kit API or as a JSON-RPC plugin. When exposed over RPC, the protocol version is:

```
provekit-orp/1
```

### Section 6.1 `initialize`

Request:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "client":{"name":"provekit-cli","version":"v1.5.0"},
  "protocol_version":"provekit-orp/1",
  "workspace_root":"/abs/path/to/workspace",
  "catalog_cid":"blake3-512:..."
}}
```

Response:

```json
{"jsonrpc":"2.0","id":1,"result":{
  "name":"provekit-rust-realizers",
  "version":"0.1.0",
  "protocol_version":"provekit-orp/1",
  "capabilities":{
    "kits":["rust"],
    "modes":["attest","transform"],
    "obligationKinds":["predicate","edge","gap"],
    "predicates":["not_null"],
    "checkerTargets":["native-rust"]
  }
}}
```

### Section 6.2 `realize`

Request:

```json
{"jsonrpc":"2.0","id":2,"method":"realize","params":{
  "plan": { "kind":"RealizerPlan", "...":"..." }
}}
```

Response:

```json
{"jsonrpc":"2.0","id":2,"result":{
  "output": { "kind":"RealizerOutput", "...":"..." }
}}
```

### Section 6.3 `shutdown`

Same lifecycle as other ProvekIt plugin protocols: complete in-flight requests, respond with `null`, then exit cleanly.

## Section 7. Trust rules

1. **Witnessers are accepted only through evidence.** A witnesser's claim is valid only if its evidence verifies under the accepted policy.

2. **Droppers are never trusted directly.** A dropper output is accepted only after re-lift and closure witness. The patch itself is not a proof.

3. **Proofless transform is degraded evidence unless policy says otherwise.** A policy may allow direct candidate generation without a `ProofPlan`, but the output must not claim proof-first evidentiary strength.

4. **Language droppers project; they do not certify.** A `LanguageDropperProjection` states how a proof plan maps into a host-language shape. Acceptance still requires re-lift and closure witness.

5. **Checker bytecode is not application bytecode.** Compiled ProofIR checkers witness predicates; they do not implement the application behavior the predicates constrain.

6. **Policy is explicit.** Every plan names a `policyCid`. A realizer may support a predicate and still fail under a stricter policy.

7. **Binding maps are part of the claim.** A witness over the wrong host value is invalid even if the predicate itself is true elsewhere.

8. **Mutation authority is mode-scoped.** `attest` mode must not modify the host context. `transform` mode may modify only declared artifacts. `monitor` mode may attach only at declared attachment points.

9. **Refusals fail closed.** Unsupported predicates, unsupported host contexts, invalid bindings, missing policy, and non-deterministic checker failure all return `RefusalResult`.

## Section 8. Kit responsibilities

A conformant ORP kit:

1. Declares supported modes, host context kinds, predicate families, edge shapes, and checker targets.
2. Accepts `RealizerPlan` inputs and returns exactly one `RealizerOutput`.
3. Produces JCS-canonical signed output artifacts when signing is requested by policy.
4. Includes enough provenance to identify the host artifact, realizer version, binding map, policy, and input CIDs.
5. Refuses unsupported obligations explicitly.
6. For `transform` mode, ensures accepted outputs include post-lift closure evidence.
7. When emitting `proofPlanCid`, makes the referenced `ProofPlan` available by CID or declares it as an external dependency.
8. When emitting `languageDropperCid`, makes the referenced `LanguageDropperProjection` available by CID and ensures it points at the same proof plan named by the transform.
9. For checker bytecode targets, records the compiler CID, target runtime, predicate CID, and binding ABI.

## Section 9. Worked examples

### Section 9.1 Rust not-null dropper as ORP transform

The current Rust dropper in `implementations/rust/provekit-walk/src/dropper/` is an ORP `transform` realizer.

Input gap:

```
maybe_null(x) -> not_null(x)
```

Host artifact:

```rust
fn caller(x: Option<i32>) {
    f(x);
}
```

Transform candidate:

```rust
fn caller(x: Option<i32>) {
    if x.is_none() { panic!("not_null: x must be Some"); }
    f(x);
}
```

Acceptance requires re-lift:

```
modified Rust source -> lifted ProofIR -> closure witness for not_null(x)
```

No patch-only output is accepted as proof.

### Section 9.2 TypeScript Zod witnesser as ORP attest

ProofIR predicate:

```
integer(amount) and 1 <= amount and amount <= 10000
```

Host context:

```ts
const TransferRequest = z.object({
  amount: z.number().int().min(1).max(10000)
});
```

The witnesser lifts or interprets the Zod validator, binds `amount` to the ProofIR variable, and emits evidence that the host surface witnesses the predicate. If Zod coercion or number semantics differ from the ProofIR predicate under policy, the witnesser refuses.

### Section 9.3 Same checker, different placement

ProofIR predicate:

```
1 <= amount <= 10000
```

Compiled checker bytecode:

```
check_amount(amount) -> true | false
```

Use as witnesser:

```
check_amount(runtime_value) -> WitnessResult
```

Use as dropper payload:

```
insert check_amount at API boundary -> re-lift -> closure witness
```

Use as monitor:

```
attach check_amount to gateway -> witness stream
```

The bytes may be the same. The ORP mode determines authority.

### Section 9.4 Proof plan plus Java language dropper

Gap:

```
maybe_null(name) -> non_null(name)
```

Proof plan:

```
violation condition: maybe_null(name) and not(non_null(name))
objective: unsat(violation condition after realization)
eliminator: strengthen-precondition(non_null(name))
```

Language dropper projection:

```java
@Requires("name != null")
public String lookup(String name) {
    return "user:" + name.toUpperCase();
}
```

The projection is not accepted because it is Java, because it resembles a
human fix, or because the dropper emitted it. It is accepted only when
the Java lifter reads the transformed artifact back into ProofIR and the
post-lift graph closes the named edge under policy.

The receipt chain is:

```
ProofPlan CID
  -> LanguageDropperProjection CID
  -> transformed artifact CID
  -> postLift CID
  -> closureWitness CID
  -> FixReceipt CID
```

## Section 10. Non-goals

- Compile ProofIR into application implementations.
- Replace lifters. ORP composes with lifters; it does not subsume them.
- Trust LLM-generated or dropper-generated code by origin.
- Define every host-language binding ABI in v0.1.
- Standardize one checker bytecode target. CBP may define checker-bytecode carriers and encodings, while ORP remains target-agnostic.
- Require every predicate to be runtime-witnessable. Some obligations remain static, solver-only, or policy-only.

## Section 11. Open questions

1. Should `RealizerPlan` itself be a signed memento, or only the output?
2. Should ORP normatively depend on CBP's dedicated `CheckerMemento`, or keep checker bytecode as an optional target-specific realization detail?
3. Should `monitor` mode be deferred entirely until a runtime evidence-stream spec exists?
4. Should ORP be cataloged under one property key (`obligation-realizer-protocol`) or split into per-mode keys (`witnesser-protocol`, `dropper-protocol`, `monitor-protocol`)?
5. Should refusal mementos be admitted into the substrate as negative/adversarial witnesses?

## Section 12. Conformance

An ORP v0.1 implementation is conformant if it:

1. Implements at least one mode: `attest` or `transform`.
2. Uses the `RealizerPlan` and `RealizerOutput` shapes in Section 4.
3. Fails closed with `RefusalResult` on unsupported obligations.
4. Does not treat transform output as evidence until re-lift closure is verified.
5. Includes policy CID, binding map, realizer identity, and relevant input CIDs in every output.
6. Produces byte-stable JCS output for byte-equal plans and byte-equal host artifacts, except where the output explicitly records nondeterministic generation provenance.

## Section 13. Citation

Cite as:

> ProvekIt Protocol Working Notes (2026). *Obligation Realizer Protocol (ORP)*. Draft protocol spec v0.1.0.
