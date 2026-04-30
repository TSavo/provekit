# ProvekIt: Memento Envelope Grammar (CDDL)

**Date:** 2026-04-30
**Status:** Specification. CDDL (RFC 8610) is normative; prose is exposition.
**Encoding:** JSON, canonicalized per `2026-04-29-universal-claim-envelope.md` §"CID construction" (sorted-keys, no whitespace, UTF-8). The CDDL below is interpreted against that JSON form.

## What this document specifies

Every memento ProvekIt produces is a `ClaimEnvelope` (defined in
`2026-04-29-universal-claim-envelope.md`). This spec adds **role**:
the application-level purpose a memento serves. Six roles are defined:
**catalog**, **property**, **bridge**, **verdict**, **audit**,
**deprecation**.

Roles are not separate top-level types. A role is a constrained
instance of the universal envelope, identified by:

- a specific `evidence.kind` value (or set of values for the verdict
  role),
- a body grammar specific to that variant,
- a derived rule for `bindingHash` and `propertyHash`,
- a permitted set of roles its `inputCids` may point at,
- a signature requirement.

The CDDL below is the protocol. A validator that accepts every
well-formed memento and rejects every malformed one is conforming.
The TypeScript code at `src/claimEnvelope/` is one reference
implementation among future possibly-many; where current TS conflicts
with this CDDL, the TS needs alignment, not the CDDL.

## Two layers of validation

CDDL covers **shape**: which keys exist, which types they are, which
fields are optional, which strings match a regexp.

CDDL cannot express:

- **REFERENT constraints.** "Memento A's `inputCids[0]` MUST be the
  CID of a memento whose `evidence.kind` is in some set." A
  validator resolves the CID against a memento store and checks the
  referent's role.
- **DERIVED constraints.** "`propertyHash` MUST equal
  `hash16(canonical(evidence.body.irFormula))`." A validator
  recomputes the hash and compares.
- **ORDERING constraints.** "`inputCids` MUST be lexicographically
  sorted." CDDL types `inputCids` as an array of CIDs but cannot
  require sort order.

Each role section labels its non-CDDL constraints explicitly:

- **REFERENT:** the field is a CID; the prose names which role(s)
  the referent must have.
- **DERIVED:** the field's value MUST equal a hash computed from
  other fields; the prose gives the hash construction.
- **ORDERING:** the field's elements MUST appear in a specific order.

A conforming validator runs CDDL acceptance first, then a post-pass
enforcing the labeled constraints. A memento that passes the CDDL
but fails a post-pass constraint is **invalid**.

## Master CDDL: shared types and the wrapper

```cddl
; ============================================================
; Master CDDL for memento envelope grammar
; ============================================================

; Reference: ProvekitIrFormula (rule name `ir-formula`) is defined in
; protocol/specs/2026-04-30-ir-formal-grammar.md and imported by name.

; ----- Scalars and CIDs --------------------------------------

hex16        = tstr .regexp "^[0-9a-f]{16}$"
hex32        = tstr .regexp "^[0-9a-f]{32}$"
cid          = hex32
binding-hash = hex16
property-hash = hex16

; ISO-8601 UTC timestamp with millisecond precision and trailing 'Z'.
iso8601      = tstr .regexp "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?Z$"

; producer-id format: <name>@<version>
producer-id  = tstr .regexp "^[A-Za-z][A-Za-z0-9_./:@-]*@[A-Za-z0-9._:+-]+$"

; ed25519 signature, base64-encoded.
ed25519-sig  = tstr .regexp "^[A-Za-z0-9+/]+=*$"

; ----- Wrapper verdict and version --------------------------

schema-version = "1"

verdict        = "holds" / "violated" / "decayed" / "undecidable" / "error"

; ----- The variant union ------------------------------------

evidence-variant =
    catalog-evidence
  / property-evidence
  / bridge-evidence
  / verdict-evidence
  / audit-evidence
  / deprecation-evidence

; The legacy-witness variant is retained at the wrapper layer for
; backward compatibility with pre-protocol mementos. It MUST NOT be
; produced for any of the six roles defined in this document. Its
; CDDL is in the universal-envelope spec, not here.

; ----- The wrapper itself -----------------------------------

claim-envelope = {
  schemaVersion:     schema-version,
  bindingHash:       binding-hash,
  propertyHash:      property-hash,
  verdict:           verdict,
  producedBy:        producer-id,
  producedAt:        iso8601,
  inputCids:         [* cid],          ; ORDERING: lex-sorted
  evidence:          evidence-variant,
  ? producerSignature: ed25519-sig,
  cid:               hex32
}
```

**ORDERING (wrapper):** `inputCids` MUST be lexicographically
ascending. Validators reject envelopes with unsorted `inputCids`.

**DERIVED (wrapper):** `cid` MUST equal `sha256-prefix-32` of the
canonicalized envelope with `cid` and `producerSignature` elided.
Computation is specified in `2026-04-29-universal-claim-envelope.md`
§"CID construction".

## Role taxonomy at a glance

| Role         | `evidence.kind` (CDDL `kind` literal)                                                                        | Signature                          | `inputCids` referent role(s)             |
|--------------|--------------------------------------------------------------------------------------------------------------|------------------------------------|------------------------------------------|
| catalog      | `"kit-catalog"`                                                                                              | REQUIRED swarm; OPTIONAL local     | bridge, property                          |
| property     | `"property-declaration"`                                                                                     | REQUIRED swarm; OPTIONAL local     | property                                  |
| bridge       | `"bridge"`                                                                                                   | REQUIRED swarm; OPTIONAL local     | property, catalog                         |
| verdict      | `"z3-model"` / `"z3-unsat"` / `"test-pass"` / `"test-fail"` / `"pattern-match"` / `"lint-pass"` / `"type-check-pass"` / `"mutation-witness"` | REQUIRED swarm; OPTIONAL local | property, verdict, bridge                 |
| audit        | `"workflow-run"`                                                                                             | REQUIRED swarm; OPTIONAL local     | audit, verdict, property, bridge          |
| deprecation  | `"retirement"`                                                                                               | REQUIRED swarm; OPTIONAL local     | property                                  |

The signature column applies a single rule across roles: signatures
are REQUIRED for swarm-distributed mementos and OPTIONAL for
local-only ones. A swarm-distributed memento is one that crosses a
trust boundary (published, fetched from a registry, used as a leaf
in a downstream proofHash). Verifiers MUST reject unsigned mementos
imported from a swarm; verifiers MAY accept unsigned mementos
produced and consumed in-process.

## Role: CatalogMemento

A library or kit's published list of `(name, role, cid)` entries.
The catalog's CID is what `package.json`'s `provekit.proofHash`
field points at. Consumers fetch the catalog, verify the signature,
and walk `inputCids` to enumerate the propertyHashes the library has
committed to upholding. The catalog IS the kit's identity at the
semantic layer; the version string is at most a human-readable
nickname for the catalog CID.

```cddl
catalog-evidence = {
  kind:    "kit-catalog",
  schema:  cid,
  body:    catalog-body
}

catalog-body = {
  kitName:    tstr,
  kitVersion: tstr,
  entries:    [+ catalog-entry]      ; one or more entries
}

catalog-entry = {
  name: tstr,
  role: ("bridge" / "property"),
  cid:  cid
}
```

**Wrapper-field constraints (CDDL-checkable via the shared rules):**

- `verdict = "holds"` (the catalog asserts the kit publishes these
  properties).

**REFERENT constraints (post-CDDL validator):**

- Each `entries[*].cid` MUST resolve to a memento whose role is
  **bridge** OR **property**. Validators look up each CID and
  reject the catalog if any referent has a different role.

**DERIVED constraints (post-CDDL validator):**

- `bindingHash == hash16(canonical("<kitName>@<kitVersion>"))` where
  `kitName` and `kitVersion` are taken from `evidence.body`.
- `propertyHash == hash16(canonical("kit-catalog-root:<kitName>@<kitVersion>"))`.
- The multiset of `evidence.body.entries[*].cid` MUST equal the
  multiset of `inputCids`. (CDDL types each separately; the equality
  is post-CDDL.)

**ORDERING constraints (post-CDDL validator):**

- `inputCids` MUST be lex-sorted (inherited from the wrapper rule).
- `entries` MAY appear in any order; the validator sorts a copy by
  `cid` before computing the multiset comparison above.

**Signature.** REQUIRED for swarm-distributed catalogs. The kit
author's public key is published as `package.json`.`provekit.publicKey`
(SPKI base64). An unsigned catalog cannot serve as the root of a
downstream consumer's proofHash.

### Worked example

```json
{
  "schemaVersion": "1",
  "bindingHash": "8f3b1c9a2d7e6f04",
  "cid": "ab12cd34ef56789012345678abcdef01",
  "evidence": {
    "body": {
      "entries": [
        {
          "cid": "3e4f5a6b7c8d9e0f1a2b3c4d5e6f7080",
          "name": "global.parseInt",
          "role": "bridge"
        },
        {
          "cid": "5a6b7c8d9e0f1a2b3c4d5e6f70809010",
          "name": "global.parseFloat",
          "role": "bridge"
        },
        {
          "cid": "7c8d9e0f1a2b3c4d5e6f708090102030",
          "name": "Math.abs-non-negative",
          "role": "property"
        }
      ],
      "kitName": "@provekit/ts-kit",
      "kitVersion": "1.0.0"
    },
    "kind": "kit-catalog",
    "schema": "00000000000000000000000000000c01"
  },
  "inputCids": [
    "3e4f5a6b7c8d9e0f1a2b3c4d5e6f7080",
    "5a6b7c8d9e0f1a2b3c4d5e6f70809010",
    "7c8d9e0f1a2b3c4d5e6f708090102030"
  ],
  "producedAt": "1970-01-01T00:00:00.000Z",
  "producedBy": "ts-kit@1.0",
  "producerSignature": "MEUCIQDxxx==",
  "propertyHash": "1d2e3f4a5b6c7d8e",
  "verdict": "holds"
}
```

## Role: PropertyMemento

Publishes one canonical IR claim and binds its `propertyHash` to
the formula bytes. The body carries the canonical IR formula (per
`2026-04-30-ir-formal-grammar.md`) plus the producer's authoring
metadata. The IR formula's sha256-prefix-16 equals the wrapper's
`propertyHash` field. This memento makes the propertyHash
*resolvable*: given a CID, a consumer can fetch the memento and
read the formula it commits to. Without a property memento, a
propertyHash is a hash with no published preimage.

A property memento says "I, the producer, commit to this
propertyHash naming this exact IR formula." It does NOT say the
property holds for any particular code; that is the verdict role's
job.

```cddl
property-evidence = {
  kind:    "property-declaration",
  schema:  cid,
  body:    property-body
}

property-body = {
  propertyName: tstr,
  irFormula:    ir-formula,        ; imported from 2026-04-30-ir-formal-grammar.md
  authoring:    authoring-block
}

authoring-block =
    llm-authoring
  / kit-author-authoring
  / fix-loop-authoring

llm-authoring = {
  producerKind: "llm",
  llm:          tstr,
  llmVersion:   tstr,
  promptCid:    cid,
  confidence:   float .ge 0.0 .le 1.0,
  ? rationale:  tstr
}

kit-author-authoring = {
  producerKind: "kit-author",
  author:       producer-id,
  ? note:       tstr
}

fix-loop-authoring = {
  producerKind:          "fix-loop",
  originatingBugSummary: tstr,
  patchSha:              (tstr / null)
}
```

**Wrapper-field constraints (CDDL-checkable via shared rules):**

- `verdict = "holds"`.

**REFERENT constraints (post-CDDL validator):**

- Every entry of `inputCids` MUST resolve to a memento whose role
  is **property**. Validators reject property mementos whose
  `inputCids` reach a non-property role.

**DERIVED constraints (post-CDDL validator):**

- `propertyHash == hash16(canonical(evidence.body.irFormula))`.
  The IR formula is hashed under the canonical rules of the
  envelope spec (sorted-keys JSON, no whitespace, UTF-8).
- `bindingHash == hash16(canonical({producerId, propertyName,
  irFormulaCid}))` where `producerId = wrapper.producedBy`,
  `propertyName = evidence.body.propertyName`,
  `irFormulaCid = wrapper.propertyHash`. Locking the construction
  makes property bindingHashes reproducible across implementations.

**Signature.** REQUIRED for swarm distribution; OPTIONAL local.

### Worked example

```json
{
  "schemaVersion": "1",
  "bindingHash": "a1b2c3d4e5f60718",
  "cid": "1234567890abcdef1234567890abcdef",
  "evidence": {
    "body": {
      "authoring": {
        "confidence": 0.93,
        "llm": "claude-opus",
        "llmVersion": "4-7",
        "producerKind": "llm",
        "promptCid": "0000000000000000000000000000beef",
        "rationale": "abs(parseInt(x)) is non-negative when parseInt converges"
      },
      "irFormula": {
        "kind": "forall-formula",
        "var": { "kind": "lambda", "varName": "x", "sort": { "kind": "primitive-sort", "name": "Int" }, "body": {
          "kind": "implies-formula",
          "antecedent": { "kind": "atomic-formula", "predicate": "is_finite", "args": [
            { "kind": "ctor-term", "ctor": "parseInt", "args": [{ "kind": "var-term", "name": "x" }] }
          ]},
          "consequent": { "kind": "atomic-formula", "predicate": "≥", "args": [
            { "kind": "ctor-term", "ctor": "abs", "args": [
              { "kind": "ctor-term", "ctor": "parseInt", "args": [{ "kind": "var-term", "name": "x" }] }
            ]},
            { "kind": "const-term", "value": 0 }
          ]}
        }}
      },
      "propertyName": "parseInt-non-negative"
    },
    "kind": "property-declaration",
    "schema": "00000000000000000000000000000c02"
  },
  "inputCids": [],
  "producedAt": "2026-04-30T12:00:00.000Z",
  "producedBy": "llm:claude-opus@4-7",
  "producerSignature": "MEUCIQ==",
  "propertyHash": "f0e1d2c3b4a59687",
  "verdict": "holds"
}
```

The exact IR-formula JSON shape is governed by the IR Formal Grammar
spec; the example uses its rule names as a reference. A conforming
validator imports the IR grammar's `ir-formula` rule and applies it
to `evidence.body.irFormula` directly.

## Role: BridgeMemento

Declares that a host-language symbol is the surface realization of
a deeper-layer published contract. A consumer-side bridge says "my
call site at X depends on the library's propertyHash Y"; a kit-side
bridge says "global.parseInt is bridged to V8's published parseInt
contract." The bridge composes by hash: walking the bridge means
traversing to the deeper layer (a different codebase, possibly a
different language). The framework MINTS the bridge; walking is
the auditor's job.

```cddl
bridge-evidence = {
  kind:    "bridge",
  schema:  cid,
  body:    bridge-body
}

bridge-body = {
  sourceSymbol:      tstr,
  sourceLayer:       tstr,
  targetContractCid: cid,
  targetLayer:       tstr,
  irArgSorts:        [* sort-ref],
  irReturnSort:      sort-ref,
  ? notes:           tstr
}

; A SortRef is either a builtin primitive sort name (the literal string
; "Int"/"Bool"/"String"/...) OR a Sort value carrying the canonical
; sort grammar (per ir-formal-grammar.md §Sort). The discriminator is:
; tstr = primitive name; map = full Sort grammar.
sort-ref = tstr / sort
sort = primitive-sort / bitvec-sort / set-sort / tuple-sort / function-sort
primitive-sort  = { kind: "primitive", name: tstr }
bitvec-sort     = { kind: "bitvec",    width: uint }
set-sort        = { kind: "set",       element: sort }
tuple-sort      = { kind: "tuple",     elements: [* sort] }
function-sort   = { kind: "function",  domain: [* sort], range: sort }
```

**Wrapper-field constraints (CDDL-checkable via shared rules):**

- `verdict = "holds"`.
- `inputCids` MUST contain exactly one entry. CDDL `[* cid]` allows
  zero-or-more; the post-pass tightens to exactly-one for this role.

**REFERENT constraints (post-CDDL validator):**

- `inputCids[0]` MUST resolve to a memento whose role is **property**
  OR **catalog** (the deeper-layer contract).
- `inputCids[0] == evidence.body.targetContractCid`. The redundancy
  is structural so a chain walker can enumerate edges without
  parsing variant bodies.

**DERIVED constraints (post-CDDL validator):**

- `bindingHash == hash16(canonical({sourceLayer, sourceSymbol}))`.
- `propertyHash == hash16(canonical("bridge:<sourceSymbol>"))`.

**ORDERING constraints:**

- The `notes` field MUST be omitted entirely when the bridge has
  no notes. The CDDL `?` operator and the canonicalizer's
  "omit undefined keys" rule together preserve this.

**Signature.** REQUIRED for swarm-distributed bridges (any bridge
whose CID appears in a published catalog or in another party's
memento store). OPTIONAL for consumer-local bridges.

### Worked example (bridge with notes)

```json
{
  "schemaVersion": "1",
  "bindingHash": "3e4f5a6b7c8d9e0f",
  "cid": "3e4f5a6b7c8d9e0f1a2b3c4d5e6f7080",
  "evidence": {
    "body": {
      "notes": "Bridges to V8's published parseInt contract",
      "sourceLayer": "ts-kit@1.0",
      "sourceSymbol": "global.parseInt",
      "targetContractCid": "00000000000000000000000000000000",
      "targetLayer": "V8@12.4 parseInt (placeholder; real CID when V8 publishes)"
    },
    "kind": "bridge",
    "schema": "00000000000000000000000000000c03"
  },
  "inputCids": [
    "00000000000000000000000000000000"
  ],
  "producedAt": "1970-01-01T00:00:00.000Z",
  "producedBy": "ts-kit@1.0",
  "producerSignature": "MEUCIQ==",
  "propertyHash": "5a6b7c8d9e0f1a2b",
  "verdict": "holds"
}
```

### Worked example (bridge without notes)

```json
{
  "schemaVersion": "1",
  "bindingHash": "5a6b7c8d9e0f1a2b",
  "cid": "5a6b7c8d9e0f1a2b3c4d5e6f70809010",
  "evidence": {
    "body": {
      "sourceLayer": "ts-kit@1.0",
      "sourceSymbol": "global.parseFloat",
      "targetContractCid": "00000000000000000000000000000000",
      "targetLayer": "V8@12.4 parseFloat"
    },
    "kind": "bridge",
    "schema": "00000000000000000000000000000c03"
  },
  "inputCids": [
    "00000000000000000000000000000000"
  ],
  "producedAt": "1970-01-01T00:00:00.000Z",
  "producedBy": "ts-kit@1.0",
  "producerSignature": "MEUCIQ==",
  "propertyHash": "9e0f1a2b3c4d5e6f",
  "verdict": "holds"
}
```

`notes` is absent: not `null`, not `""`.

## Role: VerdictMemento

Asserts a solver- or checker-derived verdict against a propertyHash
applied to a binding. "Z3 found this property unsat for this code
shape." "Vitest passed this test for this binding." "Datalog matched
this pattern in this code." A verdict is the leaf that gives a
propertyHash *operational truth* for some specific code.

A verdict memento differs from a property memento: a property
memento publishes the formula; a verdict memento applies the
formula to code and reports the answer.

```cddl
verdict-evidence =
    z3-model-evidence
  / z3-unsat-evidence
  / test-pass-evidence
  / test-fail-evidence
  / pattern-match-evidence
  / lint-pass-evidence
  / type-check-pass-evidence
  / mutation-witness-evidence

z3-model-evidence = {
  kind:    "z3-model",
  schema:  cid,
  body:    z3-model-body
}

z3-model-body = {
  smtLibInput:    tstr,
  z3Verdict:      "sat",
  model:          tstr,
  counterexample: { * tstr => any },
  z3RunMs:        uint
}

z3-unsat-evidence = {
  kind:    "z3-unsat",
  schema:  cid,
  body:    z3-unsat-body
}

z3-unsat-body = {
  smtLibInput: tstr,
  z3Verdict:   "unsat",
  ? proof:     tstr,
  z3RunMs:     uint
}

test-pass-evidence = {
  kind:    "test-pass",
  schema:  cid,
  body:    test-pass-body
}

test-pass-body = {
  runner:        tstr,
  runnerVersion: tstr,
  testId:        tstr,
  durationMs:    uint,
  ? stdout:      tstr
}

test-fail-evidence = {
  kind:    "test-fail",
  schema:  cid,
  body:    test-fail-body
}

test-fail-body = {
  runner:          tstr,
  runnerVersion:   tstr,
  testId:          tstr,
  durationMs:      uint,
  ? stdout:        tstr,
  ? failureDetail: tstr
}

pattern-match-evidence = {
  kind:    "pattern-match",
  schema:  cid,
  body:    pattern-match-body
}

pattern-match-body = {
  pattern:         tstr,
  matchedNodes:    [* tstr],
  matchedCaptures: { * tstr => any },
  polarity:        ("defect-detector" / "feature-detector")
}

lint-pass-evidence = {
  kind:    "lint-pass",
  schema:  cid,
  body:    lint-pass-body
}

lint-pass-body = {
  linter:        tstr,
  linterVersion: tstr,
  rulesetHash:   cid,
  warnings:      0
}

type-check-pass-evidence = {
  kind:    "type-check-pass",
  schema:  cid,
  body:    type-check-pass-body
}

type-check-pass-body = {
  checker:           tstr,
  checkerVersion:    tstr,
  symbol:            tstr,
  ? resolvedType:    tstr,
  diagnosticsClean:  true
}

mutation-witness-evidence = {
  kind:    "mutation-witness",
  schema:  cid,
  body:    mutation-witness-body
}

mutation-witness-body = {
  testCid:         cid,
  mutationCid:     cid,
  failsOnOriginal: bool,
  passesOnFixed:   bool
}
```

**Wrapper-field constraints (joint CDDL/post-pass):**

The wrapper `verdict` and the variant body's verdict MUST satisfy
the pairing rules below. CDDL cannot express joint constraints
across the wrapper field and the evidence body, so the pairing
rules are post-CDDL.

**PAIRING constraints (post-CDDL validator):**

| `evidence.kind`     | wrapper `verdict` MUST be                                           |
|---------------------|---------------------------------------------------------------------|
| `z3-unsat`          | `"holds"`                                                           |
| `z3-model`          | `"violated"` (sat-with-model on the negated property is refutation) |
| `test-pass`         | `"holds"`                                                           |
| `test-fail`         | `"violated"`                                                        |
| `lint-pass`         | `"holds"`                                                           |
| `type-check-pass`   | `"holds"`                                                           |
| `pattern-match`     | `"violated"` if `body.polarity = "defect-detector"`; `"holds"` if `body.polarity = "feature-detector"` |
| `mutation-witness`  | `"holds"` iff `body.failsOnOriginal == true && body.passesOnFixed == true`; `"violated"` otherwise |

A producer MUST NOT emit a verdict memento whose pairing is invalid.
A validator rejects on mismatched pairs.

The wrapper `verdict` MAY also be `"decayed"`, `"undecidable"`, or
`"error"` independent of the variant. These three values report a
non-result, not an outcome on the property; the variant body MUST
still pair correctly when present, so a `z3-unsat` body MUST NOT be
emitted with `"undecidable"`. Producers who need to record a
non-result MUST use the wrapper verdict alone with no body that
asserts an outcome (or use the legacy-witness variant for
transitional cases, which is outside the role grammar here).

**REFERENT constraints (post-CDDL validator):**

- Every entry of `inputCids` MUST resolve to a memento whose role
  is **property**, **verdict**, or **bridge**.

**Signature.** REQUIRED for swarm-distributed verdicts. OPTIONAL
for local-only ones.

### Worked example (z3-unsat -> holds)

```json
{
  "schemaVersion": "1",
  "bindingHash": "9876543210fedcba",
  "cid": "fedcba9876543210fedcba9876543210",
  "evidence": {
    "body": {
      "smtLibInput": "(declare-const x Int) (assert (>= (abs x) 0)) (check-sat)",
      "z3RunMs": 12,
      "z3Verdict": "unsat"
    },
    "kind": "z3-unsat",
    "schema": "00000000000000000000000000000c04"
  },
  "inputCids": [
    "1234567890abcdef1234567890abcdef"
  ],
  "producedAt": "2026-04-30T12:05:00.000Z",
  "producedBy": "z3-symbolic@4.13.4",
  "propertyHash": "f0e1d2c3b4a59687",
  "verdict": "holds"
}
```

### Worked example (test-fail -> violated)

```json
{
  "schemaVersion": "1",
  "bindingHash": "9876543210fedcba",
  "cid": "deadbeefcafef00ddeadbeefcafef00d",
  "evidence": {
    "body": {
      "durationMs": 47,
      "failureDetail": "Expected abs(parseInt('-3')) >= 0; got NaN",
      "runner": "vitest",
      "runnerVersion": "1.6.0",
      "stdout": "FAIL src/parseInt.test.ts > non-negative",
      "testId": "parseInt > non-negative"
    },
    "kind": "test-fail",
    "schema": "00000000000000000000000000000c05"
  },
  "inputCids": [
    "1234567890abcdef1234567890abcdef"
  ],
  "producedAt": "2026-04-30T12:06:00.000Z",
  "producedBy": "vitest@1.6.0",
  "propertyHash": "f0e1d2c3b4a59687",
  "verdict": "violated"
}
```

## Role: AuditMemento

A Stage's input/output trace, captured for the cache-key purposes
of the workflow runner (`protocol/specs/2026-04-29-workflows-as-first-class-primitive.md`).
An audit memento says "I ran this stage with these inputs and got
this output, at this time." The framework uses audit mementos for
deterministic replay and for reconstructing the provenance chain
of a workflow run; they do NOT carry truth claims about code, only
claims about the stage's execution.

```cddl
audit-evidence = {
  kind:    "workflow-run",
  schema:  cid,
  body:    audit-body
}

audit-body = {
  workflowName:       tstr,
  workflowCid:        cid,
  inputCanonicalForm: { * tstr => any },
  output:             any
}
```

**Wrapper-field constraints (post-CDDL validator):**

- `verdict` MUST be `"holds"` for a successful stage, `"error"` for
  a mechanically-failed one. No other verdict values are defined
  for this role. CDDL types `verdict` as the universal closed enum;
  the post-pass narrows to `{holds, error}` for the audit role.

**REFERENT constraints (post-CDDL validator):**

- Every entry of `inputCids` MUST resolve to a memento whose role is
  **audit**, **verdict**, **property**, or **bridge**. An audit
  memento MUST NOT point at catalog mementos directly (the catalog
  is a publication artifact, not an input to a stage).

**DERIVED constraints (post-CDDL validator):**

- `propertyHash == hash16(canonical(evidence.body.workflowCid))`.
  The wrapper-level `propertyHash` MUST agree with the body's
  workflow identity.
- `bindingHash == hash16(canonical(evidence.body.inputCanonicalForm))`.
  The wrapper-level `bindingHash` MUST agree with the input
  identity, which is what the cache lookup keys against.

**Signature.** REQUIRED for swarm-distributed audit mementos.
OPTIONAL for local audits.

### Worked example

```json
{
  "schemaVersion": "1",
  "bindingHash": "70809010a0b0c0d0",
  "cid": "abc123def456789012345678abc123de",
  "evidence": {
    "body": {
      "inputCanonicalForm": {
        "candidateCid": "1234567890abcdef1234567890abcdef",
        "sourceFile": "src/parseInt.ts",
        "sourceLineRange": [12, 18]
      },
      "output": {
        "formulaCid": "f0e1d2c3b4a59687f0e1d2c3b4a59687",
        "lifterRunMs": 340
      },
      "workflowCid": "11223344556677889900aabbccddeeff",
      "workflowName": "formulate-via-lifter"
    },
    "kind": "workflow-run",
    "schema": "00000000000000000000000000000c06"
  },
  "inputCids": [
    "1234567890abcdef1234567890abcdef"
  ],
  "producedAt": "2026-04-30T12:04:00.000Z",
  "producedBy": "workflow:formulate-via-lifter@bafy12abc",
  "propertyHash": "11223344556677889900aabbccddeeff",
  "verdict": "holds"
}
```

## Role: DeprecationMemento

Retires a previously-published propertyHash with a reason and a
timestamp. After a deprecation memento exists, consumers walking
the catalog diff between two library versions MUST treat the
retired propertyHash as removed even if it remains nominally
present in the catalog (the deprecation supersedes residual
references).

A deprecation memento is the only mechanism in the framework for
*authoritatively withdrawing* a previously-asserted claim. Without
it, a malicious or careless author could silently drop a
propertyHash from a future catalog and downstream consumers would
only see the diff, not the rationale.

```cddl
deprecation-evidence = {
  kind:    "retirement",
  schema:  cid,
  body:    retirement-body
}

retirement-body = {
  retiredPropertyCid: cid,
  retiredAt:          iso8601,
  reason:             tstr,
  ? successor:        cid
}
```

**Wrapper-field constraints:**

- `verdict = "holds"`.
- `inputCids` MUST contain exactly one entry (post-CDDL narrowing
  of the wrapper's `[* cid]`).

**REFERENT constraints (post-CDDL validator):**

- `inputCids[0]` MUST resolve to a memento whose role is
  **property**.
- `evidence.body.successor`, when present, MUST resolve to a memento
  whose role is **property**.
- `evidence.body.retiredPropertyCid == inputCids[0]`. Validators
  reject deprecations whose body and inputCids disagree.

**DERIVED constraints (post-CDDL validator):**

- `bindingHash == hash16(canonical(evidence.body.retiredPropertyCid))`.
- `propertyHash == hash16(canonical("retirement:" || retiredPropertyHash))`
  where `retiredPropertyHash` is the `propertyHash` field of the
  retired property memento (looked up via `retiredPropertyCid`).

**SIGNING-KEY constraint (post-CDDL validator):**

- The signing key for `producerSignature` MUST be the producer key
  of the retired property memento, OR a successor key declared via
  the universal envelope's key-rotation chain. A retirement signed
  by an unrelated key is rejected: it would let any party retire
  any propertyHash.

**Signature.** REQUIRED for swarm-distributed deprecations.
OPTIONAL for local deprecations.

### Worked example

```json
{
  "schemaVersion": "1",
  "bindingHash": "abcdef0123456789",
  "cid": "deadbeef0011223344556677deadbeef",
  "evidence": {
    "body": {
      "reason": "property does not hold under parseInt(NaN); replacement narrows to is_finite domain",
      "retiredAt": "2026-05-12T09:00:00.000Z",
      "retiredPropertyCid": "1234567890abcdef1234567890abcdef",
      "successor": "abcdef1234567890abcdef1234567890"
    },
    "kind": "retirement",
    "schema": "00000000000000000000000000000c07"
  },
  "inputCids": [
    "1234567890abcdef1234567890abcdef"
  ],
  "producedAt": "2026-05-12T09:00:00.000Z",
  "producedBy": "llm:claude-opus@4-7",
  "producerSignature": "MEUCIQ==",
  "propertyHash": "0011223344556677",
  "verdict": "holds"
}
```

## The full role-permission matrix

CDDL types `inputCids` as `[* cid]`. The post-CDDL referent table:

```
catalog.inputCids     -> { bridge, property }                    REQUIRED non-empty
property.inputCids    -> { property }                            MAY be empty
bridge.inputCids      -> { property, catalog }                   exactly 1 entry
verdict.inputCids     -> { property, verdict, bridge }           MAY be empty
audit.inputCids       -> { audit, verdict, property, bridge }    MAY be empty
deprecation.inputCids -> { property }                            exactly 1 entry
```

A role pointing at a forbidden role in `inputCids` is malformed.

## Acceptance test

A conforming validator implements:

```
validate(memento) :=
  let envelope = parse(memento)
  if !cddl_accept(envelope, claim-envelope) then return reject(SHAPE)
  if !ordering_check(envelope) then return reject(ORDERING)
  if !derived_check(envelope) then return reject(DERIVED)
  if !referent_check(envelope, store) then return reject(REFERENT)
  if !pairing_check(envelope) then return reject(PAIRING)        ; verdict role only
  if !signing_key_check(envelope, key_chain) then return reject(SIGNATURE)
  return accept
```

Where:

- `cddl_accept` is RFC 8610 CDDL validation against the union
  rule for the memento's role.
- `ordering_check` enforces lex-sort on `inputCids`.
- `derived_check` recomputes `cid`, role-specific `bindingHash`
  and `propertyHash` per the DERIVED constraints, and rejects on
  mismatch.
- `referent_check` looks up each `inputCids` entry in the local
  memento store (or the declared external-roots set) and enforces
  the role-permission matrix.
- `pairing_check` enforces the verdict role's
  wrapper-verdict-vs-variant pairing rules.
- `signing_key_check` enforces the deprecation role's signing-key
  chain and the universal swarm-import signature requirement.

The grammar is correct when:

1. Every memento minted by `build-ts-kit-catalog.ts` and the bridge
   demos in `scripts/cross-language-demo/` validates with `accept`.
2. A test corpus of malformed mementos (sort-broken inputCids,
   wrong-role referents, pairing violations, hash mismatches,
   missing signatures, wrong signing keys, empty catalogs)
   validates with `reject(<specific code>)`.
3. The same logical claim emitted by two reference implementations
   in different host languages produces envelopes that differ only
   in `producedBy` and `producedAt`; their CIDs differ accordingly,
   but both validate with `accept`.

## Implementation conformance status

The protocol is the CDDL above. The TypeScript reference
implementation at `src/claimEnvelope/` does not yet conform to the
protocol in several places. Each item below names the gap and the
work needed to close it.

- **Catalog role.** Protocol requires
  `evidence.kind = "kit-catalog"` with the body matching
  `catalog-body`. Current TS emits `evidence.kind = "legacy-witness"`
  with the descriptor embedded as a JSON-encoded string in
  `body.rawWitness` (see
  `scripts/cross-language-demo/kit-catalog/build-ts-kit-catalog.ts`).
  Alignment work: introduce the `kit-catalog` evidence variant in
  `src/claimEnvelope/types.ts` and switch the catalog builder.
- **Property role.** Protocol requires
  `evidence.kind = "property-declaration"` with a `property-body`
  carrying a structured `irFormula` and a tagged `authoring` block.
  Current TS encodes property mementos as `llm-proposal`
  (LLM-authored) or `legacy-witness` (kit/fix-loop-authored).
  Alignment work: introduce the `property-declaration` variant;
  migrate LLM-producer code to the `llm-authoring` shape; migrate
  kit/fix-loop producers similarly.
- **Deprecation role.** Protocol requires
  `evidence.kind = "retirement"` with a structured `retirement-body`.
  Current TS has no envelope-level deprecation encoding at all;
  retirement information lives on disk in `StoredInvariant.retired`
  (`src/fix/runtime/invariantStore.ts`) and does not travel through
  the memento store. Alignment work: introduce the `retirement`
  variant, add a `mintDeprecation` helper paralleling `mintBridge`,
  extend the catalog-diff workflow to surface deprecation mementos
  as authoritative removals.
- **Verdict role.** Protocol requires the wrapper `verdict` and the
  variant body's verdict to satisfy the pairing rules. Current TS
  types do not enforce the joint constraint at the type level: a
  producer can emit `verdict: "holds"` paired with
  `kind: "test-fail"` and the type-checker accepts it. Alignment
  work: refine the TS types into a discriminated union keyed
  jointly on `verdict` and `evidence.kind`, or add a runtime
  validator that rejects mismatched pairs.
- **Verdict role, `pattern-match` polarity.** Protocol adds
  `polarity: ("defect-detector" / "feature-detector")` to the
  pattern-match body so the pairing rule is well-defined. Current
  TS lacks the `polarity` field. Alignment work: add the field to
  `PatternMatchEvidence.body` and route producers to populate it.
- **Property `bindingHash` construction.** Protocol locks the
  construction to
  `hash16(canonical({producerId, propertyName, irFormulaCid}))`.
  Current TS computes `bindingHash` per-producer with no
  cross-producer agreement. Alignment work: implement the locked
  construction in the property-declaration mint helper.
- **Audit `bindingHash` and `propertyHash` constructions.**
  Protocol locks both to canonical hashes of body fields. Current
  TS treats them as caller-supplied. Alignment work: derive both
  inside the workflow runner instead of accepting caller input.
- **Bridge role.** Current TS `BridgeEvidence` body shape and the
  `mintBridge` helper conform to the protocol. No alignment work
  needed except removing the `notes: null` corner-case allowance
  if the type permits it.
- **`legacy-witness` variant.** The variant exists in the universal
  envelope for backward compatibility with pre-protocol mementos.
  Producing a `legacy-witness` memento for any of the six roles
  defined here is a protocol violation. Alignment work: audit
  callers of `mintLegacyWitness` and route each to the appropriate
  role-specific variant.

These items name what the TypeScript reference implementation must
become to conform. Implementations in other host languages (Rust,
Go, C++) MUST conform to the CDDL above as written, regardless of
the TypeScript implementation's current state.
