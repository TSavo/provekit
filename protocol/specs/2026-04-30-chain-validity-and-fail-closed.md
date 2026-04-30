# ProvekIt: chain validity and the fail-closed gate

> Author: shared session 2026-04-30 (T + Claude). The protocol-level
> definition of what makes a `proofHash` chain VALID, and the gate that
> rejects every chain that fails to prove its own validity. This spec
> sits at the top of the memento, signature, and canonicalization specs.
> Verifier conformance is defined here.

## Status

This document is **AUTHORITATIVE**. Where the current TypeScript runtime
in `src/fix/runtime/` diverges from this spec, the runtime is wrong and
must be aligned. Implementation-language details are non-normative; the
chain-validity rules are normative.

Today's runtime (`src/fix/runtime/verify.ts`,
`src/fix/runtime/verifyCache.ts`, `src/fix/runtime/mementoStore.ts`)
implements per-invariant decay detection and Z3 path checking but does
not implement signature verification, catalog/bridge resolution, or the
chain-level fail-closed gate this spec defines. That gap is a known
drift; see "Drift from current implementation" at the end.

## Companion specs

This spec depends on, and references by name:

- `protocol/specs/2026-04-29-the-semantic-envelope.md`: why semantic-layer
  composition exists at all.
- `protocol/specs/2026-04-29-supply-chain-via-semantic-envelope.md`: the
  attack classes the fail-closed gate must detect.
- `protocol/specs/2026-04-29-stages-vs-actions.md`: why audit mementos
  exist alongside claim mementos.
- `protocol/specs/2026-04-29-universal-claim-envelope.md`: wrapper schema,
  CID construction, and the v1 producer-signature scheme. This spec
  references the signature scheme defined in the
  "Producer-signature scheme (v1)" section of that document. A
  standalone signatures-and-non-repudiation spec is anticipated; when
  it lands, it will need a back-reference to this spec for chain-level
  signature gating semantics.
- `protocol/specs/2026-04-29-correctness-is-a-hash.md`: the leaves vs
  roots boundary, and the explicit non-shipping of a deep walker.
- `protocol/specs/2026-04-29-attack-surfaces.md`: adversarial taxonomy.
- `protocol/specs/2026-04-30-ir-formal-grammar.md`: IR canonical form,
  required for IR-CID equality checks.
- `protocol/specs/2026-04-29-ast-canonicalizer.md`: AST canonicalization.

## Terminology

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**,
**SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**,
and **OPTIONAL** in this document are to be interpreted as described
in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in
all capitals.

Additional terms used throughout:

- **CID**: content identifier; a content-addressed hash as defined by
  the universal-claim-envelope spec. A CID identifies a memento by the
  bytes of its canonicalized envelope.
- **Memento**: a single content-addressed envelope conforming to the
  universal-claim-envelope schema, with a `kind` field selected from
  the kinds enumerated in this spec or a sibling spec.
- **proofHash**: a CID over a **catalog** memento. The proofHash IS
  the catalog memento's CID; the term `proofHash` is the protocol-
  facing name for this CID when it appears as a claim by a binary, a
  package, or a release.
- **Verifier**: software that consumes a proofHash, resolves it
  against a local memento store, and produces a validity report under
  the rules of this spec. The verifier is the gate.
- **Local store**: the verifier's content-addressed store of mementos
  it has fetched, locally minted, or imported. Resolution is local;
  the verifier MUST NOT walk over the network during chain validity
  checks (see R12 below).
- **External root**: a CID referenced by a memento in the chain that
  resolves to bytes whose content-type or schema the verifier is not
  prepared to verify mechanically. A common case: an Apple
  notarization signature CID, a kernel.org tarball signature CID, a
  sigstore Rekor entry CID. External roots are accepted under explicit
  per-verifier policy or rejected; the protocol has no universal rule
  for which external roots to trust.
- **Stage memento** vs **audit memento**: per
  `2026-04-29-stages-vs-actions.md`, a Stage memento is a cacheable
  claim and a member of the proof DAG; an audit memento is a
  side-effecting action's invocation record and is NOT a member of the
  proof DAG. Chain validity walks ONLY Stage mementos. Audit mementos
  are out of scope for this spec.

## 1. The chain

### 1.1 Definition

A **proofHash chain** is a finite, directed, acyclic graph G = (N, E)
where:

- N is a set of mementos, each identified by its CID.
- E is a set of typed edges; an edge `(u, v, f)` exists if memento `u`
  references memento `v`'s CID via field `f` of `u`'s envelope.
- G has a single root memento `r`, of kind `catalog`, whose CID is the
  proofHash being verified.
- Every node `n ∈ N` is reachable from `r` by following edges.

### 1.2 Edge sources

The reference fields that produce edges are exactly those enumerated
in §3 of this spec. A field of a memento that holds a CID and is
listed in §3 produces an edge; a field that does not hold a CID, or
is not enumerated, does not produce an edge. This is closed: a
verifier MUST NOT infer edges from unspecified fields, even if those
fields contain hex strings.

The `inputCids` array in the universal-claim-envelope wrapper produces
edges of type **provenance** (see R8 below). All other edges are
type-specific and arise from variant-body fields enumerated per kind.

### 1.3 Memento kinds in scope

This spec recognizes the following memento kinds as participating in
chain validity. Each is defined in §3 with its required fields and
edge invariants. Producers MAY emit additional kinds as new evidence
variants per the universal-claim-envelope extension model; this spec
governs only the kinds listed below.

- `catalog`: root of a chain; declares a set of property bindings.
- `property`: an invariant with its IR canonical form and verdict.
- `verdict`: a producer's check result on a property's IR.
- `bridge`: a consumer-side edge to a property in another catalog.
- `external-root`: an opaque-to-the-verifier CID with provenance
  metadata, accepted only under policy.
- `producer-public-key` and `producer-key-rotation`,
  `producer-key-revocation`: producer identity mementos referenced
  during signature verification (per the universal-claim-envelope
  v1 producer-signature scheme).

The standard evidence variants from
`2026-04-29-universal-claim-envelope.md` (`z3-model`, `z3-unsat`,
`pattern-match`, `type-check-pass`, `lint-pass`, `test-pass`,
`test-fail`, `llm-proposal`, `mutation-witness`, `workflow-run`,
`legacy-witness`, `action-invocation`) are the **evidence body**
inside `verdict` mementos and other Stage mementos. They are not
separate top-level kinds for chain-validity purposes; the kind for
chain validity is `verdict`, and the evidence variant is interpreted
at the verdict's evidence layer.

### 1.4 Acyclicity

Chains are DAGs by construction. Every edge points to a CID; a CID is
the hash of its memento's canonical bytes; therefore a memento CANNOT
reference itself or any descendant without a hash collision. A
verifier MUST treat an apparent cycle as evidence of corruption or
collision and REJECT (see R10). Acyclicity is therefore an invariant,
not a check the verifier needs to enforce by graph algorithm in the
absence of a hash break.

## 2. Reachability

### 2.1 The reachability rule

A chain G is **reachable** for a verifier V iff for every memento
`n ∈ N`, V can resolve `n`'s CID against its local store and obtain
bytes that hash to that CID. This is purely local: V MUST NOT perform
network resolution as part of chain validity. The expectation is that
the verifier's local store has been pre-populated through one of the
distribution mechanisms (package manager fetch, registry sync, swarm
distribution); chain validity is the gate that runs AFTER fetch.

### 2.2 The non-walker discipline

Per `2026-04-29-correctness-is-a-hash.md`: ProvekIt does NOT ship a
deep walker. The chain G as defined in §1.1 is rooted at the
catalog memento under verification and extends only to mementos the
verifier can resolve LOCALLY. The chain's "leaves" from the
verifier's perspective are external roots (§7) that the verifier
does not attempt to resolve further.

Conformance does NOT require recursing through every transitive
dependency's catalog. Conformance requires resolving every CID
referenced by mementos the verifier IS prepared to interpret. A
verifier choosing to interpret a transitive catalog (i.e., walk a
bridge into another package's catalog) MUST apply the full chain
validity rules to that catalog as well.

### 2.3 Verifier resolution policy

The verifier's local store is a content-addressed key/value mapping
from CID to canonical bytes. Resolution is:

```
resolve(cid: CID) -> Bytes | UNRESOLVED
```

A verifier MUST treat UNRESOLVED for any reachable required CID as a
hard reject (see R1).

A verifier MAY mark certain CIDs as **external root** and apply the
external-root rule (see §7) instead of UNRESOLVED. The verifier's
policy declares which CIDs are external roots; the protocol does
not.

## 3. Per-edge validity

This section defines, for every edge type, the source field, the
target memento kind, and the additional invariants the verifier MUST
check at the edge.

### 3.1 catalog → property (edge: `catalog.properties[i]`)

A `catalog` memento has the following minimum required body:

```yaml
kind: catalog
schema: <CID of the catalog schema>
body:
  catalogVersion: integer       # MUST be 1 for this spec
  catalogName: string           # human-readable; not load-bearing
  properties: [CID]             # CIDs of property mementos
  retiredProperties: [CID]      # OPTIONAL; CIDs of properties this
                                # catalog explicitly retires from a
                                # prior catalog generation
  toolchain: [CID]              # OPTIONAL; CIDs of producer-public-key
                                # mementos that this catalog vouches
                                # for (the kit's pinned solver, lifter,
                                # type checker)
```

For each `cid` in `body.properties`:

- The verifier MUST resolve `cid` (see §2.1).
- The resolved memento MUST have `kind: property` (see §3.2).
- The verifier MUST verify the property memento's signature per §4.

For each `cid` in `body.retiredProperties` (when present):

- The verifier MUST resolve `cid` to a `kind: property` memento.
- The verifier MUST NOT include retired properties in the catalog's
  active property set; they exist only to make retirement visible
  to consumers comparing catalog generations.

For each `cid` in `body.toolchain` (when present):

- The verifier MUST resolve `cid` to a `kind: producer-public-key`
  memento.
- The verifier MUST use the keys in `toolchain` as the authoritative
  set of trusted producers for signatures within this catalog's
  property and verdict mementos. Signatures from producers not in
  `toolchain`, or for which no `producer-public-key` memento is
  resolvable, are subject to R3.

### 3.2 property → IR (edge: `property.irCid`)

A `property` memento has the following minimum required body:

```yaml
kind: property
schema: <CID of the property schema>
body:
  irCid: CID                    # CID of the canonical IR formula
  irLanguage: string            # e.g. "smt-lib2", "ts-ir@1"
  scope: enum                   # "callsite" | "graph" | "global"
  bindings: [Binding]           # what this property is about; shape
                                # is per the host kit
  verdicts: [CID]               # CIDs of verdict mementos
  retired: optional bool        # MUST be true iff this property has
                                # been retired
  retirementCid: optional CID   # CID of a verdict or audit memento
                                # justifying retirement
```

The IR-CID edge:

- `irCid` MUST resolve to bytes that, when interpreted under the
  declared `irLanguage`, satisfy that language's canonical-form
  predicate (see `2026-04-30-ir-formal-grammar.md` for the canonical
  form of the protocol IR).
- The IR MUST be canonical: two property mementos referencing the
  same logical formula via different CIDs is a violation of
  canonicalization, not of chain validity, BUT a verifier
  encountering two mementos that claim the same `propertyHash`
  (per the wrapper schema) but reference different `irCid`s MUST
  reject (R5).

The verdict edges:

- For each `vCid` in `body.verdicts`, the verifier MUST resolve
  `vCid` to a `kind: verdict` memento (see §3.3).
- The verifier MUST verify the verdict memento's signature per §4.
- The verifier MUST check that the verdict's `body.irCid` (see §3.3)
  is bytewise equal to this property's `body.irCid`; mismatch
  triggers R6.

The retirement edge:

- If `body.retired` is true, the verifier MUST resolve
  `body.retirementCid` and confirm it is a memento (a verdict, or an
  action-invocation audit memento per the stages-vs-actions spec)
  whose evidence the verifier can interpret as a justification of
  retirement. Retired properties remain reachable so that consumers
  can still distinguish "retired by author" from "missing entirely"
  during catalog diffs.

### 3.3 verdict → IR (edge: `verdict.body.irCid`)

A `verdict` memento is a Stage memento per
`2026-04-29-stages-vs-actions.md`. Its wrapper carries the verdict
field (`holds`, `violated`, `decayed`, `undecidable`, `error`). Its
body uses one of the standard evidence variants from
`2026-04-29-universal-claim-envelope.md` (`z3-unsat`, `z3-model`,
`type-check-pass`, etc.). Above the variant body, every verdict
memento MUST carry:

```yaml
body:
  irCid: CID                    # the IR formula this verdict checks
  inputContentCids: [CID]       # the source content CIDs the verdict
                                # was computed against (per the
                                # binding scope)
  toolchainPin: optional CID    # CID of the producer-public-key
                                # memento for the producer that
                                # emitted this verdict
  evidence: <variant>           # standard variant per universal-claim-
                                # envelope §"Evidence variants"
```

(The standard wrapper still carries `producedBy`, `producedAt`,
etc.; these fields are above the variant body and are validated as
part of the wrapper, not the chain.)

The IR-CID equality check:

- `body.irCid` MUST equal the parent property memento's `body.irCid`
  bytewise. If the verifier reaches this verdict via a property
  memento, mismatch triggers R6. If the verifier reaches this verdict
  via another path (e.g., a bridge), the verifier MUST still confirm
  the IR-CID equality once the property memento is resolved.

The toolchain pin:

- If `body.toolchainPin` is present, the verifier MUST resolve it to
  a `kind: producer-public-key` memento.
- The verifier MUST verify that the verdict memento's
  `producerSignature` is valid against the public key in the pinned
  memento (per §4 and the universal-claim-envelope v1 signature
  scheme).
- If the parent catalog declares `toolchain`, the toolchain pin
  SHOULD point at a key in that set. A verdict whose toolchain pin
  is outside the catalog's `toolchain` set is subject to verifier
  policy: REJECT by default (R3); a verifier MAY accept under an
  explicit per-policy override (P1).

### 3.4 bridge → property (edge: `bridge.targetContractCid`)

A `bridge` memento is the consumer-side edge that says "at this call
site in my code, I depend on the upstream's property `<X>` being
upheld." Bridge mementos are emitted by consumer kits, not by library
authors; they are how a consumer's catalog references properties in
an upstream catalog without re-verifying.

A `bridge` memento has the following minimum required body:

```yaml
kind: bridge
schema: <CID of the bridge schema>
body:
  sourceLayer: string           # e.g. "ts@5.4", "rust@1.78", "ecma262"
  sourceSymbol: string          # the consumer-side call site identifier
  sourceBindingCid: CID         # CID of the consumer's binding memento
  targetCatalogCid: CID         # CID of the upstream catalog memento
  targetContractCid: CID        # CID of the property memento in that
                                # catalog
  targetLayer: string           # e.g. "ts@5.4", "ecma262", "v8@12.x"
  translation: optional CID     # CID of a translation-justification
                                # memento for cross-layer bridges
                                # (sourceLayer ≠ targetLayer)
```

Edge invariants:

- `targetContractCid` MUST resolve to a memento with `kind: property`.
- `targetCatalogCid` MUST resolve to a memento with `kind: catalog`.
- The catalog at `targetCatalogCid` MUST list `targetContractCid` in
  its active `body.properties` set. If the property is in
  `body.retiredProperties` and not in `body.properties`, the bridge
  has resolved against a retired contract; R7 applies.
- `sourceBindingCid` MUST resolve to a binding memento conformant
  with the consumer kit's binding schema (host-language-specific;
  not normative here).

Same-layer bridges (`sourceLayer == targetLayer`):

- The verifier MUST check that the property's `irLanguage` is
  compatible with the consumer's source layer. Compatibility is
  defined per host-language kit; the protocol-level requirement is
  that the kit declares the equivalence and the verifier confirms it.
- A same-layer bridge MUST NOT carry a `translation` field; if
  present, the verifier MUST reject (R9).

Cross-layer bridges (`sourceLayer != targetLayer`):

- The verifier MUST require `body.translation` to be present.
- `translation` MUST resolve to a memento (the protocol does not
  prescribe its kind; common cases are an `llm-proposal` evidence
  variant signed by the consumer kit's translation producer, or a
  hand-authored translation-justification memento).
- The verifier MUST validate the translation memento's signature per
  §4. The verifier MUST NOT validate the **content** of the
  translation (whether the cross-layer claim is "really" preserved);
  that is the consumer-policy decision that lives in the case-3 LLM
  workflow described in the supply-chain spec. The chain-validity
  layer checks structure and signature; semantic correctness of the
  translation is the consumer policy's job.

### 3.5 catalog → producer-public-key (via `toolchain`)

Already specified in §3.1. Restated for completeness as an edge type:

- `toolchain[i]` MUST resolve to a `kind: producer-public-key`
  memento.
- The producer-public-key memento's body, signing semantics, and
  rotation/revocation chain are defined in
  `2026-04-29-universal-claim-envelope.md`'s "Producer-signature
  scheme (v1)" section. Chain validity inherits those rules; this
  spec adds only the structural invariants for **how the keys
  participate in chain validity** (see §4).

### 3.6 inputCids edges (provenance)

Every memento's wrapper carries an `inputCids` array per
`2026-04-29-universal-claim-envelope.md`. The provenance edge type:

- For each `cid` in any reachable memento's `inputCids`, the verifier
  MUST resolve `cid` and verify the resulting memento's wrapper
  integrity (signature, CID recomputation, schema shape) per §4 and
  per the universal-claim-envelope validation rules.
- The verifier MUST NOT recursively chain-validate inputCids targets;
  inputCids are a provenance ledger, not a structural part of the
  catalog graph. Resolving the bytes and confirming wrapper integrity
  is sufficient for chain validity. (Consumers performing forensic
  walks via the audit DAG MAY do more; that is outside chain validity
  scope.)

### 3.7 audit mementos are out of scope

Per `2026-04-29-stages-vs-actions.md`, mementos with
`evidence.kind: action-invocation` are AUDIT mementos. The proof DAG
walked by chain validity MUST exclude them. A verifier that
encounters an action-invocation memento via a Stage memento's
`inputCids` MUST resolve and wrapper-validate it (see §3.6) but
MUST NOT treat it as a Stage in any of the §3 edges. If a Stage
memento's typed edge (e.g., `verdict.evidence` or
`property.verdicts[i]`) resolves to an action-invocation kind, the
verifier MUST reject (R11).

## 4. Signature gating

### 4.1 The signature requirement

A chain G is **signature-valid** for a verifier V iff every memento
`n ∈ N` that is REQUIRED-to-be-signed bears a valid signature per the
universal-claim-envelope v1 producer-signature scheme, AND the signing
key was not revoked as of `n.producedAt`.

### 4.2 Which mementos are REQUIRED-to-be-signed

The protocol distinguishes locally-trusted mementos from
swarm-distributed mementos. Per
`2026-04-29-universal-claim-envelope.md`:

> Absent signatures mean the memento is locally-trusted but not
> cryptographically attested: fine for in-process use, not sufficient
> for swarm distribution.

For chain validity:

- A memento `n` is **REQUIRED-to-be-signed** iff `n` was imported
  from outside the verifier's local minting boundary. Locally-minted
  mementos (the verifier's own kit produced them in this process or
  in a trusted peer process) MAY omit `producerSignature`; the
  verifier MAY set policy to require signatures even on
  locally-minted mementos.
- All `kind: catalog`, `kind: property`, `kind: verdict`,
  `kind: bridge`, and `kind: external-root` mementos that arrived
  via fetch (e.g., from a registry, a swarm peer, a peer kit) MUST
  be signed.
- `kind: producer-public-key` mementos are themselves signed;
  rotation and revocation chain rules per
  `2026-04-29-universal-claim-envelope.md` apply.

A verifier MUST reject (R3) when a REQUIRED-to-be-signed memento
lacks a valid signature.

### 4.3 Signing-key resolution

For each REQUIRED-to-be-signed memento `n`:

- The verifier MUST identify the candidate signing key by `n`'s
  `producedBy` field, which has the form `<name>@<version>`.
- The verifier MUST resolve a `kind: producer-public-key` memento
  whose body declares `producerName == <name>` and whose validity
  window (per its rotation chain) includes `n.producedAt`. The
  catalog's `toolchain` field is the authoritative source for which
  keys are vouched for; a producer-public-key resolved outside the
  catalog's `toolchain` is subject to R3 unless policy P1 accepts
  it.
- The verifier MUST verify `n.producerSignature` against that key
  per the universal-claim-envelope v1 scheme.

### 4.4 Revocation timing

A memento `n` signed by key `k` is signature-valid iff `k` was not
revoked as of `n.producedAt`. The verifier:

- MUST resolve any `kind: producer-key-revocation` memento for `k`
  in the local store.
- MUST treat `n` as signature-INVALID if a revocation memento exists
  whose `producedAt` is less than or equal to `n.producedAt`.
  (Equivalently: the key was already revoked when `n` was minted.)
- A revocation whose `producedAt` is strictly greater than
  `n.producedAt` does NOT invalidate `n` retroactively, UNLESS the
  revocation memento's body explicitly declares retroactive scope
  (the revocation memento's schema permits a
  `retroactiveFrom: timestamp` field; if present and less than or
  equal to `n.producedAt`, the verifier MUST treat `n` as
  signature-invalid).
- Revocation timing is checked **after** rotation resolution: if
  the relevant key is a rotated-out predecessor, validity is
  decided by the predecessor's revocation status, not the
  successor's.

### 4.5 The signature gate as a fail-closed surface

The signature gate's default is REJECT. Every of the following
triggers a hard reject without override:

- Missing `producerSignature` on a REQUIRED-to-be-signed memento (R3).
- Signature present but `producer-public-key` not resolvable (R3).
- Signature verification fails arithmetically (R3).
- Key revoked as of memento's `producedAt` (R3).
- Key not in catalog `toolchain` AND no policy P1 override (R3).

There is no "trust on first use" in this spec; that is a verifier
policy decision and a verifier that adopts TOFU MUST document it as
such.

## 5. Verdict consistency

### 5.1 Single-solver verdicts

A property memento with exactly one verdict in `body.verdicts` is
consistent if:

- The verdict's wrapper `verdict` field is one of the closed-enum
  values per `2026-04-29-universal-claim-envelope.md`.
- The verdict's `body.irCid` equals the property's `body.irCid` (R6).
- The verdict's signature is valid (§4).

### 5.2 Multi-solver verdicts

A property memento with N > 1 verdicts is **agreed** iff every
verdict's wrapper `verdict` field is the same closed-enum value AND
each verdict's `body.inputContentCids` equals the union of the others'
(or a configured-per-kit equality semantics).

A property memento with N > 1 verdicts that are NOT agreed is
**disagreed**; the property memento MUST carry an explicit field
indicating disagreement is acknowledged:

```yaml
body:
  ...
  multiSolver:
    agreed: false
    rationale: optional CID    # CID of an audit/explanation memento
```

If `multiSolver.agreed` is missing or true on a property whose
verdicts disagree, R5 applies.

### 5.3 Verifier interpretation of disagreed verdicts

Disagreement is information, not failure. The verifier:

- MUST surface every per-solver verdict and its producer in the
  validity report (§9).
- MUST apply policy P2 to decide whether to ACCEPT the property.
  P2 is per-verifier: the protocol does NOT prescribe whether a
  disagreed property is acceptable. Common policies:
  - REJECT on any disagreement (strictest).
  - ACCEPT iff a designated quorum of solvers agree on `holds`.
  - ACCEPT iff a designated solver in the catalog's `toolchain`
    returns `holds`, regardless of others.
- A verifier with no P2 declared MUST default to REJECT on
  disagreement. The protocol's default is fail-closed.

### 5.4 Special verdict values

- `decayed`: per the universal-claim-envelope spec, indicates the
  bindingHash refers to content that no longer matches. A verdict
  with `decayed` does NOT invalidate the chain on its own, but it
  MUST be surfaced to the consumer. The verifier MUST set the
  property's status to `decayed` in the validity report and apply
  P3 (per-verifier acceptance of decayed properties; default
  REJECT).
- `undecidable`: an honest report that the producer could not
  finish. Treated by default as REJECT for that property unless P4
  accepts undecidable verdicts. The default is fail-closed.
- `error`: a producer fault; per the universal-claim-envelope spec,
  this is a defect in the producer. The verifier MUST treat
  `error` as REJECT unconditionally (no policy override; an errored
  verdict cannot be relied on, by definition).
- `violated`: the property does not hold. The verifier MUST treat
  `violated` as REJECT unconditionally for the property. This is
  not a policy choice; a violated verdict means the chain's claim
  is materially false.

## 6. Bridge consistency

### 6.1 Structural validity

For every bridge memento `b` in the chain:

- §3.4 invariants MUST all hold (target resolves, target is a
  property, catalog lists the property as active, source binding
  resolves, translation present iff cross-layer).
- The bridge's signature MUST be valid (§4).

### 6.2 Cross-layer bridges and the LLM workflow

The protocol does NOT validate the **semantic content** of a
translation memento for cross-layer bridges. That validation is the
case-3 LLM workflow's responsibility, and its output is itself a
memento that lives in the audit chain. The chain-validity gate at the
protocol level checks:

- Translation memento exists (§3.4).
- Translation memento is signed by a producer the catalog's
  `toolchain` vouches for (§4).
- The structural invariants (sourceLayer/targetLayer present and
  declared) are well-formed.

A verifier MAY apply policy P5 to require additional structural
checks (e.g., translation memento was minted within N days of the
bridge memento). The protocol declares only the minimum.

### 6.3 The bridge-resolution failure mode

A bridge MUST NOT resolve against a retired property. If
`targetContractCid` appears in the target catalog's
`body.retiredProperties` and not in `body.properties`, R7 applies.
This is the load-bearing semantic-firewall property described in
`2026-04-29-supply-chain-via-semantic-envelope.md`: when a library's
new catalog generation retires a property, every consumer's bridge
to that property fails to resolve cleanly, and the consumer's
chain-validity gate rejects.

A verifier MAY support a "compatibility mode" where bridges
resolving against retired properties surface as a WARNING but not
a REJECT, under P6. The default is REJECT.

## 7. External roots

### 7.1 The external-root concept

An **external root** is a CID referenced by a memento in the chain
that resolves to bytes the verifier is not prepared to interpret as a
ProvekIt memento. Examples:

- An Apple notarization signature CID (the bytes are an Apple `.p7`).
- A kernel.org tarball signature CID (the bytes are a detached PGP
  signature).
- A sigstore Rekor entry CID (the bytes are a Rekor log entry).
- A reproducible-build attestation CID (the bytes are an in-toto
  attestation).
- A Node.js release signature CID.

These CIDs are real content, signed by real institutions, and
inheriting their institutional weight. The protocol's role is to make
their presence in the chain EXPLICIT so that the verifier's policy
decides what to do.

### 7.2 The external-root memento

An external root MUST be carried by a `kind: external-root` wrapper
memento that is itself a ProvekIt memento. The `external-root`
memento body:

```yaml
kind: external-root
schema: <CID of the external-root schema>
body:
  externalCid: CID              # the foreign CID (Apple/sigstore/etc.)
  externalScheme: string        # e.g. "apple-notarization", "sigstore",
                                # "kernel-org-pgp", "in-toto-slsa"
  description: string           # human-readable description
  publishedBy: string           # e.g. "apple", "sigstore", "kernel.org"
  retrievedFrom: optional string # URL or registry name (informational)
  attachedAt: iso8601           # when the consumer kit attached this
                                # root into their chain
```

The `external-root` memento itself is signed by the consumer kit
that attached it; the verifier validates THAT signature normally
(§4). The `externalCid` is the un-interpretable inner CID; the
verifier neither resolves nor verifies it.

### 7.3 Verifier policy for external roots

The verifier MUST declare, per chain, which `externalScheme` values
its policy accepts. For each `kind: external-root` memento `e` in
the chain:

- If `e.body.externalScheme` is in the verifier's accepted-schemes
  set, the verifier MAY accept `e` as a leaf (P7). The validity
  report MUST list every accepted external-root scheme and its
  CIDs.
- If `e.body.externalScheme` is NOT accepted, R12 applies.

There is no protocol-level list of accepted schemes. A verifier
that accepts every scheme is fail-open and SHOULD NOT exist outside
explicit research/test contexts.

### 7.4 Scope of acceptance

Accepting an external-root memento does NOT mean the verifier
trusts the underlying foreign signature; it means the verifier
trusts the consumer kit's claim that the foreign signature is
relevant to the chain at this position. The validity report makes
this explicit (§9.4).

## 8. The fail-closed gate (reject case enumeration)

Every reject case below is named, has a condition, has a rationale,
and lists its override mechanism (or "no override; hard reject"). Every
other section of this spec references these IDs. The default is
REJECT.

### R1. Unresolvable required CID

**Condition.** A memento `m ∈ N` references a CID `c` via a typed
edge (§3) and `c` is not resolvable in the verifier's local store and
is not declared as an `external-root`.

**Rationale.** A chain that names a CID it cannot show is, by
definition, an unverifiable claim. Accepting it would defeat the
content-addressing primitive.

**Override.** None. Hard reject.

### R2. CID integrity failure

**Condition.** Bytes resolved at a CID `c` do not hash to `c` under
the canonical hash function defined in
`2026-04-29-universal-claim-envelope.md`.

**Rationale.** The local store is corrupted or malicious. CID
integrity is the protocol's deepest invariant.

**Override.** None. Hard reject.

### R3. Signature failure

**Condition.** Any of:

- A REQUIRED-to-be-signed memento (§4.2) lacks `producerSignature`.
- The signing producer's public-key memento is not resolvable.
- Signature arithmetic fails (the signature does not verify against
  the claimed key).
- The signing key was revoked as of the memento's `producedAt`
  (§4.4).
- The signing key is not in the relevant catalog's `toolchain` set
  and policy P1 is not declared.

**Rationale.** Without producer signatures, the chain reduces to a
trust-the-bytes claim that the protocol explicitly does not allow
for swarm-distributed mementos.

**Override.** P1 (per-verifier policy may accept signatures from
producers not in `toolchain`). No override for missing/forged
signatures.

### R4. Wrapper validation failure

**Condition.** A memento's wrapper does not satisfy
`2026-04-29-universal-claim-envelope.md`'s validation rules
(missing required fields, type mismatches, malformed `verdict`
enum, malformed `producedBy` format).

**Rationale.** Malformed wrappers cannot be canonicalized and
therefore cannot be content-addressed soundly.

**Override.** None. Hard reject.

### R5. propertyHash collision or multi-solver disagreement

**Condition.** Either:

- Two mementos in the chain claim the same `propertyHash` (per the
  wrapper) but reference different `irCid`s.
- A property memento has multiple verdicts whose wrapper `verdict`
  fields disagree, AND the property does not declare
  `multiSolver.agreed: false` with appropriate rationale.

**Rationale.** Either the canonicalizer is broken (different IRs
hashing to the same propertyHash) or the producer is dishonest
(multi-solver disagreement hidden). Both are catastrophic for the
relational-lookup model.

**Override.** None for the canonicalizer-failure case. P2 for the
disagreement case (per-verifier policy may accept disagreed
verdicts under explicit rules).

### R6. IR-CID inconsistency between property and verdict

**Condition.** A verdict memento's `body.irCid` differs from the
parent property memento's `body.irCid`.

**Rationale.** A verdict that is not "about" the property's IR is
not a verdict on the property; accepting it would let an attacker
attach an unrelated `holds` verdict to a different IR.

**Override.** None. Hard reject.

### R7. Bridge to retired property

**Condition.** A bridge memento's `targetContractCid` resolves to a
property that the target catalog has retired (present in
`retiredProperties`, absent from `properties`).

**Rationale.** The semantic-firewall property of the supply-chain
spec; consumers who depend on a property that was retired in a new
catalog generation MUST be told the dependency broke.

**Override.** P6 (per-verifier compatibility-mode policy MAY
downgrade to WARNING; default is REJECT).

### R8. Provenance memento integrity failure

**Condition.** An `inputCids` target either fails to resolve (R1
applies in that case) OR resolves to bytes that fail wrapper
validation (R4 applies in that case). R8 is the umbrella reference
into R1/R4 for the provenance-edge type.

**Rationale.** Provenance mementos are part of the chain's audit
substrate. A provenance edge that does not resolve cleanly is a
broken audit trail.

**Override.** None. Whatever R1/R4 says.

### R9. Same-layer bridge with translation memento

**Condition.** A bridge memento has `sourceLayer == targetLayer` AND
`body.translation` is present.

**Rationale.** A translation memento implies cross-layer reasoning;
its presence on a same-layer bridge indicates either misconfigured
authoring or an attempt to launder a cross-layer claim as same-layer.

**Override.** None. Hard reject.

### R10. Apparent cycle in chain

**Condition.** The verifier observes an edge sequence that returns
to a previously-visited CID.

**Rationale.** Per §1.4, this is impossible without a hash collision.
Treating it as anything other than evidence of corruption would
silently accept compromised content.

**Override.** None. Hard reject.

### R11. Audit memento on a Stage-only edge

**Condition.** A typed edge (per §3) that the spec defines as
pointing at a Stage memento resolves to a memento with
`evidence.kind == action-invocation`.

**Rationale.** Audit mementos are not claims; treating one as a
claim would let a consumer mistake "the action ran" for "the
property holds."

**Override.** None. Hard reject.

### R12. Unrecognized external-root scheme

**Condition.** A `kind: external-root` memento declares an
`externalScheme` not in the verifier's accepted-schemes set.

**Rationale.** An external root with no policy ruling is, by the
default-reject discipline, not trusted. The verifier cannot
silently accept content it has no rule for.

**Override.** P7 (per-verifier policy declares accepted schemes).
A scheme not in P7 is a hard reject.

### R13. Catalog refers to property not in its kit's IR language

**Condition.** A property memento's `irLanguage` is not declared by
the catalog's kit (e.g., a Rust catalog references a property
declared in `ts-ir@1`).

**Rationale.** Catalogs are kit-scoped; a property outside the kit's
language indicates either a misconfigured catalog or an attempt to
mix incompatible kits without an explicit bridge.

**Override.** A verifier MAY accept under P8 (cross-kit catalogs);
default REJECT.

### R14. Toolchain pin mismatch

**Condition.** A verdict memento's `body.toolchainPin` is present
but does not match a CID in the parent catalog's `toolchain`, AND
no per-policy P1 override is declared.

**Rationale.** Toolchain pinning exists to make the
"reflections-on-trusting-trust" attack surface explicit. A verdict
signed by a tool not in the catalog's toolchain set is, by
default, not part of the catalog's trust boundary.

**Override.** P1 (already enumerated under R3).

### R15. Schema CID unresolvable for an evidence variant the
verifier needs to read

**Condition.** A consumer needs to read `evidence.body` of a
memento the verifier wants to interpret structurally (e.g., to
extract `irCid` from a verdict's evidence body), AND the
`evidence.schema` CID is not resolvable.

**Rationale.** Without the schema, the body's bytes cannot be
interpreted soundly. Per
`2026-04-29-universal-claim-envelope.md`, this would normally make
the memento "verdict-trustworthy but witness-inscrutable"; for
chain-validity edges that depend on body fields (like `irCid`), the
inscrutability is fatal because the edge cannot be drawn.

**Override.** None. Hard reject.

### Override mechanisms (verifier policies)

For completeness, every override referenced above:

- **P1.** Accept signatures from producers not in `toolchain`.
- **P2.** Accept disagreed multi-solver verdicts under explicit
  per-policy quorum/designated-solver rules.
- **P3.** Accept properties with `decayed` verdicts (default
  REJECT).
- **P4.** Accept properties with `undecidable` verdicts (default
  REJECT).
- **P5.** Additional structural checks for cross-layer bridge
  translations (e.g., recency).
- **P6.** Compatibility mode: downgrade R7 (bridge to retired
  property) to WARNING.
- **P7.** Accepted external-root schemes.
- **P8.** Accept cross-kit catalog references (R13 override).

A verifier MUST document its policy set as a content-addressed
artifact (its own memento, of `kind: verifier-policy`, schema TBD)
so that the chain it validated can be reproduced by another verifier
under the same policy. The protocol-level requirement is that the
policy is **named and machine-readable**; an unnamed override is
indistinguishable from a silent reject-failure.

## 9. The validity report

A successful invocation of chain validity MUST return a structured
**validity report**, not a boolean. The report names every leaf
verified, every edge checked, every external root accepted, and
every disagreement tolerated. The verifier's output IS evidence; the
report itself is content-addressable and SHOULD be minted as a
memento so that conformance is itself auditable.

### 9.1 Recommendation: report as memento

The validity report SHOULD be wrapped as a Stage memento with
`evidence.kind: chain-validity-report`. This makes verifier
conformance composable: a verifier's report is a leaf in some larger
audit chain.

The variant body schema:

```yaml
kind: chain-validity-report
schema: <CID of the chain-validity-report schema>
body:
  rootProofHash: CID            # the catalog memento that was verified
  validityReportVersion: 1
  verifierIdentity: string      # e.g. "provekit-verifier@0.4.2"
  policyCid: CID                # the verifier's policy memento CID
  verifiedAt: iso8601
  status: enum                  # "valid" | "invalid"
  rejectReasons: optional [RejectReason]   # populated iff status=invalid
  verifiedLeaves: [VerifiedLeaf]           # every property reached
  acceptedExternalRoots: [ExternalRoot]    # every external root accepted
  toleratedDisagreements: [Disagreement]   # multi-solver disagreements
                                           # accepted under P2
  warnings: [Warning]                      # P6/P3/etc downgrades
```

### 9.2 RejectReason

```yaml
RejectReason:
  rule: enum                    # "R1" | "R2" | .. | "R15"
  mementoCid: CID               # the memento that triggered the rule
  edge: optional string         # the edge field that was traversed
  detail: string                # human-readable detail
```

A report with `status: invalid` MUST list at least one
`rejectReason`. A verifier SHOULD list every reject reason
encountered, not just the first; this makes it possible to fix
multiple problems in one consumer-side iteration. A verifier MAY
short-circuit on the first reject if performance constraints
require.

### 9.3 VerifiedLeaf

```yaml
VerifiedLeaf:
  propertyCid: CID
  propertyHash: hex16           # the wrapper-level propertyHash
  irCid: CID
  verdictCids: [CID]            # every verdict referenced by the property
  multiSolverAgreed: bool
  status: enum                  # "holds" | "violated" | "decayed" |
                                # "undecidable" | "error"
```

If `status` is anything other than `holds`, the report's overall
`status` MUST be `invalid` UNLESS the verifier's policy explicitly
accepts this status (P3 for `decayed`, P4 for `undecidable`).
`violated` and `error` are unconditional invalidations.

### 9.4 ExternalRoot

```yaml
ExternalRoot:
  externalRootCid: CID          # the kind: external-root memento CID
  externalScheme: string        # the foreign scheme (e.g. "sigstore")
  externalCid: CID              # the inner foreign CID
  policyClause: string          # which P7 clause accepted this scheme
```

The report makes EXPLICIT what the verifier did NOT verify
mechanically. A consumer reading the report can audit whether they
agree with the verifier's policy.

### 9.5 Disagreement and Warning

```yaml
Disagreement:
  propertyCid: CID
  perSolverVerdicts: [PerSolverVerdict]
  policyClause: string          # P2 clause that accepted this
                                # disagreement

PerSolverVerdict:
  verdictCid: CID
  producedBy: string            # producer-id
  verdict: enum

Warning:
  rule: enum                    # the rule that would have rejected
  mementoCid: CID
  policyClause: string          # the override that downgraded
  detail: string
```

### 9.6 The validity report as evidence

When emitted as a memento (§9.1), the validity report itself can be
signed by the verifier and referenced in larger compositions. A
consumer's own catalog can include the validity report's CID in
its `inputCids`; another verifier can then use that report as
secondary evidence (subject to its own policy) without re-running
the entire chain validity check. This is consistent with the "stop
at hashes" discipline: verifier conformance, like everything else
in the framework, is content-addressable.

## 10. Conformance criteria

A verifier conforms with this spec iff for every chain G it accepts:

1. It resolved every reachable CID per §2 (or accepted under §7's
   external-root policy).
2. It validated every memento's wrapper per
   `2026-04-29-universal-claim-envelope.md` and per §3.
3. It verified every REQUIRED-to-be-signed memento's signature per
   §4.
4. It enforced every per-edge invariant in §3.
5. It applied verdict-consistency rules per §5 and bridge-
   consistency rules per §6.
6. It rejected on any condition matching R1-R15 unless the
   corresponding override (P1-P8) is explicitly named in its
   declared policy.
7. It produced a validity report per §9, naming every accepted
   leaf, edge, external root, disagreement, and warning.

A verifier that fails any of (1)-(7) is non-conformant and its
"valid" reports MUST NOT be relied on by downstream consumers.

A conformant verifier's identity (its `verifierIdentity` string) and
policy CID are themselves part of every validity report. A consumer
who trusts a specific verifier-identity-and-policy pair can rely on
that pair's reports without re-running verification, the same way
they rely on any other content-addressed leaf.

## 11. Adversarial fail-closed walkthrough

This section maps the supply-chain attacks from
`2026-04-29-supply-chain-via-semantic-envelope.md` onto the
chain-validity rules they trigger. Each attack is followed by the
specific rule that catches it.

### 11.1 Maintainer takeover

**Attack.** Attacker phishes maintainer credentials. Pushes V2 of a
library. The new V2 catalog memento retires a property the consumer
depends on, OR the new V2 retains the property but ships a `verdict:
violated` for the new code, OR the new V2 ships `verdict: holds`
that disagrees with the new code.

**Detection paths:**

- If V2 retires the property: the consumer's bridge to that property
  becomes a bridge to a retired property. R7 applies. REJECT.
- If V2 retains the property with `verdict: violated`: §5.4 specifies
  unconditional REJECT for `violated`. The chain is invalid.
- If V2 retains the property with a `holds` verdict that doesn't
  match the new code: §5.4 alone does not catch this; what catches
  it is that the verdict's `body.inputContentCids` references the
  new code's CIDs, the consumer's verifier re-runs the verification
  in its own kit, and the recomputed verdict does not match the
  imported claim. The mismatch is caught at **the verifier's local
  re-verification stage**, which is upstream of chain validity (the
  re-verification policy is per-kit). Chain validity itself does not
  re-run solvers; it checks structural and signature invariants. The
  re-verification gate (defined per host kit) is what surfaces the
  inconsistent claim; chain validity then either rejects on the
  resulting `violated` re-verdict (§5.4) or accepts the original
  `holds` and produces a validity report whose policy clause makes
  the trust assumption explicit.

**Result.** A maintainer-takeover attack cannot keep the same
property, the same verdict, AND ship code that violates the
property, without producing a chain that fails at one of these
gates. The attacker's only remaining surface is properties the
catalog does not constrain, which is the load-bearing observation
of the supply-chain spec: "the more invariants a library publishes,
the smaller the maintainer-takeover blast radius."

### 11.2 Long-term insider turn

**Attack.** A maintainer of three years inserts a backdoor in V47.

**Detection.** Identical to 11.1 in mechanism. The defense extends
no further: a careful insider adds the backdoor in space not
constrained by any property. Chain validity does not catch what
no property constrains. This is the protocol's honest scope.

### 11.3 Stealth bug-shaped sabotage

**Attack.** A "fix" that violates a property the consumer depends
on. Tests pass at the upstream, but the consumer's bridge depends
on a stricter formal property.

**Detection.** Same as 11.1. The verdict on V47's catalog either
honestly reports `violated` (chain invalid by §5.4), or dishonestly
reports `holds` and is caught by R6 if the verdict's `irCid` is
mismatched, by R3 if signed by an untrusted producer, or by the
host kit's re-verification gate.

### 11.4 Compromised CI / toolchain

**Attack.** The build infrastructure's tools are compromised; tsc
or the lifter ships malicious behavior.

**Detection.** R14 (toolchain pin mismatch). The catalog's
`toolchain` field declares which producer-public-key mementos this
kit vouches for. A verdict produced by a tool whose key is not in
that set fails R14. The defense reduces to "is the kit's
toolchain-pin set itself trustworthy?", which depends on layers
1-4 (npm provenance, sigstore, etc.) and is out of chain-validity
scope. The chain-validity gate makes the trust boundary explicit
and rejects the cases where the trust boundary is violated.

### 11.5 Typosquatting / dependency confusion

**Attack.** Consumer pulls `lod4sh` thinking it's `lodash`.

**Detection.** R1. The consumer's `package.json` (or equivalent)
binds dependency names to specific catalog CIDs. The typosquat's
catalog CID is different from the legitimate `lodash` catalog CID.
Either:

- The consumer's bridges reference the legitimate `lodash`
  property CIDs. Resolving against the typosquat's catalog finds
  those property CIDs absent. R1 (or R7 if retired-style listing)
  applies.
- The consumer manually re-pointed at `lod4sh`'s catalog CID. The
  consumer's bridges now reference the typosquat's properties; the
  chain is internally consistent under the new pointing, but the
  consumer's *intent* (depend on `lodash`) is no longer encoded in
  the chain. This is a layer-1 (identity) failure, not a chain-
  validity failure. Chain validity does what it can: it makes the
  catalog CID the source of truth, removing names from the
  binding mechanism.

### 11.6 Sub-dependency / transitive attacks

**Attack.** Compromise three levels deep.

**Detection.** Catalog composition is recursive: A's catalog refers
to B's catalog (via bridges from A's properties to B's properties),
and B's refers to C's. C's compromise mints a new catalog CID. B's
bridges to C now reference V1 property CIDs not in C-V2's catalog;
R7 applies on the inner walk. If the verifier walks transitively
(per-policy decision), the attack surfaces. If the verifier does
not walk transitively, B's authors are responsible for re-bridging
when they bump their own dep on C. Chain validity makes the
transitive dependency mechanical at the protocol level; consumer
policy decides walk depth.

### 11.7 Malicious update via CDN

**Attack.** CDN serves modified bytes for a package.

**Detection.** R2 (CID integrity failure). The bytes don't hash to
the claimed CID. The verifier rejects before any further
processing.

### 11.8 Sybil attack on review / governance

**Attack.** Fake maintainer accounts merge a malicious PR.

**Detection.** Identical to 11.1: the merged change either
constrains a published property and gets caught at the verdict
layer, or it operates in unconstrained space. Chain validity is
not a defense against the merge itself; it is a defense against
shipping the merged result downstream without the consumer
noticing.

## 12. Drift from current implementation

The TypeScript runtime in `src/fix/runtime/` is the current
reference implementation. As of 2026-04-30, the runtime IMPLEMENTS:

- Per-invariant binding decay detection (`verify.ts`).
- Z3 path enumeration and per-path verdict aggregation
  (`pathEnumerator.ts`, `pathChecker.ts`).
- Memento store with content-addressed write/read
  (`mementoStore.ts`).
- Verify-cache lookup (`verifyCache.ts`).

The runtime DOES NOT IMPLEMENT (drift, requires alignment):

- Chain validity as defined in this spec. The current verifier
  operates on a flat list of invariants per project; it does not
  walk a catalog → property → verdict graph.
- Producer-signature verification per the universal-claim-envelope
  v1 scheme. No `producerSignature` field is currently checked.
- Bridge mementos. No `kind: bridge` is emitted or consumed by the
  current runtime. The cross-package composition story is
  speculative until bridges are implemented.
- External-root mementos. No `kind: external-root` exists yet.
- Catalog mementos as a kind. The current `mementoStore.ts` writes
  per-invariant verdict mementos but no top-level catalog memento
  per project.
- Toolchain pinning. No `toolchain` field, no
  `producer-public-key` mementos, no key-rotation or revocation
  chain.
- The validity report as a memento. The current verifier returns
  per-invariant status objects, not a structured chain-validity
  report.

These are not "bugs in the current code"; they are surface area the
spec adds. Alignment work is anticipated in a follow-up branch.
The relevant files to align:

- `src/fix/runtime/verify.ts`: needs a chain-validity entry point
  separate from the current per-invariant aggregator.
- `src/fix/runtime/mementoStore.ts`: needs new kinds (`catalog`,
  `bridge`, `external-root`, key mementos) and signature fields.
- `src/fix/runtime/verifyCache.ts`: its caching keys today are
  per-invariant; chain-validity reports need their own caching
  layer keyed by `(rootProofHash, policyCid)`.
- A new `src/fix/runtime/chainValidity.ts` (or equivalent) is the
  natural home for §3-§9's logic.

The drift is documented for visibility; this spec does not block on
its resolution. Implementation alignment is its own scope.

## 13. Worked examples

### 13.1 A valid chain (smallest case)

A package with one property, one verdict, no bridges, no external
roots:

```
catalog (root)
├── properties: [propA]
└── toolchain: [keyZ3]

propA (kind: property)
├── irCid: irA
├── verdicts: [verdictA]
└── (signed by keyAuthor, key in catalog.toolchain)

verdictA (kind: verdict)
├── body.irCid: irA              ← matches propA.body.irCid
├── body.toolchainPin: keyZ3
├── verdict: holds
└── evidence.kind: z3-unsat

irA (canonical IR bytes)
keyZ3 (producer-public-key memento)
keyAuthor (producer-public-key memento)
```

Verifier walk: catalog resolves; `propA` resolves; signature on
`propA` valid against `keyAuthor`; `verdictA` resolves; signature on
`verdictA` valid against `keyZ3`; `verdictA.body.irCid == propA.body.irCid`
(R6 passes); `irA` resolves; canonical-form check on `irA` per its
declared `irLanguage`. Validity report: `status: valid`, one
verified leaf, no external roots, no warnings.

### 13.2 An invalid chain: maintainer takeover via retirement

V2 of a library retires propA. A consumer bridges into propA:

```
consumer-catalog (root)
├── properties: [consumerPropX]
├── bridges: [bridgeAtoX]

consumerPropX (kind: property)
└── verdicts: [verdictX]

bridgeAtoX (kind: bridge)
├── targetCatalogCid: lib-V2-catalog
├── targetContractCid: propA       ← in lib-V2's retiredProperties

lib-V2-catalog (kind: catalog)
├── properties: []                 ← propA NOT here
└── retiredProperties: [propA]
```

Verifier walk: bridgeAtoX resolves; targetContractCid (propA)
resolves to a property memento; lib-V2-catalog resolves; verifier
checks "is propA in lib-V2-catalog.body.properties?" and finds it is not.
R7 applies. REJECT. Validity report: `status: invalid`,
rejectReasons: [{rule: R7, mementoCid: bridgeAtoX, ...}].

If the verifier has policy P6 (compatibility mode), R7 downgrades
to a warning; the validity report's `status: valid` BUT carries
the warning prominently. Default behavior: REJECT.

### 13.3 An invalid chain: signature failure

A verdict signed by a key that was revoked before its `producedAt`:

```
catalog
├── properties: [propA]

propA
├── verdicts: [verdictA]

verdictA (kind: verdict)
├── producedAt: 2026-04-30T00:00:00Z
├── producedBy: z3-symbolic@4.13
├── producerSignature: <ed25519 sig by keyZ3-old>

keyZ3-old (producer-public-key)
keyZ3-old-revocation (producer-key-revocation)
├── producedAt: 2026-04-29T00:00:00Z   ← strictly before verdictA's
                                         producedAt
```

Verifier walk: signature on verdictA verifies arithmetically against
keyZ3-old. The verifier resolves the revocation memento; revocation's
`producedAt` (2026-04-29) is less than verdictA's `producedAt`
(2026-04-30). R3 applies. REJECT.

### 13.4 A valid chain with external root

A consumer attaches an Apple notarization signature for the
underlying binary:

```
catalog
├── properties: [propA]
├── externalRoots: [appleNotarization]

appleNotarization (kind: external-root)
├── externalScheme: "apple-notarization"
├── externalCid: <Apple's CID>
├── publishedBy: "apple"
```

Verifier policy P7 declares `apple-notarization` as accepted. The
external-root memento is signed by the consumer kit; signature
verifies (§4). The verifier does NOT verify the inner Apple
signature. Validity report: `status: valid`, one verified leaf
(propA), one accepted external root (appleNotarization with
policyClause referencing the P7 entry).

If P7 does not accept `apple-notarization`, R12 applies. REJECT.

## 14. Acknowledgments and scope

This spec defines structural and signature gating only. Three things
explicitly out of scope:

- **Canonicalization.** What makes two IR strings equal at the byte
  level is `2026-04-30-ir-formal-grammar.md` and
  `2026-04-29-ast-canonicalizer.md`'s job. This spec assumes
  canonicalization is correct.
- **Signature mechanics.** The cryptographic primitives (ed25519,
  signature placement, key rotation chain) are
  `2026-04-29-universal-claim-envelope.md`'s job. This spec
  inherits.
- **Verifier policy authoring.** This spec enumerates the override
  mechanisms (P1-P8) but does not prescribe which a consumer
  should adopt. Policy is a downstream artifact, content-
  addressable in its own right, but its content is institutional,
  not protocol.

The fail-closed property is the load-bearing discipline. Every
ambiguous case is a reject case unless an explicit named policy
clause overrides it. A gate that fails open is worse than no gate;
the protocol's posture is that the verifier's job is to find
reasons to reject, not reasons to accept.

When this spec and the others land together, ProvekIt has its
load-bearing protocol surface: a content-addressed graph of
semantic claims, a canonical form for those claims, a signature
scheme, and a fail-closed gate that defines what "verified" means.
Everything else is implementation in service.
