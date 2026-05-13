# ConceptSiteMemento -- Normative Spec (PR-A: schema + Rust types)

**Status:** v1.6.x normative-draft (PR-A of a multi-PR landing; see §8)
**Date:** 2026-05-12
**Related:**
- `2026-05-15-concept-hub-abstraction-layer.md` (§2.1 ConceptAbstractionMemento, §2.4 loss-record dimensions, §2.5 discharge)
- `2026-05-03-substrate-layers-envelope-header-body.md` (envelope/header/metadata layering)
- `2026-05-03-contract-cid-vs-attestation-cid.md` (CID semantics for inter-memento references)
- `2026-05-05-closure-binding-memento.md` (sibling: per-site discharge memento for an opacity effect)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)
- `2026-04-30-ir-formal-grammar.md` (IrFormula shape)
- `2026-04-30-memento-envelope-grammar.md` (envelope grammar)
- `docs/papers/09-lossy-boundary-compression.md` (obligation-preserving loss, the precedent for verdict trichotomy)
- `docs/papers/20-after-hope.md` (paper 20 architectural arc, the missing connector)

## §0. Purpose

The substrate already has:

1. `ConceptAbstractionMemento` (2026-05-15 §2.1): a content-addressed catalog node carrying a `contract: ir-formula` (a `wp_rule`) over named slots, sorted result, and a list of realizations.
2. `RealizationDesugaringMemento` (2026-05-15 §2.2): a content-addressed equation pinning a concept to a per-language operation-layer expansion, with `loss_record` over the five canonical dimensions.
3. `FunctionContractMemento` (per-language lifters via `provekit-lift-contracts`): a content-addressed per-function contract lifted from user annotations (`#[requires]`, `#[ensures]`, assertions in tests, and so on).
4. The wp evaluator in `libprovekit/src/wp.rs` over `ir-formula`.
5. The structural morphism catalog in `cmd_transport.rs`: identifies, for each user operation site, which concept op the site binds to. This binding is computed and immediately discarded; it is not minted as a memento, and there is no content-addressed object that records the binding plus its discharge verdict.

What is missing is exactly the connector: a content-addressed memento that says

> this user code-site at this function-term CID binds to this concept at this concept CID, under this lifted user contract, with this discharge verdict (one of `exact`, `loudly-bounded-lossy`, `refuse`), and these per-site witness samples.

`ConceptSiteMemento` is that connector. It is the substrate object that closes the loop user-code -> catalog. Without it, the binding is implicit, ephemeral, and unsignable. With it, the binding is content-addressed, durably stored in the pool, and verifiable from the substrate side.

### §0.1 The trichotomy is by construction, not by convention

Per the 2026-05-11 refinement of "Supra omnia, rectum": the substrate never claims more than it can prove. The discharge verdict for a binding is exactly one of:

- `exact`: the user-site's lifted contract is wp-preserving and `loss_record` is empty in every canonical dimension.
- `loudly-bounded-lossy`: the user-site's lifted contract is wp-preserving on the complement of `loss_record.domain_narrowing`, and the `loss_record` is non-empty in at least one dimension. The loss is bounded by the formula stored in `loss_record`, and that formula IS the contract of the lossy transformation.
- `refuse`: the discharger cannot show wp-preservation modulo any tractable `loss_record`. The binding is recorded but the substrate refuses to compose through it. A `refusal_reason` string is required.

Silent contract-dropping is not in the substrate's vocabulary. A binding that would silently drop a precondition or postcondition MUST be minted as `refuse` rather than as `exact` or `loudly-bounded-lossy` with empty fields.

### §0.2 Why the loss_record bytes are part of the binding CID

Two bindings to the same `concept_cid` with different `loss_record` bytes are different bindings. They live at different CIDs by construction. This means a site that originally bound `loudly-bounded-lossy` with loss L1, then later was re-discharged with a tighter loss L2, produces a new `ConceptSiteMemento` with a different CID; the two coexist in the pool, and downstream consumers reference whichever CID they were composed against. This is the "concept X under loss L1" versus "concept X under loss L2" distinction. Different addresses, different things.

### §0.3 Witness propagation

Unit tests asserting properties about ONE site become `WitnessMemento`s attached to the relevant concept. Every binding citing that concept INHERITS those witnesses as an empirical-discharge obligation. This is the propagation rule.

Concretely: if test `T` asserts that `f(3) = 6` for a function `f` whose contract clusters to `concept:double`, then `T` becomes a `WitnessMemento(concept_cid=concept:double, x=3, y=6)`. Every later binding to `concept:double` (from any user's code, in any language) inherits `T` as a sample that the binding's realized code must reproduce. The site discharges `T` empirically by running the realized code and observing the asserted property, or it refuses.

Per-site `witnesses` in this memento are pinned witness IDs and per-site confidence intervals; the floor empirical-discharge mechanism is specified separately in PR-F (see §8).

### §0.4 The deployment policy hint

`realization_mode_hint` is a NON-NORMATIVE field that carries a deployment-policy suggestion: this binding can be instrumented at runtime as a `witness` (recording samples), `emitter` (asserting and aborting on violation), or `monitor` (asserting and logging). The hint is policy advice from the discharger to the realize-side compiler. It does not affect the binding's CID-determining bytes only via being PART of the bytes; see §3.1.

The compile-side honors the hint via `provekit.toml` settings (PR-E). The substrate makes no claim about what the runtime mode SHOULD be; that is a deployment decision, not a correctness decision.

## §1. Wire shape (CDDL, v1.6.x layered)

```cddl
; Shared scalar types:
;   hash, cid, signature, pubkey, iso8601, ir-formula, loss-record

; A pointer to a WitnessMemento with a per-site confidence interval.
; Confidence intervals are encoded as basis-point integers in [0, 10000];
; 9500 means "the discharger reports 95.00% confidence that this witness
; holds at this site under the recorded witness policy."
;
; Locked JCS key order: ci_basis_points, witness_cid
witness-ref = {
  ci_basis_points: uint,                ; 0..10000 inclusive
  witness_cid:     cid                  ; the WitnessMemento CID
}

; Locked JCS key order: function_term_cid, source_cid, span
code-site = {
  function_term_cid: cid,               ; the FunctionContractMemento.cid of the
                                        ;   surrounding function (contract CID,
                                        ;   not attestation CID, per
                                        ;   2026-05-03-contract-cid-vs-attestation-cid.md)
  source_cid:        cid,               ; CID of the canonicalized source artifact
                                        ;   (file or unit) the site lives in
  span: {                               ; semantic span inside the canonical source
    start: uint,                        ;   byte offset, JCS-canonical
    end:   uint                         ;   byte offset, exclusive
  }
}

; Locked JCS key order: method, refusal_reason, verdict, discharge_receipt_cid, loss_record
discharge = {
  method:                tstr,          ; "wp"  | "witness" | "wp+witness"
  ? refusal_reason:      tstr,          ; REQUIRED iff verdict = "refuse"; OMITTED otherwise
  verdict:               tstr,          ; "exact" | "loudly-bounded-lossy" | "refuse"
  ? discharge_receipt_cid: cid,         ; MorphismDischargeReceipt CID; OMITTED when verdict = "refuse"
  loss_record:           loss-record    ; per 2026-05-15 §2.4; empty map is valid (means "no loss")
}

; Locked JCS key order: clusterer_cid, discharger_cid, lifter_cid
provenance = {
  clusterer_cid:  cid,                  ; the clusterer binary or rule-set CID that produced the binding
  discharger_cid: cid,                  ; the discharger binary or rule-set CID that filled in `discharge`
  lifter_cid:     cid                   ; the lifter binary or rule-set CID that lifted local_contract
}

; The concept-site memento itself.
;
; Locked JCS key order (alphabetical inside each object; layer order envelope-header-metadata):
;   envelope: {declaredAt, signature, signer}
;   header:   {cid, code_site, concept_cid, discharge, kind, local_contract_cid,
;              provenance, realization_mode_hint, schemaVersion, witnesses}
;   metadata: {? note}
concept-site-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,              ; over JCS(header ++ metadata)
    signer:     pubkey
  },
  header: {
    cid:                     cid,       ; DERIVED -- see §3
    code_site:               code-site,
    concept_cid:             cid,       ; CID of a ConceptAbstractionMemento
    discharge:               discharge,
    kind:                    "concept-site",
    local_contract_cid:      cid,       ; CID of the FunctionContractMemento for this site's user contract
    provenance:              provenance,
    ? realization_mode_hint: tstr,      ; "witness" | "emitter" | "monitor"; OMITTED when discharger is silent
    schemaVersion:           "1",
    witnesses:               [* witness-ref]
  },
  metadata: {
    ? note: tstr
  }
}
```

### §1.1 Field semantics

| Layer    | Field                    | Required | Meaning |
|----------|--------------------------|----------|---------|
| envelope | `declaredAt`             | yes      | ISO-8601 UTC minting timestamp. |
| envelope | `signature`              | yes (swarm) | Ed25519 over `JCS(header ++ metadata)`. OPTIONAL for local-only. |
| envelope | `signer`                 | yes      | `ed25519:<base64>` minter public key. |
| header   | `cid`                    | yes      | Content CID of this memento, DERIVED per §3. |
| header   | `code_site`              | yes      | The user site this binding is for. |
| header   | `concept_cid`            | yes      | The `ConceptAbstractionMemento.cid` this site binds to. Substrate matches exactly. |
| header   | `discharge`              | yes      | Verdict, loss, receipt, method. See §1.2. |
| header   | `kind`                   | yes      | MUST be `"concept-site"`. |
| header   | `local_contract_cid`     | yes      | The `FunctionContractMemento.cid` of the user-lifted contract for this function. *(Amended by `2026-05-13-compound-contract-memento.md` §0.4: after the compound layer lands, this field points at a `CompoundContractMemento.cid` instead; bare `FunctionContractMemento`s are auto-promoted per that spec's §4.4.)* |
| header   | `provenance`             | yes      | The three CIDs of the producers (lifter, clusterer, discharger). |
| header   | `realization_mode_hint`  | no       | Deployment policy hint. OMITTED when discharger does not opinion. |
| header   | `schemaVersion`          | yes      | MUST be `"1"`. |
| header   | `witnesses`              | yes      | Per-site witness samples with confidence intervals. MAY be empty array. |
| metadata | `note`                   | no       | Human-readable annotation. OMITTED when absent. |

### §1.2 `discharge` semantics

| Verdict                  | `loss_record`            | `discharge_receipt_cid` | `refusal_reason` |
|--------------------------|--------------------------|-------------------------|------------------|
| `exact`                  | empty map                | required                | OMITTED          |
| `loudly-bounded-lossy`   | non-empty in >= 1 dim    | required                | OMITTED          |
| `refuse`                 | any                      | OMITTED                 | required, non-empty |

The discharger MUST mint exactly one of these three. A binding with verdict `exact` and a non-empty `loss_record` is invalid; a binding with verdict `loudly-bounded-lossy` and an empty `loss_record` is invalid (it would be `exact`); a binding with verdict `refuse` and a `discharge_receipt_cid` is invalid. Validators MUST reject all three.

The `method` string records HOW the verdict was obtained:
- `"wp"`: by symbolic wp-evaluation only (no runtime samples consulted).
- `"witness"`: by witness sampling only (no symbolic check ran or could run).
- `"wp+witness"`: both ran, both agreed.

A discharger MAY refuse to mint when wp and witness disagree (the right answer when correctness is at stake), or MAY mint `refuse` with `refusal_reason` recording the disagreement. The substrate treats either as the same downstream effect: the binding does not compose.

### §1.3 `witnesses` semantics

Each `witness-ref` is a content-addressed pointer to a `WitnessMemento` (PR-F) plus a per-site confidence interval in basis points. The `WitnessMemento` carries the sample data (inputs, expected outputs, assertion predicate). The per-site `ci_basis_points` records the discharger's confidence that the witness holds at THIS site (which is generally stronger than the catalog-level confidence inherited from the concept).

A per-site `witnesses` array MAY be empty: a binding can be discharged symbolically (`method = "wp"`) without any witness samples. Witnesses inherited from the concept's `WitnessMemento` attachments propagate by REFERENCE through `concept_cid`, not by being copied here.

## §2. The verdict trichotomy, formally

Let `C_local` be the user-lifted contract (the `FunctionContractMemento` at `local_contract_cid`) and `C_concept` be the catalog contract (the `wp_rule` of the `ConceptAbstractionMemento` at `concept_cid`).

Let `wp_local` and `wp_concept` be their predicate transformers under the wp evaluator (`libprovekit/src/wp.rs`).

Let `loss` be the `loss_record`, a map from dimension name to `ir-formula` per 2026-05-15 §2.4.

Let `Q` range over arbitrary post-conditions (`ir-formula`s over the result sort and effects).

### §2.1 `exact`

```
verdict == "exact"  iff
  loss is the empty map
  AND
  for all Q:  wp_local(Q)  <=>  wp_concept(Q)
```

The user-site is interchangeable with the catalog concept on every state. Composition through this binding is free.

### §2.2 `loudly-bounded-lossy`

```
verdict == "loudly-bounded-lossy"  iff
  loss has at least one non-empty dimension
  AND
  for all Q:
    wp_local(Q) /\ NOT loss.domain_narrowing  ==>  wp_concept(Q)
  AND
  every additional effect, UB introduction, value divergence, and structural
  divergence beyond what the concept admits is FULLY characterized by the
  corresponding non-empty `loss` dimension formula.
```

Composition through this binding propagates the `loss_record` per the 2026-05-15 §2.4 composition rule (per-dimension union modulo dataflow).

### §2.3 `refuse`

```
verdict == "refuse"  iff
  the discharger could not establish the loudly-bounded-lossy precondition
  for any tractable loss formula in any of the five dimensions, OR
  wp and witness sampling disagreed, OR
  the user-lifted contract requires a vocabulary the concept's theory does
  not have (a "vocabulary gap").
```

Composition through this binding is refused by the substrate. The binding is preserved in the pool (it has signed provenance and a discharge receipt at the binding's own CID, just with `refuse` as the conclusion), so downstream consumers can SEE that the discharger looked and could not close.

A `refuse` is a substrate-load-bearing piece of negative information: it is the substrate's way of saying "this gap is known, and the discharger declined to paper over it." That is the difference between a refusal and silence. Silence is the absence of an attempt; `refuse` is the recorded acknowledgement that the attempt was made and failed.

## §3. Content-addressing rules

### §3.1 CID construction

The `cid` is the BLAKE3-512 of the JCS-canonical bytes of the `header` object with `cid` elided:

```
cid_input = JCS({
  "code_site":               <code_site>,
  "concept_cid":             <concept_cid>,
  "discharge":               <discharge>,
  "kind":                    "concept-site",
  "local_contract_cid":      <local_contract_cid>,
  "provenance":              <provenance>,
  "realization_mode_hint":   <realization_mode_hint>,   ; only if present
  "schemaVersion":           "1",
  "witnesses":               <witnesses>
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

ALL header fields except `cid` itself are part of the CID input. This includes `realization_mode_hint` (when present), `provenance`, and the full `discharge` block including its `loss_record` bytes. Two bindings with byte-identical headers except for `loss_record` produce different CIDs by construction (§0.2).

### §3.2 Sub-object canonicalization

Each sub-object (`code_site`, `discharge`, `provenance`, `witnesses[i]`) is JCS-canonicalized with alphabetical key order. The CDDL above LOCKS that order for human readability; the JCS encoder enforces it normatively.

`loss_record` keys are themselves alphabetical: `domain_narrowing`, `effect_divergence`, `structural_divergence`, `ub_introduction`, `value_divergence`. Absent dimensions are omitted (an absent key means "no loss in that dimension," per 2026-05-15 §2.4).

## §4. Mint procedure

1. Identify the user site: obtain `function_term_cid` (the `FunctionContractMemento.cid` of the surrounding function), `source_cid` (the canonical source artifact), and `span` (byte offsets).
2. Resolve the concept: the clusterer maps the user site's term structure to a `ConceptAbstractionMemento.cid`. Record as `concept_cid`.
3. Pin the local contract: the lifter has already minted a `FunctionContractMemento` for the function; its CID becomes `local_contract_cid`.
4. Run discharge:
   - Compute `wp_local` from `local_contract_cid` and `wp_concept` from `concept_cid.contract`.
   - Attempt to establish the §2.1 condition. On success, build `discharge` with verdict `exact` and empty `loss_record`.
   - On failure, attempt to find a `loss_record` (a per-dimension formula) such that the §2.2 condition holds. On success, build `discharge` with verdict `loudly-bounded-lossy`.
   - On both failing, build `discharge` with verdict `refuse` and a `refusal_reason` string.
   - When witnesses are available, also run them; if wp and witnesses agree, set `method = "wp+witness"`; otherwise either `"wp"`, `"witness"`, or a `refuse`.
5. Mint or reuse the `discharge_receipt_cid` (a `MorphismDischargeReceipt`, 2026-05-15 §2.5). For `refuse`, OMIT this field.
6. Attach per-site witnesses: for each known `WitnessMemento` attached to `concept_cid` (witness propagation, §0.3), include a `witness-ref` with the per-site `ci_basis_points`.
7. Set `realization_mode_hint` if the discharger has policy guidance; OMIT otherwise.
8. Fill in `provenance` with the three producer CIDs.
9. Build the header without `cid`.
10. Compute `cid` per §3.1.
11. Build the metadata.
12. Sign `JCS({header, metadata})` with the minter's Ed25519 key.
13. Emit the envelope.

**INVARIANT (verdict consistency):** Per §1.2 table; rejected if any row mismatches.

**INVARIANT (mint idempotency):** Two mint operations with byte-identical inputs MUST produce the same `cid`.

## §5. Validation rules

### §5.1 Pass 1: CDDL shape check

Reject if:
- Any required field is missing.
- `kind != "concept-site"` or `schemaVersion != "1"`.
- Any hash/CID field does not match the `"blake3-512:" ++ 128-hex` regexp.
- `discharge.method` is not one of `"wp"`, `"witness"`, `"wp+witness"`.
- `discharge.verdict` is not one of `"exact"`, `"loudly-bounded-lossy"`, `"refuse"`.
- `realization_mode_hint`, when present, is not one of `"witness"`, `"emitter"`, `"monitor"`.
- Any `witness-ref` has `ci_basis_points > 10000`.

### §5.2 Pass 2: verdict-consistency check

Per the §1.2 table:
- `verdict = "exact"`:
  - `loss_record` MUST be the empty map.
  - `discharge_receipt_cid` MUST be present.
  - `refusal_reason` MUST be omitted.
- `verdict = "loudly-bounded-lossy"`:
  - `loss_record` MUST have at least one non-empty dimension.
  - `discharge_receipt_cid` MUST be present.
  - `refusal_reason` MUST be omitted.
- `verdict = "refuse"`:
  - `discharge_receipt_cid` MUST be omitted.
  - `refusal_reason` MUST be present and non-empty.

### §5.3 Pass 3: DERIVED constraints

**DERIVED (cid):** Recompute per §3.1 and verify it equals `header.cid`. Reject on mismatch.

**SIGNATURE:** For swarm-distributed mementos, verify `envelope.signature` over `JCS({header, metadata})` against `envelope.signer`. Reject on invalid signature.

### §5.4 Pass 4: REFERENT constraints (pool-level)

- The pool MUST contain a `ConceptAbstractionMemento` with `cid = header.concept_cid`.
- The pool MUST contain EITHER a `CompoundContractMemento` with `cid = header.local_contract_cid`, OR a `FunctionContractMemento` with that CID that the validator auto-promotes per `2026-05-13-compound-contract-memento.md` §4.4. *(Amended by `2026-05-13-compound-contract-memento.md` §0.4.)*
- The pool MUST contain a `FunctionContractMemento` with `cid = header.code_site.function_term_cid`.
- For each `witness-ref` in `witnesses`, the pool MUST contain a `WitnessMemento` with that CID.
- For non-`refuse` verdicts, the pool MUST contain a `MorphismDischargeReceipt` with `cid = header.discharge.discharge_receipt_cid`.

A binding that references a missing pool object is unverifiable in context; the memento's own CID and DERIVED constraints are still verifiable in isolation.

## §6. Composition semantics

The substrate's composition guard for a function call:

```
can_compose_call(caller_contract, callee_concept, pool) :=
  pool contains a valid ConceptSiteMemento M where:
    M.header.concept_cid == callee_concept.cid
    M.header.code_site.function_term_cid == caller_contract.cid (or a containing scope)
    M.header.discharge.verdict in {"exact", "loudly-bounded-lossy"}
```

For `loudly-bounded-lossy`, the caller's composed `loss_record` accumulates the binding's `loss_record` per 2026-05-15 §2.4 (per-dimension union modulo dataflow).

For `refuse`, the substrate returns `OpacityError::ConceptBindingRefused { concept_cid, code_site, refusal_reason }` and does NOT compose. The caller MAY mint a new `ConceptSiteMemento` with a different `loss_record` (and therefore a different CID) that the discharger CAN close, replacing the refusal.

## §7. Worked example

A function `fn double(x: i64) -> i64 { x + x }` with `#[ensures(result == 2 * x)]` lifted into a `FunctionContractMemento`, clustered to `concept:double` (catalog), discharged `exact`:

```json
{
  "envelope": {
    "declaredAt": "2026-05-12T17:00:00Z",
    "signature":  "ed25519:MEUCIQDxxx==",
    "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="
  },
  "header": {
    "cid": "blake3-512:siteCID...<128 hex>",
    "code_site": {
      "function_term_cid": "blake3-512:fnCID...<128 hex>",
      "source_cid":        "blake3-512:srcCID...<128 hex>",
      "span": {"start": 1248, "end": 1303}
    },
    "concept_cid":        "blake3-512:conceptDouble...<128 hex>",
    "discharge": {
      "method":                "wp+witness",
      "verdict":               "exact",
      "discharge_receipt_cid": "blake3-512:receiptCID...<128 hex>",
      "loss_record":           {}
    },
    "kind":               "concept-site",
    "local_contract_cid": "blake3-512:localCID...<128 hex>",
    "provenance": {
      "clusterer_cid":  "blake3-512:clustCID...<128 hex>",
      "discharger_cid": "blake3-512:dischCID...<128 hex>",
      "lifter_cid":     "blake3-512:liftCID...<128 hex>"
    },
    "realization_mode_hint": "witness",
    "schemaVersion": "1",
    "witnesses": [
      {"ci_basis_points": 10000, "witness_cid": "blake3-512:witCID0...<128 hex>"},
      {"ci_basis_points":  9500, "witness_cid": "blake3-512:witCID1...<128 hex>"}
    ]
  },
  "metadata": {
    "note": "double(x) discharges to concept:double exactly under wp+witness."
  }
}
```

A counter-example: the same site BUT the lifter could only extract `#[ensures(result > x)]` (weaker than the catalog), so the discharger mints `loudly-bounded-lossy` with `loss_record.value_divergence` set to the formula `result < 2 * x`, expressing exactly the values the lifted local contract does not pin. The binding CID differs from the `exact` case above because the `loss_record` bytes differ.

A third case: the user wrote `fn double(x: i64) -> i64 { x.wrapping_add(x) }`. The lifter cannot extract an `#[ensures]` (the wrapping op carries the `i64::MAX -> overflow -> -i64::MAX - 1` case). The clusterer still binds to `concept:double`. The discharger emits `loudly-bounded-lossy` with `loss_record.ub_introduction` populated by the formula `x > i64::MAX / 2`, characterizing exactly the inputs on which the realization deviates from the abstraction. Two bindings, two different CIDs, both pointing at the same `concept_cid` and the same source site, but in two different versions of the source.

## §8. Roadmap (PR-A through PR-F)

This PR-A lands the SPEC and the Rust types only.

- **PR-A (this PR):** CDDL spec at `protocol/specs/2026-05-12-concept-site-memento.md` (this document) and `ConceptSiteMemento`, `CodeSite`, `Span`, `Discharge`, `Provenance`, `WitnessRef` types in `provekit-ir-types/src/lib.rs` with serde round-trip tests in `provekit-ir-types/tests/concept_site_serde.rs`.
- **PR-B (lifter wiring):** `provekit-walk` and each per-language lifter gain a `concept_cid: Option<String>` field on `FunctionContractMemento` (or auto-mint a sibling `ConceptSiteMemento` into `auto_minted_mementos` when the function's term clusters to a catalog concept). The clusterer that today lives inline in `cmd_transport.rs` is extracted into a callable surface.
- **PR-C (discharge wiring):** The discharger (a new binary or a mode of `provekit-discharge`) consumes a `(local_contract_cid, concept_cid)` pair and produces the `discharge` block by running the wp evaluator (algebraic) and witness sampling (empirical) against the concept's contract and its inherited witnesses. The `MorphismDischargeReceipt` is the per-binding receipt for `exact` and `loudly-bounded-lossy`.
- **PR-D (CLI):** `provekit catalog summarize` lists `ConceptSiteMemento`s grouped by concept, with per-concept discharge breakdown (counts of `exact` / `loudly-bounded-lossy` / `refuse`) and a gap-list of `refuse` rationales.
- **PR-E (realize-side compiler):** Each binding's contract is compiled into one of `witness` / `emitter` / `monitor` runtime wrappers per the `provekit.toml` deployment policy. The `realization_mode_hint` is consulted but not authoritative; the policy decides.
- **PR-F (empirical floor):** `WitnessMemento` gets a canonical `Vec<WitnessRef>` and confidence-interval discharge mechanism. The witness propagation rule of §0.3 is implemented end-to-end: tests attached to ONE site become witnesses at the concept level and propagate by REFERENCE through `concept_cid` to every binding citing the concept.

## §9. The smoke test (the architectural acceptance test)

A complete round-trip on a real Rust codebase:

1. Lift: every function in the codebase produces a `FunctionContractMemento` from user annotations or test assertions or wp_rule synthesis at clustering-mint time.
2. Cluster: every function term that matches a catalog concept gets a `concept_cid` assigned.
3. Bind: every `(function_term_cid, concept_cid)` pair gets a `ConceptSiteMemento` with a discharge verdict.
4. Discharge: the verdict is `exact` for sites whose local contracts wp-equal the concept contract; `loudly-bounded-lossy` for sites with a non-empty `loss_record`; `refuse` for sites where the discharger cannot close.
5. Realize: the realize-side compiler picks per-language realizations from each binding's `concept_cid` and emits Rust output. The runtime mode is set per `provekit.toml`.

**Acceptance:** every contract in the output traces back to (a) lifted user annotations, (b) lifted user test assertions, or (c) wp_rule synthesized structurally from the term algebra at clustering-mint time. ZERO contracts authored by humans during the round-trip.

This is the architectural acceptance test for the whole substrate. It pins the substrate's claim that user code-sites are bound to catalog concepts under content-addressed, signed, verdict-trichotomous bindings, and that the bindings compose under the 2026-05-15 §2.4 loss-record discipline.

## §10. Cross-references

- The `concept_cid` is produced today by the clusterer inline in `implementations/rust/provekit-transport/src/cmd_transport.rs`; PR-B extracts that surface.
- The `local_contract_cid` is produced by `provekit-lift-contracts` per language; the same CID semantics as `FunctionContractMemento.cid` per `2026-05-03-contract-cid-vs-attestation-cid.md`.
- The `discharge_receipt_cid` references a `MorphismDischargeReceipt`, the discharge type defined in 2026-05-15 §2.5.
- The substrate composition guard lives in `compose_function_contracts`; PR-B extends it to consult `ConceptSiteMemento`s in the pool.
- The wp evaluator at `libprovekit/src/wp.rs` is the single algebraic verdict-source.
- For sibling per-site discharge mementos see the closure / loop / try-branch mementos (2026-05-05).
- For the loss-record discipline see 2026-05-15 §2.4 (`domain_narrowing`, `effect_divergence`, `structural_divergence`, `ub_introduction`, `value_divergence`).
- For the trichotomy precedent see `docs/papers/09-lossy-boundary-compression.md` (obligation-preserving loss).
- For the architectural arc see `docs/papers/20-after-hope.md`.

## §11. Out of scope for PR-A

- Implementation of the lifter, clusterer, and discharger wiring (PR-B and PR-C).
- The `MorphismDischargeReceipt` schema beyond what 2026-05-15 §2.5 already specifies.
- The CLI surface for `provekit catalog summarize` (PR-D).
- The realize-side compiler integration (PR-E).
- The `WitnessMemento` canonical schema (PR-F).
- Backward-compatibility migration of existing `cmd_transport.rs` bindings (handled as a one-shot data migration in PR-B).

PR-A is the SPEC and the Rust TYPES. Validation passes 1-3 are testable from the types layer (CDDL-shape + verdict-consistency + DERIVED-cid); pass 4 (pool REFERENT) is tested in PR-B when the pool is wired.
