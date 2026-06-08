# Extension Protocols

**Status:** v0.1.0 draft doctrine
**Date:** 2026-05-06
**Layer:** protocol doctrine over the ProofIR/memento substrate
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` - envelope/header/metadata layering and the body-only extension cut
- `2026-05-03-contract-set-extension.md` - example extension protocol carried as a signed metadata convention
- `2026-04-30-ir-extension-protocol.md` - IR-name extension mechanism
- `2026-05-06-obligation-realizer-protocol.md` - ORP as an extension protocol over realization artifacts
- `2026-05-06-truth-discharge-protocol.md` - unit truth over signed body-claims
- `2026-05-06-grammar-conformance-protocol.md` - formal grammars and ProofIR invariants for extension bodies
- `2026-04-30-protocol-catalog-format.md` - catalog shape for pinning protocol specs

## Section 0. Purpose

This spec defines what Sugar means by **extension protocol**.

An extension protocol is an optional protocol layered on top of the complete core substrate. It standardizes a workflow, signed metadata convention, executable metadata bytecode, body-field convention, producer interface, consumer interpretation, or artifact family without adding a new core primitive.

Important terminology: in the v1.4 layered shape, the `metadata` field is the body. It is part of the signed letter and part of the memento CID. "Extension metadata" means "opaque to the core verifier," not "outside the envelope" and not "unsigned."

Extension metadata can literally be executable bytes for an extension interpreter. To the core substrate, those bytes are signed, content-addressed payload. To extension-aware tooling, those same bytes are protocol instructions. They are not comments, hints, or decoration.

The doctrine:

```
Core protocol is complete enough.
Extension protocols compose on top.
Core stays finite.
Extension execution may be Turing complete.
```

Extension protocols are how ecosystems build new behavior without growing the substrate.

## Section 1. Core versus extension

The **core protocol** defines the substrate primitives:

- canonical ProofIR claims;
- content addressing by CID;
- signed mementos;
- proof bundles;
- envelope/header/body layering;
- verification of signatures, hashes, references, and declared memento semantics.

An **extension protocol** defines an interoperable convention above those primitives:

- a signed `metadata`/body convention;
- executable extension bytecode or instruction streams inside signed metadata/body;
- a producer/consumer workflow;
- an optional plugin or RPC surface;
- a derived view over existing mementos;
- a checker, witness, dropper, monitor, resolver, catalog, bridge, or policy interpretation;
- any other ecosystem convention whose artifacts ultimately reduce to existing signed, content-addressed claims.

An extension protocol MUST NOT require a core verifier to learn a new primitive in order to verify the underlying substrate artifacts.

## Section 2. Completeness principle

Extension protocols prove the core is complete; they do not patch holes in it.

The test:

```
If a consumer does not understand extension protocol X,
can it still verify the signatures, CIDs, and core memento semantics
of the artifacts X references or emits?
```

If yes, X is a valid extension protocol.

If no, X is attempting to extend the core substrate and must either be rejected or promoted through a core protocol versioning process.

## Section 3. Signed metadata/body rule

Per `2026-05-03-substrate-layers-envelope-header-body.md`, extension protocols are body-only unless explicitly versioned as a core change.

The extension surface is:

- `body` fields;
- `metadata` fields, where `metadata` is the body object in the layered envelope;
- references to other content-addressed artifacts;
- signed mementos that already have a core-recognized kind;
- derived views over existing mementos;
- plugin/RPC conventions that produce the above.

These fields are still part of the canonical signed bytes. They affect the attestation CID. The core verifier verifies the envelope signature and CID over them; it simply does not interpret their extension-specific semantics.

The extension surface is not:

- new envelope fields;
- changes to signature semantics;
- changes to CID derivation;
- changes to canonicalization;
- changes to core verifier acceptance rules;
- unversioned changes to header fields that affect content identity.

Header changes are allowed only when the memento kind's own core spec declares the header field and versioning rule. Extension protocols should assume they live in signed metadata/body fields and references.

## Section 4. Extension execution model

An extension protocol MAY define executable metadata/body bytes.

The execution model is:

```
signed metadata/body bytes + memento DAG + extension interpreter -> extension result
```

The core substrate does not execute these bytes. It signs, hashes, stores, resolves, and verifies the mementos that contain them. Extension-aware tooling executes them according to the extension protocol's interpreter semantics.

### Section 4.1 DAG ordering

Extension execution is ordered by the memento DAG.

Let `G = (N, E)` be the reachable memento graph for an extension evaluation, where each node is a memento CID and each edge points from a memento to a referenced input CID. The extension interpreter MUST evaluate dependencies before dependents:

```
if memento A references memento B, execute B before A
```

Independent nodes MAY be evaluated in parallel. If an extension needs deterministic sequential execution across independent nodes, it MUST use a deterministic tie-breaker. The default tie-breaker is lexicographic CID order.

`inputCids` ordering is a canonicalization rule, not by itself semantic instruction order. Semantic order comes from the DAG plus any explicit ordering fields defined by the extension protocol. If an extension defines explicit instruction order inside a single metadata/body object, that order is local to that object and still executes only after the object's DAG dependencies are available.

### Section 4.2 Reified subgraphs and DAG roots

Extension protocols build DAGs whose nodes may themselves name other DAGs.

The doctrine:

```
Every relied-on step can be a CID.
Every relied-on dependency is an edge.
Every obligation edge is p -> q.
Every subgraph can be reified by its root CID.
```

This is not metaphor. It is the substrate rule for compositional evidence. If an extension asks a consumer to rely on a source file, compiler, flag set, dependency bundle, interpreter, policy, bytecode stream, execution trace, witness proof, or derived claim, that thing SHOULD be represented by content-addressed bytes and referred to by CID. If a result depends on another artifact, that dependency MUST be represented as an edge from the dependent artifact to the prerequisite CID.

At the obligation layer, an implication edge has the shape:

```
p -> q
```

where `p` and `q` are content-addressed predicates, facts, states, or claims. The edge itself is also content-addressed. A higher-arity claim is represented by a CID-bearing object whose body names its input CIDs and output CID, so the multi-input case still reduces to an edge-bearing node in the memento DAG.

This gives Sugar DAGs of DAGs:

```
source DAG
  -> compilation DAG
    -> bytecode DAG
      -> execution DAG
        -> witness DAG
          -> claim DAG
```

Each nested DAG may be collapsed to a root CID and used as a value in a parent memento. Extension-aware tooling may expand the child DAG under policy. Core verification verifies only the signed bytes, CIDs, references, and core memento/header validity rules.

**Lemma (Reified Subgraph / DAG-of-DAGs).** If a signed memento contains or references a well-formed extension DAG by content-addressed root CID, then the entire extension DAG may be treated as a single value in the parent memento DAG without requiring core verification to execute, traverse semantically, or understand the extension DAG.

**Proof sketch.** The parent memento's core identity is computed over canonical signed bytes that include the child root CID. Core verification checks the parent's CID, signature, and references. It does not need the child extension semantics to know that the parent refers to the child root. An extension-aware consumer may recursively expand the child root, verify its internal CIDs and signatures, execute any extension bytecode under policy, and emit a result as another signed/content-addressed memento. That result can then be used as another root value in another parent graph. QED.

**Reliance rule.** A parent claim SHOULD reference the strongest already-witnessed root it means to rely on.

Do not inline the world. Reference the witnessed root of the world you mean.

Raw artifact CIDs establish byte identity. Proof CIDs establish proof-byte identity. Witness CIDs establish that an accepted checker, interpreter, signer, and policy evaluated those bytes and emitted a result. Therefore a parent that relies on a property SHOULD reference the witness root that binds the property, not only the raw artifacts beneath it.

Example:

```
evmBytecodeCid
proofCid
proofCheckerCid
policyCid
obligationCid
  -> proofAcceptanceWitnessCid

parentClaimCid
  -> proofAcceptanceWitnessCid
```

The parent claim does not need to include the EVM bytecode directly if the accepted witness root already commits to it. The EVM bytecode still exists in the child DAG. The parent references the claim it relies on.

**Truth-discharge rule.** A positive witness discharges exactly one thing:

```
the claim named by this witness body is true under the cited policy
```

The truth value is intentionally small. The body carries the claim being discharged: obligation CID, artifact CIDs, checker/interpreter CIDs, policy CID, input roots, binding rules, and any proof bytes or proof CIDs. The positive witness result is the unit discharge over that body.

So the reusable shape is:

```
body = { claim, inputs, checker, policy, bindings, proof/artifact refs }
accepted execution over body -> true
```

A witness does not make every object in its body globally trusted. It makes the body-claim true under the policy that accepted the witness. Parent claims therefore reference the witness root when they rely on that truth, and reference lower artifact CIDs only when they rely on byte identity or availability.

`2026-05-06-truth-discharge-protocol.md` standardizes this rule as an extension protocol.

### Section 4.3 Fail-closed execution

An extension interpreter MUST fail closed when:

1. a referenced memento is unavailable;
2. a referenced memento fails signature or CID verification;
3. the extension bytecode/instruction stream is malformed;
4. the extension bytecode references an unsupported opcode or protocol version;
5. the interpreter cannot construct a valid topological evaluation order;
6. extension-specific policy rejects the execution.

The failure does not make the underlying mementos invalid as signed bytes. It means the extension result is unavailable.

### Section 4.4 Finite core, executable extension layer

The core substrate is deliberately finite:

- parse canonical bytes;
- compute CIDs;
- verify signatures;
- resolve references;
- apply core memento/header validity rules;
- fail closed.

The extension layer may be Turing complete. An extension protocol may define bytecode, interpreters, recursive DAG walks, generated witnessers, generated droppers, monitors, proof search, or any other executable process expressible in signed metadata/body bytes and referenced artifacts.

This creates a hard rule:

```
Core verification MUST NOT depend on extension execution terminating.
```

Extension execution is an optional computation over already-verifiable substrate bytes. If the computation terminates and emits a signed/content-addressed result, that result can enter the substrate as another memento. If it does not terminate, exceeds fuel, violates sandbox policy, or cannot be interpreted, the extension result is absent. Absence of an extension result is refusal, not partial truth.

Operational extension interpreters SHOULD be sandboxed and fuel/time bounded. Policy decides which interpreters, bytecode CIDs, signer keys, fuel limits, and result mementos are acceptable.

The substrate is not Turing complete. The extension layer may be.

**Lemma (Non-Executing Core / Turing-Complete Extension).** If core verification is defined only over canonical bytes, CIDs, signatures, references, and core header validity rules, and if extension bytecode execution is excluded from core verification, then extension protocols may be Turing complete without making substrate verification Turing complete.

**Proof sketch.** Core verification terminates because it never invokes the extension interpreter. It verifies the signed byte graph: parse, hash, signature-check, reference-resolve, and apply finite core validity rules. Extension execution is a separate computation over already-verifiable bytes. If the extension computation terminates and emits a signed/content-addressed result, that result becomes another memento in the graph. If it does not terminate, the extension result is absent. In neither case does core verification wait on, depend on, or inherit the halting behavior of the extension interpreter. Therefore Turing-complete extension protocols do not make substrate verification Turing complete. QED.

This differs from architectures where script or VM execution is inside the mandatory validation path. In those systems, the validator must execute the script to decide whether the artifact is valid, so VM power directly affects validation liveness and consensus safety. In Sugar, the core validates the signed byte graph; extension interpreters compute optional results over that graph. The extension language may be powerful because no consumer is forced to execute it to validate the substrate.

### Section 4.5 Reflective self-witnessing

Because specs, interpreters, witnessers, checker bytecode, implementations, and conformance claims are all content-addressable artifacts, Sugar can carry witnesses about its own conformance.

Example shape:

```
spec bytes S                  -> spec CID
implementation bytes I        -> implementation CID
checker/interpreter bytes C   -> checker CID
claim: I conforms to S under C
  -> signed witness memento
```

This is self-witnessing, not proof from nowhere. A consumer still relies on a base kernel:

- canonical byte parsing;
- CID computation;
- signature verification;
- policy selection;
- accepted interpreter/checker CIDs;
- accepted signer keys.

Above that base kernel, the substrate can host reflective evidence:

```
Sugar artifacts witnessing Sugar conformance claims.
```

The protocol can therefore carry its own self-correctness evidence without making core verification circular. Core verification checks the signed bytes and references. Extension-aware tooling interprets the self-witness under policy. The two layers must not collapse.

`2026-05-06-grammar-conformance-protocol.md` formalizes this as the Stratified Self-Conformance theorem: extension protocols may publish grammars and ProofIR invariants for their own bodies, then receive TDP witnesses over those conformance claims, while the core verifier remains limited to signed bytes, CIDs, references, and finite header rules.

## Section 5. Graceful degradation

Extension protocols MUST degrade gracefully.

A consumer that does not understand an extension protocol:

1. MAY ignore the extension-specific metadata/body fields semantically.
2. MUST still be able to verify the core memento envelope, signature, CID, and references.
3. MUST NOT silently treat extension-specific claims as verified core semantics.
4. SHOULD fail closed when asked to rely on extension semantics it cannot interpret.

Ignoring extension semantics does not mean ignoring bytes. The metadata/body remains part of the signed object and the CID. A non-aware consumer verifies the letter; it merely declines to interpret that paragraph.

Example:

```
Consumer understands signed mementos, but not ORP.
It can verify that an ORP TransformResult is signed and content-addressed.
It cannot conclude that the transform closed a gap unless it also verifies
the referenced post-lift closure witness under its policy.
```

## Section 6. Cataloging extension protocols

An extension protocol MAY be cataloged.

Cataloging an extension protocol:

- pins the spec bytes by CID;
- gives the extension a stable property key;
- lets producers declare protocol conformance by CID;
- lets consumers select compatible tooling;
- does not make the extension part of the core verifier.

Cataloging is therefore:

```
normative reference, not runtime activation
```

A cataloged extension protocol SHOULD use a property key ending in `-protocol`, e.g.:

```
obligation-realizer-protocol
contract-set-extension-protocol
some-future-monitor-protocol
```

If an extension protocol later becomes load-bearing for core verification, it MUST move through a core versioning process rather than silently relying on its catalog entry.

## Section 7. Conformance language

When a tool says:

```
implements extension protocol X
```

it means:

1. the tool understands X's workflow and data shapes;
2. the tool can produce or consume X-specific signed metadata/body conventions;
3. the tool can verify X-specific references according to X's rules;
4. the tool does not require non-X consumers to change core verification behavior.

It does not mean:

1. X is required to verify all substrate artifacts;
2. X has added a new core primitive;
3. core consumers must accept X-specific semantics;
4. X-specific claims are trusted without the signatures, CIDs, witnesses, and policies the core substrate already requires.

## Section 8. Examples

### Section 8.1 Contract Set Extension

The Contract Set Extension defines semver/version-chain semantics for groups of contracts. Its claims live in signed metadata/body fields and references. A consumer that ignores the extension can still verify the underlying signed mementos and their CIDs. A consumer that understands the extension can additionally interpret version history, compatibility, and derived contract-set views.

This is a valid extension protocol.

### Section 8.2 Obligation Realizer Protocol (ORP)

ORP defines a workflow for host-space realizers:

```
Witnesser: show me it holds.
Dropper:   make it hold, then show me.
Monitor:   keep showing me while it runs.
```

ORP uses existing substrate artifacts: ProofIR obligations, CIDs, signed mementos, policies, lift outputs, and closure witnesses. It standardizes how tools produce and interpret realization plans/results.

A consumer that does not understand ORP can still verify the referenced mementos and CIDs. It simply cannot interpret ORP's workflow-level claim unless it implements ORP.

This is a valid extension protocol.

### Section 8.3 Grammar Conformance Protocol (GCP)

GCP defines how an extension body is witnessed as conforming to a formal grammar and optional ProofIR invariant set:

```
grammar + subject body + invariant set + parser + policy -> conformance witness
```

GCP is an extension because:

- the grammar and invariant set are content-addressed artifacts;
- the subject body remains ordinary signed metadata/body;
- non-aware consumers can still verify the body bytes, signature, CID, and references;
- core verification does not run the parser or invariant checker.

This is a valid extension protocol.

### Section 8.4 Truth Discharge Protocol (TDP)

TDP defines a reusable positive witness shape:

```
body-claim + verifier + policy + evidence root -> true
```

TDP is an extension because:

- the body-claim is signed/content-addressed;
- the witness is a signed/content-addressed memento;
- non-aware consumers can still verify the witness bytes, signature, CID, and references;
- core verification does not run the verifier that produced the truth result.

This is a valid extension protocol.

### Section 8.5 Checker bytecode targets

The Checker Bytecode Protocol (CBP) defines how ProofIR predicates compile to boundary-check bytecode for WASM, eBPF, JVM bytecode, native plugins, interpreted host predicates, or Sugar-native instruction streams.

CBP is an extension because:

- the checker bytecode is embedded in signed metadata/body or referenced by CID;
- evidence emitted by the checker is signed or otherwise verifiable under existing memento rules;
- non-aware consumers can still verify the referenced artifacts;
- the checker does not become required for core memento verification.

If the checker bytecode target changes core predicate semantics, it is not an extension; it is a core protocol change.

## Section 9. Extension protocol checklist

A proposed extension protocol should answer:

1. What core artifacts does it read?
2. What core artifacts does it emit or reference?
3. Which fields are signed metadata/body conventions?
4. What does a non-aware consumer still verify?
5. What must an aware consumer additionally interpret?
6. What is the fail-closed behavior?
7. Does the extension require any new envelope/header/signature/CID semantics?
8. Does the extension execute metadata/body bytes?
9. If yes, what is the interpreter, fuel/sandbox policy, and DAG evaluation rule?
10. What exact body-claim does a positive witness discharge?
11. Which root CID should parent claims reference when relying on this extension's result?
12. Can non-termination or interpreter refusal affect core verification? (Required answer: no.)
13. What is the catalog property key, if cataloged?
14. What is the migration path if the extension later needs core support?

Any "yes" answer to question 7 requires core protocol review.

## Section 10. Non-goals

- Define every extension protocol.
- Make extension protocols mandatory for core verification.
- Provide a plugin transport for every extension.
- Collapse all extension protocols into one universal extension surface.
- Let extension protocols bypass policy, signatures, CIDs, or witness verification.

## Section 11. Open questions

1. Should the protocol catalog distinguish core protocols from extension protocols with an explicit `layer` field?
2. Should draft extension protocols be cataloged, or only stable ones?
3. Should extension protocol conformance mementos have a standard shape?
4. Should consumers advertise supported extension protocols in bundle verification output?

## Section 12. Citation

Cite as:

> Sugar Protocol Working Notes (2026). *Extension Protocols*. Draft doctrine v0.1.0.
