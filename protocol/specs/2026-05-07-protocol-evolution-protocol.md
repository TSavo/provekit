# Protocol Evolution Protocol (PEP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-07
**Layer:** extension protocol over TDP, GCP, protocol catalogs, and the ProofIR/memento substrate
**Related:**
- `2026-05-06-extension-protocols.md` - extension-protocol doctrine, DAG-of-DAGs, non-executing core
- `2026-05-06-truth-discharge-protocol.md` - unit truth over signed body-claims
- `2026-05-06-grammar-conformance-protocol.md` - grammar and invariant conformance for extension bodies
- `2026-05-06-fix-receipt-protocol.md` - implementation repair receipts
- `2026-04-30-protocol-versioning.md` - catalog CID as protocol version
- `2026-04-30-protocol-catalog-format.md` - catalog bytes, spec CIDs, and catalog CID rules
- `2026-04-30-proof-file-format.md` - `.proof` bundle format

## Section 0. Purpose

PEP defines how protocol evolution becomes data.

A protocol change is not only a migration note, release announcement, or
maintainer promise. In Sugar, a protocol change can be represented as
a signed, content-addressed, grammar-constrained, invariant-constrained,
witnessable artifact:

```text
from catalog CID + to catalog CID + exact change set + evidence roots + policy
  -> ProtocolEvolutionBodyClaim
  -> TruthDischargeWitness(result = true)
```

The witness does not say "trust the new world." It says:

```text
this exact successor catalog is admissible from this exact predecessor
under this exact evolution policy
```

Protocol evolution can therefore be proven, compared, bridged, pinned,
refused, superseded, audited, and cached using the same substrate as any
other claim.

PEP is an extension protocol. It does not change content addressing,
catalog hashing, signature semantics, envelope/header shape, or core
verification. Core verification still validates signed bytes, CIDs,
references, and finite core rules. PEP-aware tooling interprets the
evolution claim and emits optional witness or refusal artifacts.

## Section 1. Relationship to Fix Receipts

PEP is receipt-shaped, but it is not a Fix Receipt Protocol (FRP)
receipt.

FRP witnesses a repair to an implementation artifact:

```text
pre artifact CID -> transformed artifact CID -> re-lift -> gap closed
```

PEP witnesses an admissible change to a protocol catalog:

```text
old protocol catalog CID -> new protocol catalog CID -> evolution admitted
```

The distinction:

| Question | FRP | PEP |
|---|---|---|
| What changed? | Host-language bytes or derived artifacts. | Protocol catalogs, spec CIDs, grammars, invariants, policies, or conformance requirements. |
| What is witnessed? | A named ProofIR gap is closed after re-lift. | A successor catalog is admissible under evolution policy. |
| What is the before/after pair? | `preArtifactCid` and `transformedArtifactCid`. | `fromCatalogCid` and `toCatalogCid`. |
| What is the load-bearing evidence? | Post-transform lift plus closure witness. | Catalog diff, grammar/invariant witnesses, conformance witnesses, migration witnesses, and policy admission. |
| What root should parents reference? | The fix receipt or closure witness root. | The protocol evolution TDP witness root. |

FRP and PEP compose. If a protocol change requires implementation
changes, the PEP claim SHOULD reference the required FRP roots or
implementation conformance witnesses beneath the protocol-evolution
evidence DAG.

The slogan:

```text
FixReceipt witnesses a repair.
PEP witnesses an admissible change to what counts as repair,
conformance, or validity.
```

## Section 2. Relationship to protocol versioning

`2026-04-30-protocol-versioning.md` defines the protocol version as the
catalog CID. Changing any spec bytes changes the spec CID; changing the
catalog's `properties` map changes the catalog CID. That is the byte
identity rule.

PEP does not replace that rule. PEP adds an evidence rule:

```text
catalog CID tells you what version is.
PEP witness tells you why this version is an admitted successor.
```

Versioning says:

```text
old catalog bytes -> old catalog CID
new catalog bytes -> new catalog CID
```

PEP says:

```text
old catalog CID + new catalog CID + exact delta + policy + evidence
  -> admitted evolution witness
```

A new catalog SHOULD NOT be required to contain the final PEP witness
over itself. That creates unnecessary self-reference pressure. The clean
shape is:

```text
old catalog
new catalog
ProtocolEvolutionBodyClaim(old, new, evidence)
  -> outer TruthDischargeWitness
  -> proof bundle or adoption root
```

The new catalog may list the PEP spec CID as a protocol property. The
PEP witness that admits the new catalog lives beside or above the catalog
as a referenced witness root.

## Section 3. Vocabulary

**ProtocolEvolutionBodyClaim.** Canonical signed/content-addressed body
bytes describing a proposed evolution from one protocol catalog CID to
another.

**ProtocolEvolutionWitness.** A TDP-compatible positive witness over a
`ProtocolEvolutionBodyClaim`.

**ProtocolEvolutionRefusal.** A signed refusal explaining why an
evolution claim was not discharged under the cited verifier and policy.

**Change set.** The exact catalog-property delta between
`fromCatalogCid` and `toCatalogCid`: added, modified, removed,
deprecated, or renamed property keys and their CIDs.

**Change class.** The declared evolution class: `extension-only`,
`compatible`, `migration-required`, `breaking`, `core-candidate`, or
`key-rotation`.

**Migration witness.** Evidence that artifacts or implementations that
conformed to the old catalog can be translated, bridged, repaired, or
explicitly refused under the new catalog.

**Adoption witness.** Evidence that a specific implementation, CLI,
library, verifier, checker, parser, or consumer conforms to a catalog or
extension protocol set.

**Evolution policy.** The content-addressed acceptance rule deciding
which signers, verifiers, migration evidence, compatibility claims,
grammar witnesses, and conformance witnesses are sufficient.

**Evolution root.** The TDP witness CID for the positive protocol
evolution discharge. Parent claims SHOULD reference this root when they
rely on the admitted protocol change.

## Section 4. Body-claim shape

Draft PEP body-claim convention:

```json
{
  "kind": "ProtocolEvolutionBodyClaim",
  "schemaVersion": "1",
  "protocolName": "sugar-protocol",
  "fromCatalogCid": "blake3-512:...",
  "toCatalogCid": "blake3-512:...",
  "fromVersionLabel": "v1.6.0-2026-05-05",
  "toVersionLabel": "v1.6.1-2026-05-07",
  "changeClass": "extension-only",
  "changeSet": {
    "added": [
      {
        "propertyKey": "protocol-evolution-protocol",
        "toCid": "blake3-512:...",
        "layer": "extension",
        "reasonCid": "blake3-512:..."
      }
    ],
    "modified": [
      {
        "propertyKey": "grammar-conformance-protocol",
        "fromCid": "blake3-512:...",
        "toCid": "blake3-512:...",
        "layer": "extension",
        "reasonCid": "blake3-512:..."
      }
    ],
    "removed": [],
    "deprecated": []
  },
  "compatibility": {
    "claim": "backward-compatible",
    "migrationRequired": false,
    "bridgeWitnessCids": [],
    "migrationWitnessCids": []
  },
  "evidence": {
    "catalogDiffCid": "blake3-512:...",
    "grammarConformanceWitnessCid": "blake3-512:...",
    "invariantConformanceWitnessCid": "blake3-512:...",
    "conformanceCorpusCid": "blake3-512:...",
    "conformanceWitnessCids": ["blake3-512:..."],
    "implementationAdoptionWitnessCids": ["blake3-512:..."],
    "fixReceiptCids": []
  },
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."],
  "producer": {
    "kind": "working-group|maintainer|automation|implementation",
    "name": "sugar",
    "version": "0.1.0"
  }
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"ProtocolEvolutionBodyClaim"`. |
| `schemaVersion` | MUST be `"1"` for this draft. |
| `protocolName` | Protocol identifier. For Sugar, `"sugar-protocol"`. |
| `fromCatalogCid` | CID of the predecessor protocol catalog. |
| `toCatalogCid` | CID of the successor protocol catalog. |
| `fromVersionLabel` | Human-facing predecessor version label, informational but policy-checkable. |
| `toVersionLabel` | Human-facing successor version label, informational but policy-checkable. |
| `changeClass` | Declared evolution class. |
| `changeSet` | Exact catalog-property delta between predecessor and successor. |
| `compatibility` | Declared compatibility and migration posture. |
| `evidence` | Evidence roots used by the verifier to discharge the evolution claim. |
| `verifierCid` | CID of the accepted protocol-evolution verifier/checker. |
| `policyCid` | CID of the evolution policy. |
| `inputCids` | Prior artifacts this claim depends on. MUST include `fromCatalogCid`, `toCatalogCid`, and all non-null evidence roots. |
| `producer` | Signed metadata identifying the claim producer. |

The body-claim is content-addressed. Changing a reason, migration
witness, compatibility claim, verifier, policy, or evidence root changes
the body CID and invalidates prior PEP witnesses over that body.

## Section 5. Witness shape

PEP positive witnesses SHOULD be TDP witnesses with:

```json
{
  "kind": "TruthDischargeWitness",
  "schemaVersion": "1",
  "claimBodyCid": "blake3-512:...",
  "claimKind": "protocol-evolution",
  "result": true,
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "evidenceRootCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."],
  "execution": {
    "startedAt": "2026-05-07T00:00:00Z",
    "finishedAt": "2026-05-07T00:00:00Z",
    "fuelUsed": 0
  }
}
```

For PEP, `claimBodyCid` is the CID of a
`ProtocolEvolutionBodyClaim`. The `result` is deliberately small:

```text
true(this evolution body was admitted under this verifier and policy)
```

It does not globally bless every referenced artifact. It discharges the
one evolution claim named by the body.

## Section 6. Nontriviality rule

A positive PEP witness is nontrivial only if it binds both sides of the
protocol change:

```text
fromCatalogCid != toCatalogCid
and both catalogs resolve
and both catalog CIDs verify under the catalog hashing rule
and the actual catalog-property delta equals changeSet exactly
and every changed spec CID resolves
and every changed spec CID hashes to the referenced spec bytes
and all required signatures or catalog attestations verify
and the declared changeClass is accepted by policy
and the required grammar/invariant/conformance/migration witnesses exist
and every witness root named by evidence is itself verified or policy-accepted
```

An announcement, changelog, release tag, migration guide, unsigned diff,
or new catalog by itself may be useful. It is not a positive PEP witness
unless the evolution body has been discharged under policy.

## Section 7. Evolution classes

PEP defines the following draft evolution classes.

### Section 7.1 `extension-only`

The successor catalog adds or changes extension protocol specs, extension
body conventions, optional tooling workflows, extension grammars, or
extension policies without changing core substrate verification.

An `extension-only` change MUST NOT require a core verifier to learn a
new primitive. A non-aware consumer can still verify signatures, CIDs,
references, and core memento semantics.

### Section 7.2 `compatible`

The successor catalog changes rules in a way policy accepts as
backward-compatible. Existing valid core artifacts remain valid under the
successor catalog, though consumers MAY gain new optional evidence paths.

### Section 7.3 `migration-required`

The successor catalog requires migration evidence for some artifacts,
implementations, policies, or extension bodies. The PEP body MUST name
the required migration witness roots.

If implementation bytes must change, the PEP evidence SHOULD reference
FRP receipts or equivalent post-migration conformance witnesses.

### Section 7.4 `breaking`

The successor catalog intentionally breaks compatibility. A `breaking`
claim MUST name the broken property keys, the reason artifacts, and the
required adoption or refusal path.

Policy MAY refuse `breaking` claims unless an accepted governance,
deprecation, or migration process is witnessed.

### Section 7.5 `core-candidate`

The successor catalog proposes a change to core substrate behavior:
envelope shape, header semantics, CID derivation, canonicalization,
signature verification, core memento validity, or required reference
rules.

PEP can describe and witness the proposal process, but an extension
protocol MUST NOT silently make a core-candidate change load-bearing for
core verification. If accepted, the change moves through the core
protocol versioning process. Old core verifiers may refuse the new
catalog.

### Section 7.6 `key-rotation`

The successor catalog or sidecar attestations change accepted signing
keys, root keys, quorum rules, or trust anchors. The PEP body MUST name
the old key material, new key material, rotation attestation, and policy
under which the rotation is accepted.

### Section 7.7 Version labels

The catalog CID is the protocol version. A semantic version label such as
`v1.6.1` is human-facing metadata. It becomes meaningful only because
policy interprets it.

PEP therefore treats the version label as a policy-checked claim over the
catalog transition, not as the source of truth.

Recommended default policy:

| Change shape | Default label movement |
|---|---|
| Clarification, re-sync, catalog attestation refresh, or extension-only change with no new cross-kit semantic obligation | patch |
| Additive core-facing grammar growth, new required conformance fixture, or new cross-kit semantic obligation | minor |
| Breaking core rule, removed valid artifact class, or incompatible required migration | major |

This means a cataloged extension protocol can be a patch bump when it
does not change core substrate verification and does not require language
kits to emit, lift, canonicalize, or verify new cross-kit semantics.

The PEP verifier MAY reject a version label that is incompatible with the
declared `changeClass` and evidence. The rejection is about the label
claim, not about the bytes. The successor catalog may still exist; it is
not admitted under that policy.

## Section 8. Protocol adoption claims

PEP also standardizes the companion question:

```text
does this implementation implement this protocol catalog under this policy?
```

That question is not the same as catalog evolution, but it is the
natural adoption edge produced after an evolution is admitted.

Draft adoption body convention:

```json
{
  "kind": "ProtocolAdoptionBodyClaim",
  "schemaVersion": "1",
  "protocolName": "sugar-protocol",
  "catalogCid": "blake3-512:...",
  "implementationCid": "blake3-512:...",
  "implementationKind": "rust-cli|java-kit|typescript-kit|verifier|lifter|dropper",
  "supportedExtensionProtocolCids": ["blake3-512:..."],
  "conformanceCorpusCid": "blake3-512:...",
  "conformanceWitnessCids": ["blake3-512:..."],
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

Positive adoption witnesses SHOULD be TDP witnesses with:

```json
{
  "kind": "TruthDischargeWitness",
  "schemaVersion": "1",
  "claimBodyCid": "blake3-512:...",
  "claimKind": "protocol-adoption",
  "result": true,
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "evidenceRootCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

This is the witnessed form of:

```text
this program implements the `.proof` protocol / this catalog / these
extension protocols under this conformance policy
```

An implementation may support the core catalog but not every extension
protocol. It may support PEP verification but not ORP transforms. The
adoption body makes that support set explicit and content-addressed.

## Section 9. Relationship to GCP

PEP bodies SHOULD be witnessed under GCP.

The useful stack is:

```text
PEP grammar CID
PEP invariant set CID
PEP parser CID
PEP invariant checker CID
ProtocolEvolutionBodyClaim body CID
  -> grammar conformance witness
  -> protocol evolution body
  -> TDP witness(result = true)
```

GCP answers:

```text
is this body shaped like a valid PEP body?
```

PEP answers:

```text
does this shaped body describe an admissible protocol evolution?
```

The PEP invariant set SHOULD include at least:

1. `fromCatalogCid` and `toCatalogCid` are syntactically valid CIDs.
2. `inputCids` contains `fromCatalogCid`, `toCatalogCid`, and all
   non-null evidence roots.
3. The actual catalog delta equals `changeSet`.
4. Every changed property has exactly one declared layer.
5. `extension-only` changes do not alter core substrate specs; claims
   that do so are rejected or reclassified as `core-candidate`.
6. `core-candidate` changes are not treated as extension-only changes.
7. `migration-required` and `breaking` changes name migration or refusal
   evidence accepted by policy.
8. No body claims its own enclosing final witness CID.
9. Parent reliance points to the TDP witness root, not to a hand-picked
   subset of evidence.

## Section 10. Consumer behavior

A PEP-unaware consumer:

1. MAY ignore PEP-specific body fields semantically.
2. MUST still verify signed bytes, CIDs, signatures, references, and
   core memento/header validity for the artifacts it understands.
3. MUST NOT silently treat a new catalog as an admitted successor merely
   because a PEP-shaped body exists.
4. SHOULD fail closed when asked to rely on PEP-specific evolution
   semantics.

A PEP-aware consumer:

1. Resolves the predecessor and successor catalogs.
2. Recomputes catalog CIDs according to catalog rules.
3. Recomputes the exact property delta.
4. Checks the PEP body grammar and invariant witnesses when policy
   requires them.
5. Checks required signatures, key rotations, migration evidence,
   conformance witnesses, and adoption witnesses.
6. Verifies or policy-accepts the TDP witness over the PEP body.
7. Emits acceptance, refusal, or degraded-evidence status.

Ignoring PEP semantics does not mean ignoring PEP bytes. The body remains
signed and content-addressed.

## Section 11. Fail-closed behavior

PEP-aware tooling MUST fail closed when:

1. either catalog is unavailable;
2. either catalog CID fails to verify;
3. any changed spec CID is unavailable or hashes incorrectly;
4. the computed catalog delta does not match `changeSet`;
5. a required catalog signature or key-rotation attestation is invalid;
6. the declared `changeClass` is incompatible with the actual delta;
7. a required grammar, invariant, conformance, migration, adoption, or
   fix receipt witness is absent or invalid;
8. the verifier or policy CID is unsupported;
9. the evolution body is malformed;
10. extension execution times out, exceeds fuel, refuses, or does not
    terminate.

These failures do not make the underlying catalogs invalid as signed or
content-addressed bytes. They mean the PEP result is unavailable.

## Section 12. Non-executing core

PEP inherits the non-executing-core rule from the extension protocol
doctrine.

Core verification MUST NOT execute PEP verifiers, diff engines, grammar
parsers, invariant checkers, migration tools, source compilers, proof
checkers, or conformance suites. Core verification validates signed byte
graphs. PEP-aware tooling evaluates the evolution body under policy and,
if accepted, emits another signed/content-addressed witness.

This is the difference between:

```text
valid substrate artifact
```

and:

```text
admitted protocol successor
```

The first is core. The second is an extension witness.

## Section 13. Self-evolution and bootstrap

PEP is designed to host its own successors without circularity.

Example:

```text
pep-v1 spec bytes             -> pep-v1 spec CID
pep-v2 spec bytes             -> pep-v2 spec CID
old catalog containing pep-v1 -> old catalog CID
new catalog containing pep-v2 -> new catalog CID
PEP body(old, new, delta, evidence)
  -> TDP witness under pep-v1 policy
```

The witness is not inside the body it discharges. It is an outer result
over the body. A later parent claim may reference the witnessed root.

The first PEP adoption may be bootstrapped by sidecar attestation or a
proof bundle that references:

```text
.proof format spec
.proof grammar
.proof invariants
.proof verifier identity
.proof policy
canonical fixture .proof files
  -> proof-format conformance witnesses
  -> protocol adoption witness
```

This answers the operational question:

```text
does this program implement the proof protocol?
```

without forcing the core verifier to execute the conformance suite.

## Section 14. Protocol PR binding

The operational consequence of PEP is that protocol pull requests become
proof-bearing artifacts.

A protocol PR that changes any cataloged protocol artifact SHOULD carry,
or be able to deterministically produce, a PEP bundle:

```text
base catalog CID
candidate catalog CID
catalog diff CID
changed spec CIDs
PEP body-claim CID
required grammar/invariant/conformance/migration evidence
  -> protocol evolution witness root
```

The PR diff is no longer the authority. The diff is one input to the
evolution body. The load-bearing artifact is the witnessed root that says
the candidate catalog is an admitted successor under policy.

This turns the merge question from:

```text
does this protocol PR look right?
```

into:

```text
does this PR produce an accepted protocol evolution witness from the
currently pinned catalog CID to the candidate catalog CID?
```

A protected branch MAY require a PEP witness before merging any change
that modifies:

- a cataloged protocol spec;
- the protocol catalog;
- a grammar used by cataloged protocol bodies;
- an invariant set used by cataloged protocol bodies;
- a conformance corpus used for protocol admission;
- verifier, parser, checker, or policy identity used for protocol
  admission;
- root signing or key-rotation material.

A PEP-capable merge gate SHOULD fail closed when the PR changes protocol
bytes but does not provide an evolution body. The failure does not say
the PR is bad. It says the PR is not yet protocol-admissible.

This is the point at which protocol evolution becomes data:

```text
draft PR -> candidate catalog -> PEP body -> witness/refusal
```

The same shape supports approval, refusal, pinning, supersession, and
forking:

```text
consumer policy accepts witness root A
consumer policy refuses witness root B
consumer pins old catalog until migration witness C exists
consumer accepts fork catalog D under a different signer policy
```

Protocol governance remains human where policy says it is human. The
human decision becomes a signed, content-addressed input to the evidence
DAG rather than an out-of-band story about the bytes.

## Section 15. Evolution-chain validity

The PR witness is one edge:

```text
fromCatalogCid -> toCatalogCid
```

More precisely, it is a witnessed implication:

```text
accepted(fromCatalogCid, policyCid)
and admitted_successor(fromCatalogCid, toCatalogCid, policyCid)
  -> accepted(toCatalogCid, policyCid)
```

Once protocol PRs emit PEP witnesses, the protocol's history is no
longer only a Git branch, release series, or prose changelog. It is a
content-addressed graph of admitted catalog transitions.

```text
catalog A --PEP witness--> catalog B --PEP witness--> catalog C
        \                                      \
         \--PEP witness under policy X--> catalog D
```

An upgrade is therefore a path query:

```text
given pinned catalog P
and target catalog T
and policy Pol
is there a path of accepted PEP witness roots from P to T?
```

If yes, a consumer can adopt `T` under `Pol`. If no, the consumer has not
received an admissible upgrade path, even if `T` exists, is signed, and
has a valid catalog CID.

This gives Sugar protocol evolution the same substrate shape as every
other obligation:

```text
every catalog version is a CID
every admitted transition is an edge
every edge has a witness root
every upgrade is a witnessed path
every fork is a branch in the evidence graph
every refusal is a signed explanation for why no edge was admitted
```

The current pinned catalog is not a social convention. It is the root
from which this consumer is willing to search. A different consumer may
pin a different root, accept a different signer policy, refuse a breaking
transition, or follow a fork. The graph is shared. The admissible path is
policy-selected.

This is the deeper consequence of PEP:

```text
protocol evolution is not a sequence of trusted announcements.
protocol evolution is a witnessed reachability relation over catalog CIDs.
```

### Section 15.1 Path compression

A later witness MAY summarize a chain:

```text
catalog A -> catalog B -> catalog C -> catalog D
```

as a witnessed path root:

```text
path(A, D, [witnessAB, witnessBC, witnessCD], policyCid)
  -> pathWitnessCid
```

Parent claims SHOULD reference the strongest witnessed root they rely on.
If the parent only needs byte identity for `D`, it references `D`. If it
needs the fact that `D` is reachable from `A` under policy, it references
`pathWitnessCid`.

Path compression is an extension result over already-verifiable PEP
edges. It does not alter the underlying catalog CIDs or require core
verification to understand upgrade semantics.

### Section 15.2 Supersession

PEP does not require linear history.

A catalog transition can be superseded by a later transition, refusal, or
policy update:

```text
witnessAB accepted under policy v1
refusalAB accepted under policy v2
witnessAC accepted under policy v2
```

This does not rewrite history. It adds new signed facts to the evidence
graph. Consumers decide which path is admissible under their current
policy.

### Section 15.3 Forks

A fork is not a protocol catastrophe. It is a graph branch:

```text
catalog A -> catalog B
catalog A -> catalog F
```

If both edges are witnessed under different policies, both branches can
be valid for different consumers. Interoperability then becomes another
extension claim:

```text
catalog B artifact -> bridge/migration witness -> catalog F artifact
```

The fork does not need to be hidden. It can be named, signed, compared,
bridged, pinned, or refused.

## Section 16. Examples

### Section 16.1 Adding PEP as an extension protocol

The repository adds this spec under a new catalog key:

```text
protocol-evolution-protocol -> pep spec CID
```

The PEP body declares `changeClass = "extension-only"`. The computed
catalog delta has one added extension property. Policy requires:

- the new spec CID hashes to the spec bytes;
- the catalog CID recomputes;
- the PEP body conforms to the PEP grammar;
- the PEP invariant set discharges;
- the PEP verifier is accepted.

The TDP witness root becomes the admitted evolution root.

### Section 16.2 Changing `.proof` conformance rules

A successor catalog modifies the proof-file-format conformance rules.
The PEP body declares `changeClass = "migration-required"` if existing
fixtures or bundles must be regenerated.

Evidence may include:

- old `.proof` corpus CID;
- new `.proof` corpus CID;
- grammar conformance witnesses for canonical fixtures;
- verifier adoption witness for the Rust CLI;
- migration or refusal witnesses for fixtures that no longer conform.

The new `.proof` rules are not accepted because a document says they are
new. They are accepted because the successor catalog and its evidence DAG
are discharged under the evolution policy.

### Section 16.3 Protocol change requiring implementation fixes

A protocol update changes a language-dropper output invariant. Some Java
and TypeScript droppers must change.

The PEP body declares `changeClass = "migration-required"` and references
FRP receipts:

```text
old catalog CID
new catalog CID
changed ORP/FRP spec CIDs
java dropper FixReceipt root
typescript dropper FixReceipt root
post-migration adoption witness roots
  -> ProtocolEvolutionBodyClaim
  -> protocol evolution witness
```

The implementation changes remain FRP claims. The catalog succession is
the PEP claim.

## Section 17. Catalog property key

If cataloged, this extension protocol SHOULD use:

```text
protocol-evolution-protocol
```

Cataloging PEP pins the spec bytes and gives producers a stable key for
declaring support. It does not make PEP part of core verification.

## Section 18. Non-goals

- Make protocol evolution a core verifier behavior.
- Replace catalog CID versioning.
- Replace FRP, ORP, GCP, TDP, or proof-file conformance.
- Require every consumer to execute migration tools or conformance
  suites.
- Treat release notes, changelogs, or prose as positive evolution
  witnesses.
- Let a new protocol version silently change core behavior through an
  extension-only claim.

## Section 19. Open questions

1. Should PEP define one combined verifier for catalog diff, grammar
   conformance, invariant conformance, and policy admission, or should
   each remain a separately witnessed root?
2. Should `ProtocolAdoptionBodyClaim` remain inside PEP, or become its
   own conformance/adoption extension protocol once the shape stabilizes?
3. Should draft protocol catalogs accept PEP witnesses signed by the v0
   foundation key, or require a separate working-group key?
4. Should `changeClass` be a closed enum in v0.1, or a policy-resolved
   extension field?
5. What is the minimal canonical corpus required before the Rust CLI can
   witness its own support for `.proof` plus PEP?

## Section 20. Citation

Cite as:

> Sugar Protocol Working Notes (2026). *Protocol Evolution Protocol
> (PEP)*. Draft extension protocol v0.1.0.
