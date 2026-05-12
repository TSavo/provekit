# DomainClaim Normalization — Substrate Surface as `k(I) = t`

**Status:** v1.6.x normative-draft (PR-A of a multi-PR landing; see §6)
**Date:** 2026-05-13
**Author directive:** Sir, 2026-05-12: "The code needs to literally read k(I)=t at a high level of abstraction."

**Related:**
- `2026-05-12-concept-site-memento.md` (the binding memento that is the canonical source of a per-site DomainClaim)
- `2026-05-15-concept-hub-abstraction-layer.md` §2.1, §2.2, §2.4 (ConceptAbstractionMemento, RealizationDesugaringMemento, loss-record dimensions)
- `2026-05-13-compound-contract-memento.md` (CompoundContractMemento and EvidenceMemento, currently in PR #716; conditional mapping rows §6.2)
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` (transport-gap mementos, loss composition)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization)
- `docs/papers/09-lossy-boundary-compression.md` (the trichotomy: exact / loudly-bounded-lossy / refuse)
- `docs/papers/17-program-is-structure.md` (the substrate-is-cipher lens; `k(I) = t` is the literal claim)

## §0. Purpose

The substrate's organizing equation is `k(I) = t`: apply key `k` (an algebra/operation, content-addressed) to input `I` (an artifact, content-addressed), and you get truth-claim `t` (a content-addressed concept, target-language source, shape, or other canonical truth). The trichotomy decides whether the equation holds exactly, holds with bounded loss, or refuses.

Today the verifier has per-memento-type knowledge. It dispatches differently on `ConceptSiteMemento`, on `RealizationDesugaringMemento`, on (in PR #716) `CompoundContractMemento`, on `FunctionContractMemento`. As new memento types land, the verifier needs new dispatch arms. The substrate's primary equation is aspirational rather than literal at the consumer surface.

This spec introduces a canonical wire-form type, `DomainClaim`, that every memento type decomposes into. The verifier consumes only `DomainClaim`. Per-memento-type knowledge becomes a thin `Into<DomainClaim>` impl per type. The verifier's surface is literally `k(I) = t`, with the verdict attached, in code as well as in prose.

### §0.1 Relationship to the existing `libprovekit::core::types::DomainClaim`

A type with the same name exists in `libprovekit::core::types` (introduced for the in-memory primitives). Its fields are richer: `domain, contract, artifacts, from: Vec<Cid>, premises, to: Cid, witness, verdict, attestation`. That type is a per-domain in-memory aggregate; it is NOT the wire-form surface this spec normalizes.

This spec defines a NEW canonical wire-form `DomainClaim` in `provekit-ir-types`. The relationship:

- `provekit_ir_types::DomainClaim` is the wire form. Three content-addressed CIDs (`kit_cid`, `input_cid`, `truth_cid`), a verdict in the trichotomy, provenance, and a signature. This is what the verifier consumes.
- `libprovekit::core::types::DomainClaim` is the in-memory aggregate used by primitives (`compose`, `address`, `discharge`, etc.). It can be projected onto the wire form via a `From` impl that lives in `libprovekit` (where the dependency direction allows it).

Both types coexist for the deprecation window. PR-D considers renaming or retiring the libprovekit aggregate once the verifier refactor in PR-B is complete.

### §0.2 The `Verdict` axis disambiguation

Two `Verdict` enums exist on `origin/main`:

- `libprovekit::core::types::Verdict`: `Unresolved | Proved | Refuted | Unknown`. This is a **scheduling/state** axis (has discharge run yet, did it succeed, did it fail, is it indeterminate).
- `ConceptSiteMemento.discharge.verdict` (string field): `"exact" | "loudly-bounded-lossy" | "refuse"`. This is the **trichotomy** axis (paper 09 obligation-preserving loss).

This spec aligns the wire-form `DomainClaim` with the trichotomy. The scheduling-state axis stays where it is, on the libprovekit in-memory aggregate. A `DomainClaim` minted from a `ConceptSiteMemento` carries the trichotomy verdict directly. A `DomainClaim` minted from a libprovekit `DomainClaim` (the in-memory aggregate) requires the projector to first resolve scheduling-state to trichotomy (a `Proved` aggregate with `loss_record` empty projects to `exact`; a `Proved` aggregate with non-empty `loss_record` projects to `loudly-bounded-lossy`; a `Refuted` aggregate projects to `refuse`; `Unresolved` and `Unknown` cannot be projected and the projection returns `Err`).

### §0.3 What "literally reads `k(I) = t`" means at the API level

After this spec lands and PR-B refactors the verifier:

```rust
// PR-B verifier surface (illustrative; not in this PR):
pub fn verify(claim: &DomainClaim) -> VerificationOutcome { ... }
```

Every consumer constructs a `DomainClaim` (directly or via `Into<DomainClaim>` from a memento) and hands it to `verify`. No per-memento-type dispatch arms. New memento types ship with their own `Into<DomainClaim>` impl and the verifier never grows.

## §1. CDDL — DomainClaim schema

```cddl
; Shared scalar types (per existing substrate):
;   cid          = tstr; "blake3-512:" prefix + 128 hex chars
;   iso8601      = tstr; ISO 8601 timestamp
;   pubkey       = tstr; "ed25519:" prefix + base64 32-byte key
;   signature    = tstr; "ed25519:" prefix + base64 64-byte sig
;   ir-formula   = (per 2026-04-30-ir-formal-grammar.md)
;   loss-record  = (per 2026-05-15-concept-hub-abstraction-layer.md §2.4)

; The trichotomy verdict. Exactly one of three.
;
; - "exact":                 k(I) = t with loss_record empty in every dimension.
; - "loudly-bounded-lossy":  k(I) = t modulo the loss bounded by `loss_record`.
;                            `loss_record` MUST be non-empty in at least one dim.
; - "refuse":                k(I) cannot be shown to equal t under any tractable
;                            loss-record. `refusal_reason` is REQUIRED.
verdict = "exact" / "loudly-bounded-lossy" / "refuse"

; Locked JCS key order: discharge_receipt_cid, kind, loss_record,
;                       refusal_reason.
verdict-body = {
  ? discharge_receipt_cid: cid,         ; MorphismDischargeReceipt CID; OMITTED iff kind = "refuse"
  kind: verdict,
  loss_record: loss-record,             ; per 2026-05-15 §2.4; empty map valid for "exact"
  ? refusal_reason: tstr                ; REQUIRED iff kind = "refuse"; OMITTED otherwise
}

; Locked JCS key order: declared_at, signer.
;
; Provenance is intentionally lean. Additional provenance lives on the
; source memento; the DomainClaim carries only the minimum required for
; the verifier to record who staked which claim and when.
domain-claim-provenance = {
  declared_at: iso8601,
  signer:      pubkey
}

; The DomainClaim itself. The substrate's literal `k(I) = t` surface.
;
; Locked JCS key order (alphabetical, with `kind` first by the substrate's
; envelope convention):
;   kind, input_cid, kit_cid, provenance, signature, truth_cid, verdict
;
; `signature` is OMITTED from the CID-determining bytes (signer-independent
; addressing per 2026-05-03-contract-cid-vs-attestation-cid.md and the
; libprovekit `address()` precedent).
domain-claim = {
  kind:       "domain-claim",
  input_cid:  cid,                      ; I: the artifact the kit operated on
  kit_cid:    cid,                      ; k: the operation/algebra that produced the claim
  provenance: domain-claim-provenance,
  signature:  signature,                ; Ed25519 over JCS(claim with signature elided)
  truth_cid:  cid,                      ; t: the canonical truth (concept, target source, shape, etc.)
  verdict:    verdict-body
}
```

### §1.1 Field semantics

- **`kit_cid`** (`k`): The content-addressed CID of the operation that produced this claim. Examples: a lifter binary CID, a discharger binary CID, a clusterer rule-set CID, a realize-side compiler CID. For multi-stage operations the kit-CID is the CID of the composite operation, not an intermediate step.

- **`input_cid`** (`I`): The content-addressed CID of the artifact the operation was applied to. Examples: the canonical source CID for a lifter, a tuple-CID `(lang-term-cid, concept-term-cid)` for a discharger that operates on a pair (canonicalized as a JCS array, then hashed). The input MUST be unambiguous; if the operation consumes multiple inputs, they are aggregated into a single tuple-CID per §3.2.

- **`truth_cid`** (`t`): The content-addressed CID of the canonical truth the claim asserts. Examples: a `ConceptAbstractionMemento.cid` for a binding-shaped claim, a target-language source CID for a realization, a shape-CID for a morphism discharge, a `FunctionContractMemento.cid` for a bare-contract lift.

- **`verdict`**: The trichotomy. `discharge_receipt_cid` is present iff `kind != "refuse"`. `refusal_reason` is present iff `kind == "refuse"`. `loss_record` is empty for `"exact"`, non-empty for `"loudly-bounded-lossy"`, and any (typically empty) for `"refuse"`.

- **`provenance`**: Author and timestamp. The signer-pubkey here MUST match the pubkey corresponding to `signature` (validators reject on mismatch). `declared_at` is non-normative metadata.

- **`signature`**: Ed25519 signature over `JCS(claim with signature elided)`. This is the courtesy attestation layer; it is excluded from the CID-determining bytes (see §3.1).

### §1.2 Verdict consistency invariants (normative)

The same constraints from `ConceptSiteMemento.discharge` apply, lifted to the DomainClaim wire form:

| `verdict.kind` | `discharge_receipt_cid` | `refusal_reason` | `loss_record` |
|---|---|---|---|
| `"exact"` | REQUIRED | MUST be absent | MUST be empty |
| `"loudly-bounded-lossy"` | REQUIRED | MUST be absent | MUST be non-empty |
| `"refuse"` | MUST be absent | REQUIRED | MAY be empty or non-empty |

A `DomainClaim` violating these is rejected at validation. Silent contract-dropping is not in the substrate's vocabulary.

## §2. `Into<DomainClaim>` impls — the normative mapping table

This table is normative for every memento type that exists on `origin/main` at the time of this spec. Each row defines `kit_cid`, `input_cid`, `truth_cid`, and the verdict source for one memento type. The mapping MUST be byte-deterministic: given a memento, the resulting `DomainClaim` bytes (after JCS canonicalization with `signature` elided) MUST be a deterministic function of the memento bytes plus the signer pubkey at signing time.

| Memento type | `kit_cid` (k) | `input_cid` (I) | `truth_cid` (t) | Verdict source |
|---|---|---|---|---|
| `ConceptSiteMemento` | `provenance.discharger_cid` | `code_site.source_cid` | `concept_cid` | `discharge.verdict` (already trichotomy) |
| `RealizationDesugaringMemento` | (see §2.2) | `provenance.lifter_cid` from the binding citing this realization, or the equation's own producer-CID if minted standalone | the `target_lang` source-CID at the realization site | `discharge_receipt`-derived (see §2.2) |

### §2.1 `ConceptSiteMemento → DomainClaim`

```rust
impl From<&ConceptSiteMemento> for DomainClaim {
    fn from(m: &ConceptSiteMemento) -> Self {
        DomainClaim {
            kind: "domain-claim".to_string(),
            kit_cid:   m.provenance.discharger_cid.clone(),
            input_cid: m.code_site.source_cid.clone(),
            truth_cid: m.concept_cid.clone(),
            verdict: VerdictBody {
                kind: parse_verdict(&m.discharge.verdict),
                loss_record: m.discharge.loss_record.clone(),
                discharge_receipt_cid: m.discharge.discharge_receipt_cid.clone(),
                refusal_reason: m.discharge.refusal_reason.clone(),
            },
            // provenance.signer / declared_at and signature are filled by the
            // envelope-layer signer at mint time; the From impl produces an
            // UNSIGNED claim ready for signing.
            provenance: DomainClaimProvenance::unsigned_placeholder(),
            signature: Signature::empty(),
        }
    }
}
```

- `kit_cid` is `provenance.discharger_cid`, not `provenance.lifter_cid`. The DomainClaim asserts the **binding** (the discharge's verdict), and the binding is the discharger's claim. The lifter and clusterer participated in producing the inputs the discharger acted on; their CIDs live on the source `ConceptSiteMemento` and are recoverable from the source memento at the address `truth_cid` would not normally cover; PR-B's verifier MAY consult them for richer provenance reporting without affecting verification correctness.
- `input_cid` is `code_site.source_cid`. The discharger's input is the canonical source artifact the site lives in. The byte-span (`code_site.span`) is binding-specific metadata; it does NOT participate in the wire-form `input_cid` (different sites within the same source map to different bindings, which differ by `truth_cid` and `kit_cid` in practice).
- `truth_cid` is `concept_cid`. The truth being asserted is "this site realizes this concept" with verdict `discharge.verdict`.

### §2.2 `RealizationDesugaringMemento → DomainClaim`

This memento is the per-language operation-layer expansion of a concept. The mapping is more delicate because the equation itself is the truth claim, but a target-language source-CID is what the substrate verifies against.

The mapping splits by whether the memento was minted **standalone** (in the catalog as a pure equation) or **at-a-binding** (cited by a `ConceptSiteMemento`):

- **Standalone (catalog-only)**: This memento expresses a generic abstraction-to-realization claim that is not tied to a user code-site. Standalone realizations are NOT directly verifiable via the DomainClaim surface; they are CATALOG entries that contribute to bindings. The `From<&RealizationDesugaringMemento>` impl produces an `Err(DomainClaimConversionError::Standalone)` to make this explicit. Standalone equations are exercised by the verifier ONLY through the bindings that cite them.
- **At-a-binding**: The realization is cited by a `ConceptSiteMemento` whose `discharge.method` is one of `"wp"`, `"witness"`, `"wp+witness"`. In this case the **binding** is the canonical DomainClaim (per §2.1) and the realization's contribution is recoverable from the binding's `discharge_receipt_cid`. The substrate does NOT mint a parallel DomainClaim from the realization.

The result: there is no `Into<DomainClaim>` impl for `RealizationDesugaringMemento` in this PR. The mapping table row above documents the conceptual mapping for future readers; the Rust impl materializes only the binding-side surface. PR-B's verifier walks bindings; standalone realizations are gathered as catalog inputs.

### §2.3 Why the mapping for `FunctionContractMemento` does not live in this crate

`FunctionContractMemento` is defined in `libprovekit::compose` (not `provekit-ir-types`). A `From` impl on it from `provekit-ir-types` would invert the dependency direction.

Therefore: the `From<&FunctionContractMemento> for DomainClaim` impl will live in `libprovekit`, NOT in this PR. It is documented here in the mapping table for completeness:

| Memento type | `kit_cid` (k) | `input_cid` (I) | `truth_cid` (t) | Verdict source |
|---|---|---|---|---|
| `FunctionContractMemento` (libprovekit-side impl, PR-B) | the lifter binary CID that produced this contract (carried in `locus` or auto_minted_mementos provenance) | the source CID the function was lifted from | the contract's own `cid` (the substrate asserts "this lift produced this contract" as the truth) | bare contract has no inline discharge: verdict defaults to a special "unverified" sentinel that the wire form rejects; the verifier consumes this only when the contract is wrapped in a binding |

PR-B addresses the practical question of what verdict to attach to a bare contract that has not yet been discharged. The provisional answer: bare contracts are NOT consumable by the verifier directly; they MUST be wrapped in a `ConceptSiteMemento` (the binding) or a `CompoundContractMemento` (PR #716) before reaching the verifier surface.

### §2.4 The `verdict` kind parser

```rust
fn parse_verdict(s: &str) -> VerdictKind {
    match s {
        "exact" => VerdictKind::Exact,
        "loudly-bounded-lossy" => VerdictKind::LoudlyBoundedLossy,
        "refuse" => VerdictKind::Refuse,
        other => panic!("invalid verdict kind on the wire: {other:?}; \
                         this is a substrate invariant violation -- the source \
                         memento should have been rejected at mint"),
    }
}
```

This parser is INFALLIBLE on a well-formed source memento (the source memento's own validation rejects bad verdict strings). The panic is the substrate-correct response; a caller observing this panic has found a bug in the source-memento validator.

## §3. CID derivation

The `DomainClaim` CID is BLAKE3-512 over JCS-canonical bytes of the claim, with the `signature` field elided. This is the signer-independent address pattern from `2026-05-03-contract-cid-vs-attestation-cid.md`: two well-formed `DomainClaim`s with identical `kit_cid` / `input_cid` / `truth_cid` / `verdict` / `provenance` but signed by different signers MUST collapse to the SAME CID.

### §3.1 JCS canonicalization

Per `2026-04-30-canonicalization-grammar.md`:

1. Serialize the claim with `signature` field REPLACED by the empty string `""` (not omitted; the field is REQUIRED and elision is by zero-value substitution).
2. Canonicalize via JCS (alphabetical key order at every object level; numbers in shortest-form; strings UTF-8).
3. Hash with BLAKE3-512.
4. Prefix `blake3-512:` and emit 128 hex chars.

### §3.2 Multi-input tupling

When a kit operates on more than one logical input, the wire-form `input_cid` is computed as:

```
input_cid = BLAKE3-512(JCS([cid_1, cid_2, ..., cid_n]))
```

That is: the inputs are JCS-canonicalized as a JSON array of CID strings (preserving order, per the kit's input contract), and the array's bytes are hashed. The resulting CID is itself a content-addressed object that the verifier MAY look up to retrieve the tuple. Single-input kits skip this step and pass the input's CID directly.

### §3.3 The three component CIDs are byte-included, not body-inlined

The `kit_cid`, `input_cid`, and `truth_cid` strings appear in the JCS bytes as CID strings, NOT as inlined memento bodies. This keeps `DomainClaim` size constant regardless of the size of the inputs / kit / truth. Verifiers MAY resolve any of the three to its body via pool lookup.

## §4. Verifier consumption (preview; full refactor in PR-B)

`provekit prove`, after PR-B, consumes only `DomainClaim` instances:

```
$ provekit prove --claim <DomainClaim.json>
verifying k(I) = t at address <DomainClaim.cid>...
  k = <kit_cid> (lifter "rust-contracts v1.6.0")
  I = <input_cid> (source "fixtures/safe_div.rs")
  t = <truth_cid> (concept "concept:div-by-zero-guarded")
verdict: loudly-bounded-lossy
  loss_record:
    domain_narrowing: x > 0
  discharge_receipt: <discharge_receipt_cid>
  signer: ed25519:...
  signature: VALID
result: OK
```

The CLI surface in PR-C walks a codebase, gathers all derivable mementos, converts each via `Into<DomainClaim>` (where applicable per §2), emits a stream of `DomainClaim` JSON objects, and pipes them through the verifier. The verifier's job is reduced to: parse the claim, recompute the CID, verify the signature, recompute the verdict from the discharge_receipt, and emit one outcome per claim.

## §5. Backward compatibility (deprecation shim)

Existing verifier code paths that consume typed memento inputs continue to work. The shim is structured as follows:

1. The `libprovekit` verifier code that today does `match memento_kind { ... }` adds a default arm that calls `memento.into_domain_claim().and_then(verify)`. This routes all not-yet-migrated callers through the unified path automatically.
2. Each existing per-memento-type entry point is marked `#[deprecated(since = "..", note = "use verify(&claim) on the DomainClaim wire form")]` but remains exported. Internally each shim builds a `DomainClaim`, delegates to `verify`, and returns the result in the old shape.
3. After the deprecation window (PR-D), the per-type entry points are removed. New verifier code paths consume only `DomainClaim`.

The shim is straightforward because §2's mappings are mechanical. The substrate trichotomy is preserved end-to-end by construction (the verdict on the source memento equals the verdict on the produced DomainClaim, byte-deterministically).

## §6. PR roadmap

This is **PR-A**: spec + Rust types + serde round-trip tests + the `ConceptSiteMemento → DomainClaim` mapping impl.

- **PR-A** (this PR): the spec, the `DomainClaim` / `VerdictKind` / `VerdictBody` / `DomainClaimProvenance` types in `provekit-ir-types`, serde round-trip tests for shape parity, the `From<&ConceptSiteMemento> for DomainClaim` impl (the one impl whose source type lives in `provekit-ir-types`).
- **PR-B** (`feat(domain-claim): refactor verifier to consume DomainClaim`): the `libprovekit` side of the conversion (`From<&FunctionContractMemento> for DomainClaim`, `From<&core::types::DomainClaim> for ir_types::DomainClaim`) and the `verify(&DomainClaim) -> VerificationOutcome` API. The existing verifier paths gain the deprecation shim from §5.
- **PR-C** (`feat(domain-claim): CLI surface for claim-graph walks`): `provekit prove` gathers mementos from a codebase, converts to DomainClaim, pipes to the verifier. Smoke-test driver gains the optional `--emit-domain-claim-graph` flag (§7).
- **PR-D** (`refactor(domain-claim): remove deprecation shim, retire libprovekit DomainClaim aggregate or rename it`): the deprecation-window cleanup. Renames `libprovekit::core::types::DomainClaim` to something less collision-prone (candidate name: `core::types::ClaimAggregate`) and routes all internal calls through the wire-form `DomainClaim`.

### §6.1 What does NOT land in PR-A

- No verifier refactor. The verifier still uses its per-type dispatch arms; PR-B does that work.
- No CLI changes. `provekit prove` keeps its current surface; PR-C does that work.
- No removal of the libprovekit `DomainClaim` aggregate. PR-D does that work.
- No `Into<DomainClaim>` for `FunctionContractMemento` (its source type lives in `libprovekit`, not `provekit-ir-types`). PR-B does that work.
- No `Into<DomainClaim>` for `MorphismDischargeReceipt` — this type is referenced in CDDL and compiler READMEs but does not yet have a Rust struct. §6.3 covers it conditionally.

### §6.2 Conditional mapping rows — `CompoundContractMemento` and `EvidenceMemento` (PR #716)

These types are in OPEN PR #716, not on `main`. Their normative mapping rows are recorded here for adoption when #716 lands:

| Memento type | `kit_cid` (k) | `input_cid` (I) | `truth_cid` (t) | Verdict source |
|---|---|---|---|---|
| `CompoundContractMemento` | the lifter-CID of the multi-source compound-extractor (per #716 §4) | the `function_term_cid`'s `source_cid` (recovered via pool lookup) | the compound's own canonical `function_term_cid` (the compound itself IS the truth claim) | derived from per-evidence verdicts via the compound's `aggregation_strategy` (per #716 §2 trichotomy at the compound level) |
| `EvidenceMemento` | `lifter_cid` (per-source lifter) | `source_locator.source_cid` | the predicate's CID (the evidence's truth claim is "this predicate holds at this source-locator") | confidence-derived: `confidence_basis_points >= 10000 ⇒ exact`; `> 0 ⇒ loudly-bounded-lossy` with a `confidence-divergence` loss-record dimension; `= 0 ⇒ refuse` |

When PR #716 lands, a follow-up PR (PR-A2 of this normalization) adds the `Into<DomainClaim>` impls for both types and adds their round-trip tests. The trichotomy mapping for `EvidenceMemento` is novel (confidence-to-trichotomy) and the loss-record dimension `confidence-divergence` requires a §2.4 addition to `2026-05-15-concept-hub-abstraction-layer.md`. That addition is in scope for the #716 follow-up, NOT this PR.

### §6.3 Conditional mapping rows — `MorphismDischargeReceipt`

`MorphismDischargeReceipt` is referenced in `protocol/provekit-ir.cddl` (line 508), in compiler-crate READMEs and spec.jsons, but does not yet have a corresponding `pub struct MorphismDischargeReceipt` anywhere in Rust. It is currently produced and consumed as JSON-blob payloads identified by CID.

The mapping row for when this type is Rust-typed:

| Memento type | `kit_cid` (k) | `input_cid` (I) | `truth_cid` (t) | Verdict source |
|---|---|---|---|---|
| `MorphismDischargeReceipt` | `discharger_cid` | tuple-CID of `(lang_term_cid, concept_term_cid)` per §3.2 | `shape_cid` (the morphism's truth: the source's shape equals the concept's shape under the discharger's method) | `discharged` boolean ∧ `method` string: `discharged && method != "refuse" ⇒ exact`; `discharged && loss_record nonempty ⇒ loudly-bounded-lossy`; `!discharged ⇒ refuse` |

When the type is Rust-typed (currently expected to land alongside the discharger refactor in `libprovekit/src/wp.rs`), the `Into<DomainClaim>` impl goes in `libprovekit`. The Rust-typed mint is tracked separately and is NOT part of this PR.

## §7. Cross-language compatibility

The `DomainClaim` wire form is language-agnostic. The CDDL grammar §1 is the source of truth; any language's verifier consumes the same JCS bytes. A Java verifier, a Python verifier, a TypeScript verifier each implements a parser for the JSON wire shape, a JCS canonicalizer, BLAKE3-512, and Ed25519 verification. None of them needs to know about ConceptSiteMemento, FunctionContractMemento, etc.

Per-language lifters and dischargers MUST emit `DomainClaim` directly when they participate in the verifier surface. They MAY also emit their richer source mementos (ConceptSiteMemento, etc.) into the pool for use by other tooling, but the verifier's contract is `DomainClaim` only.

This closes paper 17 §"name-by-vector": the `DomainClaim` is the canonical 3-CID vector that names the substrate's verifier-facing equation. Different languages produce different source mementos; all roads lead to the same wire surface.

## §8. Smoke test

The smoke-test driver (`menagerie/smoke-test-e2e/`) gains an OPTIONAL flag `--emit-domain-claim-graph <out.json>` (lands in PR-C). When set:

- The driver walks every binding discovered during the smoke run.
- For each binding, it constructs the corresponding `DomainClaim` via the §2 mapping.
- It emits the resulting graph as `out.json`: a JSON array of `DomainClaim` objects.

Verifying the smoke-test fixture is then equivalent to running `provekit prove` against `out.json`. The expected outcome is byte-identical to the binding-level discharge outcome that the smoke driver already reports.

This is the closure check that pins the substrate-surface normalization to ground truth: the wire-form verifier must give the SAME answer the typed verifier gives, for every binding the smoke test exercises.

## §9. Constraints honored (Supra omnia, rectum)

- The trichotomy verdict is preserved across the `Into<DomainClaim>` conversion. The mapping table §2 makes the source-of-verdict explicit for every memento type; silent collapse is impossible by construction.
- Backward compatibility is mandatory. Existing per-type entry points remain available through the deprecation window (§5); no breaking change in PR-A.
- The mapping table is normative. Each memento type that exists on `main` has a row in §2 or a documented exclusion (FunctionContractMemento per §2.3, RealizationDesugaringMemento per §2.2). Types not yet on `main` (CompoundContractMemento, EvidenceMemento, MorphismDischargeReceipt) have conditional rows in §6.2 and §6.3.
- CID stability: `DomainClaim` CID is signer-independent (the `signature` field is elided from CID-determining bytes per §3.1). Different `(k, I, t, verdict)` ⇒ different CID. Same `(k, I, t, verdict)` ⇒ same CID regardless of signer.
- No em-dashes anywhere in this spec (Sir's rule).
- This is a SPEC PR. The verifier refactor lands in PR-B; the CLI lands in PR-C; the deprecation cleanup lands in PR-D.
