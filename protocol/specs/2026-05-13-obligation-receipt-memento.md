# ObligationReceiptMemento

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Related:**
- `2026-05-13-proof-run-memento.md` (`StageReceipt`, TSavo/provekit#792)
- `2026-05-13-promotion-decision-memento.md` (`PromotionDecisionMemento`, TSavo/provekit#791)
- TSavo/provekit#796 (admissibility-spine umbrella)
- `2026-05-02-multi-solver-protocol-v2.md`
- `2026-05-09-algorithm-memento-protocol.md`
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)

## §0. Purpose

`ObligationReceiptMemento` is the durable substrate shape for obligation-level proof outcomes. It records the backend result for a single proof obligation, including the canonical input formula, verdict, backend identity, and any durable proof, model, tactic, witness, log, or trace artifacts needed to cite or audit the result.

Discharge is not promotion. A receipt records a discharge event as evidence. It does not admit substrate semantics, does not promote an object, and does not by itself satisfy any higher-layer gate. `PromotionDecisionMemento` may admit an obligation receipt as evidence for the proof gate, but that admission is a separate decision.

## §1. Wire shape (CDDL)

```cddl
; Shared scalar types:
;   cid
;
; Locked JCS key order: alphabetical within the object.
ObligationReceiptMemento = {
  "artifact_cids": [* cid],
  "backend_cid": cid,
  "backend_version": tstr,
  "counterexample_cid": nil / cid,
  "input_formula_cid": cid,
  "model_or_trace_cid": nil / cid,
  "obligation_cid": cid,
  "provenance_cid": cid,
  "receipt_kind": receipt-kind,
  "tactic_script_cid": nil / cid,
  "verdict": verdict,
}

receipt-kind =
  "counterexample" /
  "discharged" /
  "inconclusive" /
  "refused" /
  "tactic" /
  tstr

verdict =
  "backend-disagreement" /
  "budget-exhausted" /
  "malformed-artifact" /
  "sat" /
  "timeout" /
  "unknown" /
  "unsat" /
  tstr

cid = tstr .regexp "^blake3-512:[0-9a-f]{128}$"
```

## §2. Field semantics

Fields are serialized in JCS-canonical alphabetical key order before CID construction.

`artifact_cids` contains zero or more additional durable artifacts, such as witness files, backend logs, proof traces, solver transcripts, or normalized reports. Producers MUST sort this list ascending by CID string before CID construction unless an extension specification explicitly assigns order semantics to the list.

`backend_cid` identifies the backend, solver, checker, tactic engine, or verifier stage that produced the receipt.

`backend_version` is the backend version string reported by the producer. Producers SHOULD include enough information to distinguish materially different backend behavior.

`counterexample_cid` is either `nil` or the CID of a durable counterexample artifact.

`input_formula_cid` is the CID of the canonical formula handed to the backend.

`model_or_trace_cid` is either `nil` or the CID of a durable model, trace, proof log, or backend transcript.

`obligation_cid` identifies the obligation whose outcome is recorded.

`provenance_cid` identifies the provenance record for this receipt production event.

`receipt_kind` classifies the proof outcome shape. Consumers MUST fail closed for unknown unnamespaced values. Extension values MUST be namespaced strings.

`tactic_script_cid` is either `nil` or the CID of a tactic script artifact.

`verdict` records the backend verdict. Consumers MUST fail closed for unknown unnamespaced values. Extension values MUST be namespaced strings.

## §3. Receipt kinds

### §3.1 `discharged`

Emitted when a backend reports that the obligation has been proven, refuted as unsatisfiable, or otherwise discharged under that backend's proof discipline. A discharged receipt is evidence for higher-layer policy, not admission.

### §3.2 `counterexample`

Emitted when a backend produces a concrete witness, model, failing input, or trace showing that the obligation is not satisfied. The counterexample artifact SHOULD be referenced by `counterexample_cid`.

### §3.3 `tactic`

Emitted when the primary durable artifact is a tactic script or proof script that can be replayed or inspected independently. The script SHOULD be referenced by `tactic_script_cid`.

### §3.4 `inconclusive`

Emitted when the backend ran but did not produce a decisive proof or counterexample. This includes unknown solver results that are not better classified as timeout, budget exhaustion, malformed artifact, or backend disagreement.

### §3.5 `refused`

Emitted when the backend declines to process the obligation, rejects the input before analysis, or refuses under configured admissibility, safety, or capability rules. Refusal is a durable outcome and can be cited, but it is not a proof result.

### §3.6 Extension kinds

Additional receipt kinds MAY be represented by `tstr`. Extension values MUST be namespaced, for example `example.org/custom-kind`. Consumers that do not understand an extension kind MUST fail closed.

### §3.7 Receipt-kind × verdict × artifact invariants

The CDDL admits any combination of `receipt_kind`, `verdict`, and optional artifact CIDs. The following invariants are NORMATIVE; receipts that violate them are malformed and consumers MUST refuse them. A producer that cannot satisfy the invariants for its outcome MUST emit a `refused` receipt and explain the refusal in `artifact_cids`.

| `receipt_kind` | Required `verdict` values | Forbidden `verdict` values | Required CID fields | Forbidden CID fields |
|---|---|---|---|---|
| `discharged` | `unsat` (obligation negation refuted) OR `sat` (obligation directly witnessed by model used as proof, where backend discipline permits) | `unknown`, `timeout`, `budget-exhausted`, `backend-disagreement`, `malformed-artifact` | none beyond core | `counterexample_cid` MUST be `nil` |
| `counterexample` | `sat` (the obligation's negation is satisfied; equivalently, the obligation does not hold) | `unsat`, `unknown`, `timeout`, `budget-exhausted`, `backend-disagreement`, `malformed-artifact` | `counterexample_cid` MUST be non-`nil` | none |
| `tactic` | `unsat` (the tactic discharged the obligation) OR `unknown` (tactic explored but did not close) | `sat`, `timeout`, `budget-exhausted`, `backend-disagreement`, `malformed-artifact` | `tactic_script_cid` MUST be non-`nil` | `counterexample_cid` MUST be `nil` |
| `inconclusive` | `unknown` OR `timeout` OR `budget-exhausted` OR `backend-disagreement` (synthesis only) | `sat`, `unsat`, `malformed-artifact` | none beyond core, except `backend-disagreement` REQUIRES `artifact_cids` to be non-empty and cite the underlying disagreeing `ObligationReceiptMemento` CIDs | `counterexample_cid` MUST be `nil`; `tactic_script_cid` MUST be `nil` |
| `refused` | `malformed-artifact` OR a namespaced extension verdict explaining the refusal | `sat`, `unsat`, `unknown`, `timeout`, `budget-exhausted` | `artifact_cids` SHOULD be non-empty with a diagnostic citation | `counterexample_cid` MUST be `nil`; `tactic_script_cid` MUST be `nil` |

`backend-disagreement` is special: it is a stage-level synthesis verdict (per §7), not produced by a single backend run. A `backend-disagreement` verdict appears ONLY on synthesis receipts whose `receipt_kind` is `inconclusive`, and such a synthesis receipt MUST have `artifact_cids` non-empty with every entry citing one of the underlying disagreeing `ObligationReceiptMemento` CIDs. Other combinations of `receipt_kind` with `backend-disagreement` are malformed.

`model_or_trace_cid` is optional for every kind. When present:
- For `discharged` with `verdict: "unsat"`, it MAY cite a proof trace or refutation certificate.
- For `discharged` with `verdict: "sat"`, it MUST cite the witnessing model used as proof.
- For `counterexample`, it SHOULD cite the same model as `counterexample_cid` or a richer trace artifact.
- For `tactic` and `inconclusive`, it MAY cite a partial trace; absence is permitted.
- For `refused`, it MUST be `nil`.

Extension `receipt_kind` values MUST publish their own row in this table or in a companion namespaced extension spec. Consumers that cannot find such a row for an unknown extension MUST fail closed.

## §4. Verdict taxonomy

Canonical verdict values are:

`sat`: The backend found the formula satisfiable, usually supporting a counterexample or model.

`unsat`: The backend found the formula unsatisfiable, usually supporting discharge for an obligation encoded as a negated condition or refutation target.

`unknown`: The backend completed without a decisive satisfiability result.

`timeout`: The backend exceeded a time limit.

`budget-exhausted`: The backend exceeded a configured resource budget other than wall-clock timeout, such as fuel, iterations, memory, or proof search budget.

`backend-disagreement`: Multiple backend results conflict for the same obligation or canonical input formula.

`malformed-artifact`: A referenced artifact, formula, proof, model, trace, or witness was malformed, unparsable, failed validation, or could not be normalized.

Additional verdicts MAY be represented by `tstr`. Extension values MUST be namespaced. Consumers MUST fail closed for unknown unnamespaced verdicts and for unsupported extension verdicts.

## §5. StageReceipt interaction

`StageReceipt` references obligation receipts through `StageReceipt.output_cids[]`. A verifier stage that emits one or more `ObligationReceiptMemento` objects places their CIDs in `output_cids[]`.

The stage receipt records that a stage ran and produced outputs. The obligation receipt records the durable proof outcome for a specific obligation. Neither object is a promotion decision.

## §6. Counterexamples as durable negative evidence

Counterexamples are durable negative evidence. They preserve the model, witness, failing input, or trace needed to audit why an obligation did not discharge.

A counterexample is not admitted substrate semantics. An `ObligationReceiptMemento` with `receipt_kind = "counterexample"` can never be promoted as accepted because that would contradict its construction as negative evidence.

## §7. Backend-disagreement handling

When two or more backends disagree, each backend result remains durable as its own receipt. A disagreement SHOULD also be represented by a receipt with verdict `backend-disagreement` when a stage detects the conflict.

Resolution of backend disagreement belongs to higher-layer policy. This specification does not encode quorum rules, backend priority, trust ranking, or promotion policy.

## §8. CID construction

The CID for an `ObligationReceiptMemento` is computed from the JCS-canonical serialization of the object with alphabetical key order. The digest algorithm is BLAKE3-512.

Optional CID fields are encoded as `nil` when absent so the durable shape is stable and explicit.

## §9. Cross-references

This specification is intended to compose with:

* StageReceipt, issue #792
* PromotionDecisionMemento, issue #791
* admissibility-spine, issue #796
* `multi-solver-protocol-v2`
* `algorithm-memento-protocol`

## §10. Out of scope

Backend-specific artifact formats are out of scope.

Rules for combining multiple receipts into a higher-confidence verdict are out of scope.
