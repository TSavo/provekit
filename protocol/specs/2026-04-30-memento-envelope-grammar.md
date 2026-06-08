# Sugar: Memento Envelope Grammar (CDDL)

**Date:** 2026-04-30
**Status:** Specification (v1.1 flat shape). CDDL (RFC 8610) is normative; prose is exposition.
**Encoding:** JSON, canonicalized per `2026-04-29-universal-claim-envelope.md` §"CID construction" (sorted-keys, no whitespace, UTF-8). The CDDL below is interpreted against that JSON form.

> **Supersession for v1.4-and-later mementos.** This grammar describes the v1.1 flat-shape memento (top-level `bindingHash`, `propertyHash`, `producerSignature`, `inputCids`, `cid`). Under protocol v1.4, every memento adopts the three-layer envelope/header/body shape defined in [`2026-05-03-substrate-layers-envelope-header-body.md`](2026-05-03-substrate-layers-envelope-header-body.md). The substrate-layers spec is canonical for v1.4-and-later mementos; this grammar remains canonical for v1.1 mementos and serves as historical reference for the role taxonomy (catalog / contract / bridge / verdict / audit / deprecation / extension-declaration / implication), which carries forward into v1.4 unchanged. Per the protocol's monotonicity rule, v1.1 mementos remain valid forever against the bytes they were minted for. New kits MUST emit the v1.4 layered shape; old v1.1 mementos remain readable by any verifier that supports both shapes.

## What this document specifies

Every memento Sugar produces is a `ClaimEnvelope` (defined in
`2026-04-29-universal-claim-envelope.md`). This spec adds **role**:
the application-level purpose a memento serves. Eight roles are defined:
**catalog**, **contract**, **bridge**, **verdict**, **audit**,
**deprecation**, **extension-declaration**, **implication**.

The **contract** memento is the unit of behavior specification: a
function's pre + post + inv together, signed and content-addressed as
one artifact. The **implication** memento is a published Z3 witness
that one formula implies another, used by verifiers as a hash-keyed
cache of proven facts. Together these enable the handshake algorithm
specified in `2026-04-30-handshake-algorithm.md`: most call sites
discharge by hash equality; the residue runs Z3 once per (post, pre)
pair and publishes the result for everyone else.

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
  `hash(canonical(evidence.body.irFormula))`." A validator
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

; Reference: SugarIrFormula (rule name `ir-formula`) is defined in
; protocol/specs/2026-04-30-ir-formal-grammar.md and imported by name.

; ----- Scalars and CIDs --------------------------------------
;
; **Self-identifying cryptographic primitives.** Every hash and every
; signature carries its algorithm tag inline. Migration to new
; primitives (e.g. post-quantum signatures) is additive: new bytes
; carry new tags, old bytes keep their tags, verifiers dispatch by
; tag. There is no truncation. There are no per-purpose lengths.
; The hash IS the trust AND tells you how to check it.
;
; Format:
;   hash:      <algorithm>-<bits>:<lowercase-hex-digest>
;   signature: <algorithm>:<base64-payload>
;   pubkey:    <algorithm>:<base64-payload>
;
; v1.1.0 ships with `blake3-512` (hash) and `ed25519` (signature/key)
; as the only permitted tags. The protocol catalog lists permitted
; tags; future catalogs add tags additively.

; All hashes in the protocol — bindingHash, propertyHash, preHash,
; postHash, invHash, antecedentHash, consequentHash, cid, member-CID,
; schema-CID, and every other hash field — are this exact shape.
hash         = tstr .regexp "^[a-z0-9]+-[0-9]+:[0-9a-f]+$"
cid          = hash
binding-hash = hash
property-hash = hash

; ISO-8601 UTC timestamp with millisecond precision and trailing 'Z'.
iso8601      = tstr .regexp "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?Z$"

; producer-id format: <name>@<version>
producer-id  = tstr .regexp "^[A-Za-z][A-Za-z0-9_./:@-]*@[A-Za-z0-9._:+-]+$"

; Self-identifying signature: <algorithm>:<base64>. v1.1.0: ed25519.
signature    = tstr .regexp "^[a-z0-9]+:[A-Za-z0-9+/]+=*$"

; Self-identifying public key: <algorithm>:<base64>. v1.1.0: ed25519.
pubkey       = tstr .regexp "^[a-z0-9]+:[A-Za-z0-9+/]+=*$"

; ----- Wrapper verdict and version --------------------------

schema-version = "1"

verdict        = "holds" / "violated" / "decayed" / "undecidable" / "error"

; ----- The variant union ------------------------------------

evidence-variant =
    catalog-evidence
  / contract-evidence
  / bridge-evidence
  / verdict-evidence
  / audit-evidence
  / deprecation-evidence
  / extension-declaration-evidence
  / implication-evidence

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
  ? producerSignature: signature,
  cid:               hash
}
```

**ORDERING (wrapper):** `inputCids` MUST be lexicographically
ascending. Validators reject envelopes with unsorted `inputCids`.

**DERIVED (wrapper):** `cid` MUST equal `"blake3-512:" + hex(blake3_512(canonical))`
where `canonical` is the JCS encoding of the envelope with `cid`
and `producerSignature` elided. Computation is specified in
`2026-04-29-universal-claim-envelope.md` §"CID construction". The
algorithm tag (`blake3-512`) is part of the CID, not metadata about
it; verifiers dispatch on the tag at verification time.

## Role taxonomy at a glance

| Role         | `evidence.kind` (CDDL `kind` literal)                                                                        | Signature                          | `inputCids` referent role(s)             |
|--------------|--------------------------------------------------------------------------------------------------------------|------------------------------------|------------------------------------------|
| catalog      | `"kit-catalog"`                                                                                              | REQUIRED swarm; OPTIONAL local     | bridge, contract                          |
| contract     | `"contract"`                                                                                                 | REQUIRED swarm; OPTIONAL local     | contract                                  |
| bridge       | `"bridge"`                                                                                                   | REQUIRED swarm; OPTIONAL local     | contract, catalog                         |
| verdict      | `"z3-model"` / `"z3-unsat"` / `"test-pass"` / `"test-fail"` / `"pattern-match"` / `"lint-pass"` / `"type-check-pass"` / `"mutation-witness"` | REQUIRED swarm; OPTIONAL local | contract, verdict, bridge                 |
| audit        | `"workflow-run"`                                                                                             | REQUIRED swarm; OPTIONAL local     | audit, verdict, contract, bridge          |
| deprecation  | `"retirement"`                                                                                               | REQUIRED swarm; OPTIONAL local     | contract                                  |
| implication  | `"implication"`                                                                                              | REQUIRED swarm; OPTIONAL local     | contract                                  |

The signature column applies a single rule across roles: signatures
are REQUIRED for swarm-distributed mementos and OPTIONAL for
local-only ones. A swarm-distributed memento is one that crosses a
trust boundary (published, fetched from a registry, used as a leaf
in a downstream proofHash). Verifiers MUST reject unsigned mementos
imported from a swarm; verifiers MAY accept unsigned mementos
produced and consumed in-process.

## Role: CatalogMemento

A library or kit's published list of `(name, role, cid)` entries.
The catalog's CID is what `package.json`'s `sugar.proofHash`
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

- `bindingHash == hash(canonical("<kitName>@<kitVersion>"))` where
  `kitName` and `kitVersion` are taken from `evidence.body`.
- `propertyHash == hash(canonical("kit-catalog-root:<kitName>@<kitVersion>"))`.
- The multiset of `evidence.body.entries[*].cid` MUST equal the
  multiset of `inputCids`. (CDDL types each separately; the equality
  is post-CDDL.)

**ORDERING constraints (post-CDDL validator):**

- `inputCids` MUST be lex-sorted (inherited from the wrapper rule).
- `entries` MAY appear in any order; the validator sorts a copy by
  `cid` before computing the multiset comparison above.

**Signature.** REQUIRED for swarm-distributed catalogs. The kit
author's public key is published as `package.json`.`sugar.publicKey`
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
      "kitName": "@sugar/ts-kit",
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

## Role: ContractMemento

Publishes the canonical behavior contract for a function-shaped
binding: precondition, postcondition, and inductive invariant
together as one signed, content-addressed unit. The body carries up
to three IR formulas (per `2026-04-30-ir-formal-grammar.md`) — `pre`,
`post`, `inv` — each optional but at least one MUST be present. The
contract memento is the unit of behavior specification; bridges and
verdicts both refer back to it.

A contract memento says "I, the producer, commit that this binding
SHOULD satisfy these formulas." It does NOT assert the formulas hold
of any particular implementation; that is the verdict role's job. A
contract is a definition; a verdict reports whether reality conforms.

The three slots have distinct semantic roles:

- **`pre`**: a constraint the caller MUST establish before the call.
  Quantified over function inputs (parameter sorts).
- **`post`**: a guarantee the function provides on its return. The
  formula has free-variable `outBinding` (default `"out"`) referring
  to the return value; the verifier substitutes the call expression
  there at use sites.
- **`inv`**: a property that holds across the function's lifetime
  (e.g. "the receiver's state remains valid"). Optional; rare for
  pure functions, common for stateful objects.

The pre/post split is what enables the **handshake algorithm**
(`2026-04-30-handshake-algorithm.md`): a callee's `post` is matched
against a caller's `pre` by hash equality (free), by published
implication memento (cached), or by Z3 (residue). Most call sites
discharge without a solver invocation when the ecosystem ships
sufficient implication mementos.

```cddl
contract-evidence = {
  kind:    "contract",
  schema:  cid,
  body:    contract-body
}

contract-body = {
  contractName: tstr,
  ? pre:        ir-formula,        ; imported from 2026-04-30-ir-formal-grammar.md
  ? post:       ir-formula,        ; the post-formula's free var refers to the return value
  ? inv:        ir-formula,
  outBinding:   tstr,              ; name of the free-variable post uses for the return value
                                   ; conventionally "out"; required even when post is absent
  ? preHash:    hash,              ; DERIVED: hash(canonical(pre)) when pre is present
  ? postHash:   hash,              ; DERIVED: hash(canonical(post)) when post is present
  ? invHash:    hash,              ; DERIVED: hash(canonical(inv)) when inv is present
  authoring:    authoring-block
}

authoring-block =
    llm-authoring
  / kit-author-authoring
  / lift-authoring

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

lift-authoring = {
  producerKind: "lift",
  lifter:       producer-id,       ; e.g. "sugar-lift@1.0"
  evidence:     "tests" / "types" / "docs" / "symbolic-exec",
  ? sourceCid:  cid                ; the source artifact lift derived from
}
```

**Wrapper-field constraints (CDDL-checkable via shared rules):**

- `verdict = "holds"`.
- At least one of `pre`, `post`, `inv` MUST be present in
  `evidence.body`. A contract with all three absent has no
  semantic content; producers MUST NOT mint such a memento.
- `outBinding` MUST be present even when `post` is absent (this lets
  the contract evolve to add a `post` later without changing the
  binding name). The string MUST match the regex
  `^[A-Za-z_][A-Za-z0-9_]*$` to keep substitution predictable.

**REFERENT constraints (post-CDDL validator):**

- Every entry of `inputCids` MUST resolve to a memento whose role is
  **contract**. Validators reject contracts whose `inputCids` reach
  a non-contract role.

**DERIVED constraints (post-CDDL validator):**

- `preHash`, `postHash`, `invHash` MUST be present iff their
  corresponding `pre`/`post`/`inv` formula is present, AND MUST equal
  `hash(canonical(formula))` under JCS canonicalization. Validators
  recompute and reject mismatches. The redundancy makes the
  per-formula handshake index O(1) per memento at load time.
- `propertyHash == hash(canonical({pre?, post?, inv?, outBinding}))`
  — the wrapper-level `propertyHash` is the whole-contract identity,
  computed over the semantic fields (formulas + binding name) with
  JCS-canonical key order and `omit absent` semantics. The
  `contractName` and `authoring` fields are NOT in the
  propertyHash: two contracts with byte-equal pre/post/inv/outBinding
  produced under different names or by different authors share the
  same propertyHash, which is the point — the propertyHash is the
  canonical identity of the contract's behavior, not its provenance.
- `bindingHash == hash(canonical({producerId, contractName,
  propertyHash}))` where `producerId = wrapper.producedBy`,
  `contractName = evidence.body.contractName`,
  `propertyHash = wrapper.propertyHash`. Locking the construction
  makes contract bindingHashes reproducible across implementations.

**Signature.** REQUIRED for swarm distribution; OPTIONAL local.

### Worked example (parseInt: pre + post)

```json
{
  "schemaVersion": "1",
  "bindingHash": "a1b2c3d4e5f60718",
  "cid": "1234567890abcdef1234567890abcdef",
  "evidence": {
    "body": {
      "authoring": {
        "evidence": "tests",
        "lifter": "sugar-lift@1.0",
        "producerKind": "lift",
        "sourceCid": "0000000000000000000000000000beef"
      },
      "contractName": "parseInt",
      "outBinding": "out",
      "pre": {
        "kind": "forall",
        "name": "_x0",
        "sort": { "kind": "primitive", "name": "String" },
        "body": {
          "kind": "atomic",
          "name": ">",
          "args": [
            { "kind": "ctor", "name": "length", "args": [
              { "kind": "var", "name": "_x0" }
            ]},
            { "kind": "const", "value": 0,
              "sort": { "kind": "primitive", "name": "Int" } }
          ]
        }
      },
      "post": {
        "kind": "forall",
        "name": "_x0",
        "sort": { "kind": "primitive", "name": "String" },
        "body": {
          "kind": "atomic",
          "name": ">",
          "args": [
            { "kind": "var", "name": "out" },
            { "kind": "const", "value": 0,
              "sort": { "kind": "primitive", "name": "Int" } }
          ]
        }
      },
      "preHash": "5a6b7c8d9e0f1a2b",
      "postHash": "9e0f1a2b3c4d5e6f"
    },
    "kind": "contract",
    "schema": "00000000000000000000000000000c02"
  },
  "inputCids": [],
  "producedAt": "2026-04-30T12:00:00.000Z",
  "producedBy": "sugar-lift@1.0",
  "producerSignature": "MEUCIQ==",
  "propertyHash": "f0e1d2c3b4a59687",
  "verdict": "holds"
}
```

The IR-JSON shape is governed by the IR Formal Grammar spec. A
conforming validator imports the IR grammar's `ir-formula` rule and
applies it to `evidence.body.pre`, `body.post`, and `body.inv`
independently.

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

- `inputCids[0]` MUST resolve to a memento whose role is **contract**
  OR **catalog** (the deeper-layer contract).
- `inputCids[0] == evidence.body.targetContractCid`. The redundancy
  is structural so a chain walker can enumerate edges without
  parsing variant bodies.

**DERIVED constraints (post-CDDL validator):**

- `bindingHash == hash(canonical({sourceLayer, sourceSymbol}))`.
- `propertyHash == hash(canonical("bridge:<sourceSymbol>"))`.

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
  is **contract**, **verdict**, or **bridge**.

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
  **audit**, **verdict**, **contract**, or **bridge**. An audit
  memento MUST NOT point at catalog mementos directly (the catalog
  is a publication artifact, not an input to a stage).

**DERIVED constraints (post-CDDL validator):**

- `propertyHash == hash(canonical(evidence.body.workflowCid))`.
  The wrapper-level `propertyHash` MUST agree with the body's
  workflow identity.
- `bindingHash == hash(canonical(evidence.body.inputCanonicalForm))`.
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
  **contract**.
- `evidence.body.successor`, when present, MUST resolve to a memento
  whose role is **contract**.
- `evidence.body.retiredPropertyCid == inputCids[0]`. Validators
  reject deprecations whose body and inputCids disagree.

**DERIVED constraints (post-CDDL validator):**

- `bindingHash == hash(canonical(evidence.body.retiredPropertyCid))`.
- `propertyHash == hash(canonical("retirement:" || retiredPropertyHash))`
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

## Role: ImplicationMemento

A signed Z3 (or other-solver) witness that one IR formula
universally implies another. Implication mementos are how the
**handshake algorithm** caches proven facts: once any party has
discharged `forall x. Q(x) -> P(x)`, the witness is content-
addressed and shared. Future verifiers that need the same
implication look it up by the (antecedent, consequent) hash pair,
verify the producer's signature, and skip the solver entirely.

This memento type is what makes Sugar anti-rivalrous: a Z3
invocation that produces an unsat result becomes a publishable
artifact, indexable by any party. An external **implication server**
crawls published `.proof` files, extracts implication mementos, and
exposes a query API: "given consequent hash H, list contracts whose
post-formula has antecedent that implies H." The memento format is
what such a server consumes; the server's specific implementation
is out of scope for this document.

```cddl
implication-evidence = {
  kind:    "implication",
  schema:  cid,
  body:    implication-body
}

implication-body = {
  antecedentHash: hash,         ; hash(canonical(antecedent-formula))
  consequentHash: hash,         ; hash(canonical(consequent-formula))
  antecedentCid:  cid,          ; CID of a contract memento containing the antecedent
                                ; as one of its pre/post/inv slots
  consequentCid:  cid,          ; CID of a contract memento containing the consequent
  antecedentSlot: ("pre" / "post" / "inv"),
  consequentSlot: ("pre" / "post" / "inv"),
  prover:         producer-id,  ; e.g. "z3@4.13.4"
  proverRunMs:    uint,
  ? smtLibInput:  tstr,         ; the script the prover was given (for replay)
  ? proofWitness: tstr          ; solver-specific unsat-proof artifact when available
}
```

**Wrapper-field constraints:**

- `verdict = "holds"` (the implication is asserted as true).
- `inputCids` MUST contain exactly two entries, lex-sorted: the
  antecedent contract CID and the consequent contract CID. CDDL
  `[* cid]` is narrowed to exactly-2 in the post-pass.

**REFERENT constraints (post-CDDL validator):**

- Each entry of `inputCids` MUST resolve to a memento whose role is
  **contract**.
- `{antecedentCid, consequentCid}` MUST equal the multiset of
  `inputCids` (the body and inputCids agree on which contracts are
  involved).
- The contract at `antecedentCid` MUST contain a slot named by
  `antecedentSlot`, and the slot's formula's `hash(canonical(...))`
  MUST equal `antecedentHash`. Same for `consequentCid` /
  `consequentSlot` / `consequentHash`. Validators verify both before
  accepting the implication.

**DERIVED constraints:**

- `bindingHash == hash(canonical({antecedentHash, consequentHash}))`.
  The bindingHash is the (ordered) hash pair, which is what an
  implication server indexes on.
- `propertyHash == hash(canonical("implication:" || antecedentHash || ":" || consequentHash))`.

**Signature.** REQUIRED for swarm-distributed implications. An
unsigned implication has no useful trust property: the verifier
cannot distinguish a real Z3 witness from a fabrication. Producers
of implication mementos publish their solver identity (`prover`
field) and sign with the key they have published as their solver
identity's public key.

**Replay.** A verifier that wants to re-prove the implication
locally can decode `smtLibInput` and feed it to its own solver. If
the local solver disagrees with the published witness, the
implication is rejected and SHOULD be reported (the producer or the
solver is buggy or the signature is forged).

### Worked example

```json
{
  "schemaVersion": "1",
  "bindingHash": "8f3a7c0d1e2b4a56",
  "cid": "bcde0123456789abcdef0123456789ab",
  "evidence": {
    "body": {
      "antecedentCid": "1234567890abcdef1234567890abcdef",
      "antecedentHash": "9e0f1a2b3c4d5e6f",
      "antecedentSlot": "post",
      "consequentCid": "abcd1234ef567890abcd1234ef567890",
      "consequentHash": "5a6b7c8d9e0f1a2b",
      "consequentSlot": "pre",
      "prover": "z3@4.13.4",
      "proverRunMs": 47,
      "smtLibInput": "(set-logic ALL) ..."
    },
    "kind": "implication",
    "schema": "00000000000000000000000000000c08"
  },
  "inputCids": [
    "1234567890abcdef1234567890abcdef",
    "abcd1234ef567890abcd1234ef567890"
  ],
  "producedAt": "2026-04-30T12:08:00.000Z",
  "producedBy": "z3@4.13.4",
  "producerSignature": "MEUCIQ==",
  "propertyHash": "0d2e4f6a8b0c1d3e",
  "verdict": "holds"
}
```

## Role: ExtensionDeclarationMemento

Wraps an IR extension declaration (sort / predicate / ctor introduction)
per `protocol/specs/2026-04-30-ir-extension-protocol.md`. Kit-shipped
extensions ride in catalogs alongside bridges + properties; consumers
walk the envelope and dispatch to the local extension registry.

```cddl
extension-declaration-evidence = {
  kind:   "extension-declaration",
  schema: cid,
  body:   extension-declaration-body
}

extension-declaration-body = {
  declaration: extension-declaration
}

; The extension-declaration itself is the IR-extension-protocol's
; authoring shape, defined in the IR extension protocol spec. The
; embedded `signer`/`signature` fields (if any) are unused at this
; layer — the wrapping envelope's producerSignature is the authority.
extension-declaration = sort-extension / predicate-extension / ctor-extension

sort-extension = {
  introduces: "sort",
  name:       tstr,
  ? params:   [* { name: tstr, paramSort: "Int" / "Bool" / "String" }],
  semantics:  [* semantic-declaration],
  compilers:  [* tstr],
  ? declaredAt: tstr,
  ? dependsOn:  [* cid]
}

predicate-extension = {
  introduces: "predicate",
  name:       tstr,
  argSorts:   [* sort-ref],
  semantics:  [* semantic-declaration],
  compilers:  [* tstr],
  ? declaredAt: tstr,
  ? dependsOn:  [* cid]
}

ctor-extension = {
  introduces: "ctor",
  name:       tstr,
  argSorts:   [* sort-ref],
  returnSort: sort-ref,
  semantics:  [* semantic-declaration],
  compilers:  [* tstr],
  ? declaredAt: tstr,
  ? dependsOn:  [* cid]
}

semantic-declaration =
    { kind: "smt-lib-theory",   theory: tstr, ? version: tstr }
  / { kind: "axiom-set",        axioms: [* any] }
  / { kind: "proof-assistant",  system: tstr, identifier: tstr, ? proofCid: cid }
  / { kind: "natural-language", text: tstr }
```

**Wrapper-field constraints:**

- `verdict = "holds"`.
- `inputCids` MAY be empty (ground-level introduction) OR list other
  extension-declaration mementos this declaration depends on
  (e.g., a ctor extension that depends on a sort extension introducing
  its return sort).

**REFERENT constraints:**

- Each `inputCids[i]` MUST resolve to an extension-declaration memento.

## The full role-permission matrix

CDDL types `inputCids` as `[* cid]`. The post-CDDL referent table:

```
catalog.inputCids     -> { bridge, contract }                    REQUIRED non-empty
contract.inputCids    -> { contract }                            MAY be empty
bridge.inputCids      -> { contract, catalog }                   exactly 1 entry
verdict.inputCids     -> { contract, verdict, bridge }           MAY be empty
audit.inputCids       -> { audit, verdict, contract, bridge }    MAY be empty
deprecation.inputCids -> { contract }                            exactly 1 entry
implication.inputCids -> { contract }                            exactly 2 entries
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
  `catalog-body`. Implementations MUST emit this exact shape;
  legacy `legacy-witness`-wrapped catalogs are not protocol-
  conformant.
- **Contract role.** Protocol requires `evidence.kind = "contract"`
  with a `contract-body` carrying optional `pre`/`post`/`inv`
  formulas (at least one present), a required `outBinding` name,
  the DERIVED per-formula hashes (`preHash`/`postHash`/`invHash`),
  and a tagged `authoring` block. The role replaces the prior
  `property-declaration` shape entirely; producers MUST NOT emit
  `kind: "property-declaration"` or use `irFormula` as a body field.
- **Implication role.** Protocol requires
  `evidence.kind = "implication"` with a body identifying the
  antecedent and consequent contract memento CIDs and the
  per-formula hashes. The handshake algorithm spec
  (`2026-04-30-handshake-algorithm.md`) defines when verifiers
  consume these mementos.
- **Deprecation role.** Protocol requires
  `evidence.kind = "retirement"` with a structured `retirement-body`
  pointing at a contract memento CID. Implementations that retire
  contracts via on-disk state outside the memento store are not
  protocol-conformant.
- **Verdict role.** Protocol requires the wrapper `verdict` and the
  variant body's verdict to satisfy the pairing rules. Joint
  constraint enforcement (rejecting `verdict: "holds"` paired with
  `kind: "test-fail"`) is mandatory at validation time, even if a
  given language's type system cannot statically express the joint
  constraint.
- **Verdict role, `pattern-match` polarity.** Protocol requires
  `polarity: ("defect-detector" / "feature-detector")` in the
  pattern-match body so the pairing rule is well-defined.
- **Contract `bindingHash` construction.** Locked to
  `hash(canonical({producerId, contractName, propertyHash}))` per
  the contract role's DERIVED constraints.
- **Audit `bindingHash` and `propertyHash` constructions.**
  Locked to canonical hashes of body fields per the audit role's
  DERIVED constraints; producers derive these, not callers.
- **Bridge role.** Body shape conforms to the protocol's
  `bridge-body`. The `notes: null` shape is non-conformant; the
  field MUST be omitted entirely when no notes apply.
- **Removed: the `legacy-witness` variant.** This variant existed
  for transitional backward compatibility with pre-v1.0.1 mementos.
  Under the scorched-earth protocol cut (catalog v1.0.2+) it is
  removed entirely. Producers MUST emit only the role-specific
  variants defined here. Validators MUST reject mementos with
  `evidence.kind = "legacy-witness"` as malformed.

These items name what every conforming reference implementation
(TypeScript, Rust, Go, C++) MUST do. There is no transitional
window: implementations either match the spec or they don't write
`.proof` files.
