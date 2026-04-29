# ProvekIt: universal claim envelope

> Author: shared session 2026-04-29 (T + Claude). Producer-agnostic
> memento schema. The contract every producer commits to.

## Why this spec exists

Today producers emit witness data in producer-private JSON. Z3 emits a
model. Datalog emits match-rows. The intake LLM emits an IntentSignal.
Cross-validation works at the level of (bindingHash, propertyHash) +
verdict — the framework reads the *wrapper* but not the *contents*.

For per-language kits and the producer pool to compose, the wrapper
fields must be fixed and the witness must be a *tagged union of typed
evidence variants*. New producers (a new LLM, a new prover, a new
language's type checker) add new variants without changing the wrapper.
Existing producers wrap their output to match.

Without this, Rust kit and COBOL kit produce mementos in incompatible
shapes; cross-validation degrades to "trust the producer name"; the
swarm cannot exchange mementos across kits.

This spec fixes:
- The memento wrapper schema (fields, types, semantics).
- The evidence-variant union (open, extensible, content-hashed).
- The CID construction (deterministic, canonical, host-language-
  agnostic).
- The producer-signature placement.
- The validation rules every consumer applies before trusting a
  memento.

## The wrapper schema

Every memento has the following fields. Wrapper schema is fixed; new
fields are not added without bumping the wrapper version.

```yaml
schemaVersion: "1"             # bump on incompatible wrapper change
bindingHash: hex16             # what content this claim is about
propertyHash: hex16            # what property is being claimed
verdict: enum                  # holds | violated | decayed | undecidable | error
producedBy: producer-id        # versioned producer identity (e.g. "z3-symbolic@4.13")
producedAt: iso8601            # production timestamp (UTC)
inputCids: [cid32]             # CIDs of upstream mementos that fed this one
evidence: EvidenceVariant      # tagged union (see below)
producerSignature: optional    # ed25519 sig over the canonicalized envelope
cid: hex32                     # sha256-prefix-32 of the canonicalized envelope
```

### Field semantics

**bindingHash.** sha256-prefix-16 of the canonical-encoded *content
identity* — for code claims, the bound source spans + structural
relationships. Hash construction is host-language-specific and
specified in the per-language kit spec; the wrapper requires only that
the result is a 16-hex-char string.

**propertyHash.** sha256-prefix-16 of the canonical-encoded *property
identity* — for code claims, the IR formula's canonical AST hash. Same
host-language-specific construction; wrapper requires only the result
shape.

**verdict.** Closed enum. The producer's verdict on whether the
property holds for the bound content.
- `holds` — property is satisfied.
- `violated` — property is contradicted (counterexample available).
- `decayed` — the bindingHash refers to content that no longer exists
  or has changed shape since this memento was produced. Memento is
  retained for audit but should not be relied on.
- `undecidable` — producer cannot determine in finite resources (e.g.,
  Z3 timeout, undecidable fragment).
- `error` — production failed mechanically (compiler crashed, network
  failure, etc.). Distinguished from `violated`: an error is a defect
  in the producer, not in the code.

**producedBy.** Versioned producer identity string. Format: `<name>@<version>`
where `<version>` is a semver, a git SHA, or a content hash sufficient
to identify the exact producer that emitted this memento. Examples:
`z3-symbolic@4.13.4`, `tsc@5.4.2`, `clippy@0.1.84`,
`llm:claude-opus@4-7`, `mutation-test@1.0.0`,
`workflow:bug-fix@bafy12abc...`.

**producedAt.** ISO-8601 UTC. Used for audit, replay, and decay
detection. Not part of the cache key (two runs of the same producer on
the same content at different times produce different `producedAt`
but identical CIDs only if `producedAt` is excluded from the hash —
see CID construction below).

**inputCids.** Sorted array of CIDs of upstream mementos. Empty for
terminal mementos. The DAG edge: walking inputCids reconstructs the
provenance chain.

**evidence.** The tagged-union variant carrying producer-specific
witness data. See "Evidence variants" below.

**producerSignature.** Optional. When present, ed25519 signature over
the canonicalized envelope (with `producerSignature` and `cid` fields
elided during signing). Verifier checks the signature against the
producer's published key. Absent signatures mean the memento is locally-
trusted but not cryptographically attested — fine for in-process use,
not sufficient for swarm distribution.

**cid.** sha256-prefix-32 of the canonicalized envelope (with `cid`
itself elided during hashing). The memento's content identity. Two
mementos with the same wrapper fields and evidence have the same cid;
mementos differ in cid iff they differ in any wrapper field or in
evidence content.

## Evidence variants

`evidence` is a tagged union. Every variant has a `kind` discriminator
and a `schema` field giving the content-hash of the variant's schema
definition (so consumers can confirm they understand the variant).

```yaml
evidence:
  kind: <variant-name>
  schema: hex32              # CID of the schema for this variant
  body: <variant-specific>   # variant payload
```

The wrapper does not interpret `body`. Consumers that need to read the
witness look up the variant by `kind`, fetch the schema by `schema`,
then parse `body` according to the schema.

### Standard variants (shipped with v1)

These are the variants the framework ships with. New producers add new
variants by publishing their schema (content-addressed) and registering
the kind with the swarm.

**`z3-model`** — Z3 returned `sat` with a model.
```yaml
kind: z3-model
schema: <CID of the z3-model schema definition>
body:
  smtLibInput: string         # the assertion that was checked
  z3Verdict: sat              # always "sat" for this variant
  model: string               # Z3's `(get-model)` output
  counterexample: object      # decoded model values keyed by binding name
  z3RunMs: number
```

**`z3-unsat`** — Z3 returned `unsat`.
```yaml
kind: z3-unsat
schema: <CID>
body:
  smtLibInput: string
  z3Verdict: unsat
  proof: optional string       # Z3's proof certificate when available
  z3RunMs: number
```

**`pattern-match`** — Datalog / SAST structural matcher fired.
```yaml
kind: pattern-match
schema: <CID>
body:
  pattern: string              # canonical pattern source
  matchedNodes: [nodeId]
  matchedCaptures: object      # capture-name → bindings
```

**`type-check-pass`** — host-language type checker accepted the binding.
```yaml
kind: type-check-pass
schema: <CID>
body:
  checker: string              # "tsc" | "rustc" | "ghc" | ...
  checkerVersion: string
  symbol: string               # the symbol that type-checked
  resolvedType: optional string
  diagnosticsClean: true
```

**`lint-pass`** — linter ran clean against the binding.
```yaml
kind: lint-pass
schema: <CID>
body:
  linter: string               # "clippy" | "biome" | "eslint" | ...
  linterVersion: string
  rulesetHash: hex32           # CID of the ruleset that ran
  warnings: 0
```

**`test-pass`** / **`test-fail`** — behavioral test ran.
```yaml
kind: test-pass | test-fail
schema: <CID>
body:
  runner: string               # "vitest" | "cargo test" | "pytest" | ...
  runnerVersion: string
  testId: string               # test identifier
  durationMs: number
  stdout: optional string
  failureDetail: optional string  # only on test-fail
```

**`llm-proposal`** — LLM-producer emitted an IR formula proposal.
```yaml
kind: llm-proposal
schema: <CID>
body:
  llm: string                  # "claude-opus" | "gpt-4" | "llama-70b" | ...
  llmVersion: string
  promptCid: hex32             # CID of the prompt that was used
  proposedIrFormula: string    # serialized IR formula
  confidence: number           # 0..1
  rationale: optional string
```

**`mutation-witness`** — Oracle #9 mutation verification produced its
verdict pair.
```yaml
kind: mutation-witness
schema: <CID>
body:
  testCid: hex32               # CID of the test memento
  mutationCid: hex32           # CID of the mutation that was applied
  failsOnOriginal: boolean
  passesOnFixed: boolean
```

**`workflow-run`** — terminal memento for a workflow execution.
```yaml
kind: workflow-run
schema: <CID>
body:
  workflowName: string
  workflowCid: hex32           # CID of the workflow definition
  inputCanonicalForm: object   # canonicalized input (for replay)
  output: <type-specific>      # workflow's terminal output
```

**`legacy-witness`** — backward-compat wrapper for pre-spec mementos
whose witness was opaque JSON.
```yaml
kind: legacy-witness
schema: <CID>
body:
  rawWitness: string           # the original opaque JSON
  legacyProducerId: string
```

### Adding a variant

A new variant is added by:

1. Publishing a schema definition (a JSON Schema or equivalent) as a
   content-addressed artifact with its own CID.
2. Producers using the variant emit `kind: <new-name>` and reference
   the schema CID in `evidence.schema`.
3. Consumers that don't recognize the variant treat the memento as
   "verdict-trustworthy but witness-opaque" — they can use the
   verdict for cross-validation but not inspect the evidence.

Variant authors publish their schemas to the swarm so consumers can
fetch the schema by CID. The framework does not gate variant
registration; new producers add variants freely. Cross-validation works
at the wrapper level (verdict + bindingHash + propertyHash) regardless
of variant compatibility.

## CID construction

The memento's `cid` is the sha256-prefix-32 of the canonicalized
envelope, with `cid` and `producerSignature` elided during hashing
(otherwise either field would self-reference).

Canonical encoding: JSON with sorted keys, no whitespace, UTF-8.

```
cidInput = canonicalize({
  schemaVersion,
  bindingHash,
  propertyHash,
  verdict,
  producedBy,
  producedAt,
  inputCids: sorted(inputCids),
  evidence: canonicalize(evidence),
})
cid = sha256(cidInput)[:32 hex chars]
```

`evidence` is canonicalized recursively — `kind`, `schema`, and `body`
all hashed; the variant body's canonical form is the variant's schema
responsibility (typically also sorted-keys JSON).

Two mementos with byte-identical canonical forms have the same cid.
Two mementos with different `producedAt` timestamps have *different*
cids (intentional — the timestamp is part of the memento identity).
Producers that want timestamp-independent cache keys should use the
(bindingHash, propertyHash, producedBy) tuple, not the cid.

## Validation rules

Before a consumer trusts a memento, it MUST validate:

1. **Wrapper shape.** All required fields present; types match the
   schema; `verdict` is one of the closed-enum values; `producedBy`
   matches the format `<name>@<version>`.

2. **CID integrity.** Recompute `cid` from the canonicalized envelope.
   Mismatch = the memento has been tampered with or generated
   incorrectly. REJECT.

3. **Signature (when present).** Verify `producerSignature` against the
   producer's published key. Mismatch = signature forged or producer's
   key compromised. REJECT for swarm-imported mementos; warn for
   locally-produced ones.

4. **Variant schema.** If the consumer needs to read `evidence.body`,
   fetch the schema by `evidence.schema` CID and validate the body
   against it. If the schema is unavailable or the body fails
   validation, the memento is "verdict-trustworthy but witness-
   inscrutable" — verdict can be used; body cannot be parsed.

5. **inputCids consistency.** If the consumer needs to walk the DAG,
   fetch each inputCid, confirm those mementos exist, and recursively
   validate them.

6. **Decay check (optional).** If the bindingHash refers to current
   content that no longer matches (the SAST node has been deleted, the
   file has been refactored), the memento is `decayed`. The framework's
   default policy is to mark decayed mementos in the local cache and
   re-run the producer to mint a fresh memento.

A memento that fails (1)–(3) is **rejected** — it is not a valid
memento. (4) and (5) are application-specific failure modes; (6) is a
freshness check.

## Producer-signature scheme (v1)

When mementos move between machines (swarm distribution), signatures
are required. v1 scheme:

- ed25519 keypair per producer-id.
- Public key published as a content-addressed artifact (a tiny memento
  itself with `kind: producer-public-key`).
- Signature scope: the canonical envelope with `cid` and
  `producerSignature` elided.
- Signature placement: `producerSignature` field, base64-encoded
  ed25519 signature.
- Key rotation: a producer publishing a new key emits a "rotation"
  memento referencing the old key's CID. Consumers walk the rotation
  chain to validate signatures against historically-current keys.
- Compromise / revocation: a producer (or an authorized peer) emits a
  "revoke" memento referencing the compromised key. Consumers reading
  the revoke memento ignore signatures from that key after the
  revoke's `producedAt`.

Higher-assurance schemes (HSM-backed signatures, multi-sig producer
identities, attestation-bound signatures) are forward-compatible —
they slot into `producerSignature` as alternative signature schemes
identified by a prefix byte.

## Backward compatibility

Existing mementos in the v1 schema (pre-this-spec) had:
- bindingHash, propertyHash, verdict, witness (opaque), producedBy,
  producedAt, producerSignal, cid, inputCids.

Migration: consumers reading pre-spec mementos wrap them in the
universal envelope by:
- Setting `evidence.kind = "legacy-witness"`.
- Setting `evidence.schema` to the legacy-witness schema CID.
- Wrapping the original `witness` string into `evidence.body.rawWitness`.
- Recomputing `cid` against the new canonical envelope (the cid
  changes because the envelope changed; the mapping from old cid to
  new cid is recorded in a one-time migration manifest).

Pre-spec producers continue to emit their old format until each is
upgraded to emit the new envelope directly. The framework reads both
formats during the transition.

## Implementation notes

- The schema definitions for each variant ship as files in the IR
  library's `evidence-schemas/` directory. They are content-hashed at
  package-publish time; the resulting CIDs are baked into the variant
  emitters.
- The wrapper validation library lives in `@provekit/claim-envelope`
  (TypeScript) and `provekit_claim_envelope` (Rust crate). Both are
  generated from the same canonical schema definition.
- Producers in any host language can emit valid envelopes by
  serializing via the canonical-encoding library; the bytes are
  content-addressed identically across languages.

## Acceptance test

The wrapper is correct when:
1. Two producers in different host languages (TS and Rust) emitting
   the same logical claim produce envelopes with the same
   `bindingHash`, `propertyHash`, and `verdict`. CIDs differ only in
   `producedBy`, `producedAt`, and `evidence` fields.
2. A consumer in any host language can parse, validate, and verify
   any envelope produced by any producer in any host language.
3. The cross-validation operation (find mementos with matching
   bindingHash + propertyHash but disagreeing verdicts) works
   identically across languages.
4. A new producer adds a new evidence variant by publishing the
   schema and registering the kind. Existing consumers handle the
   memento at the wrapper level without code changes.
5. The DAG walk (`walk(cid)`) traverses inputCids correctly across
   memento boundaries regardless of producer or host language.

When all five hold, the universal claim envelope is operational. Every
downstream spec (per-language kit, IR library, AST canonicalizer,
diff-driven intent extraction) builds on this contract.
