# PromotionDecisionMemento Normative Spec

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Author:** T Savo
**Related:**
- `2026-05-12-plugin-protocol.md` (PEP 1.7.0 plugin protocol)
- `2026-05-13-compound-contract-memento.md` (`EvidenceMemento`)
- `2026-05-15-concept-hub-abstraction-layer.md` (`ConceptAbstractionMemento`)
- `2026-05-12-concept-site-memento.md` (`ConceptSiteMemento`)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)
- `2026-05-03-contract-cid-vs-attestation-cid.md` (CID semantics for inter-memento references)
- `TSavo/provekit#796` (admissibility-spine umbrella)

## §0. Purpose

The substrate has an admissibility spine:

```text
terms + contracts + implications      = substrate objects
evidence + promotion                  = how substrate objects are learned
catalog equations                     = how substrate objects transport
proof runs + receipts                 = how substrate claims become admissible
provenance + plugin registry          = how the run becomes replayable
```

`PromotionDecisionMemento` is the promotion step made explicit. It records the moment a hypothesis stops being only a candidate and becomes substrate truth: a catalog fact such as a `FunctionContractMemento` clause, a `ConceptAbstractionMemento` member, or another promoted object that downstream mementos may cite.

Without this memento, promotion is implicit. A verifier can see the promoted artifact, but cannot replay which gate admitted it, which evidence supported it, which policy was applied, or which actor or backend made the decision. With this memento, promotion is content-addressed and auditable from the same pool as the evidence and catalog artifacts it connects.

### §0.1 Discharge is not promotion

Discharge proves an implication; promotion admits a claim. A `DischargeReceipt` (see TSavo/provekit#792, #800) records that a solver, equational rewriter, or other backend closed an obligation. A `DischargeReceipt` CAN be EVIDENCE for the `proof` gate of a promotion, but is not itself promotion: the catalog does not grow until a `PromotionDecisionMemento` cites the discharge receipt under an applicable policy and records the admission. This distinction is load-bearing. Discharge is a closed implication; promotion is an admission decision against a policy.

### §0.2 The substrate does not encode policy

This spec defines the SHAPE of a promotion decision. It does NOT define what counts as enough evidence, how many trials a property gate requires, what threshold a count-based gate sets, or which signers a human gate accepts. Those are policy decisions. Each is captured in a `PromotionPolicyMemento` (TSavo/provekit#798) cited by `policy_cid`, with `decision_payload` carrying the gate's structured evaluation against that policy. Substrate-level constants do not appear here. The same evidence + same gate + a different `policy_cid` MUST produce a different promotion CID by federation; that asymmetry is the federation guarantee on admissibility.

## §1. Wire shape (CDDL)

```cddl
; Shared scalar types:
;   cid, signature, pubkey, iso8601
;
; Locked JCS key order: alphabetical within each object.
; Producers MUST emit objects in JCS-canonical key order and MUST omit
; optional metadata fields when absent.

promotion-gate = "human"
               / "proof"
               / "property"
               / "threshold"
               / tstr   ; namespaced extensions, e.g. "ext:multisig", "ext:quorum"

promotion-result = "admitted" / "rejected" / "deferred"

; Locked JCS key order (top level): envelope, header, metadata
promotion-decision-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,           ; over JCS(header ++ metadata)
    signer:     pubkey
  },
  header: {
    candidate_cid:    cid,              ; hypothesis being promoted
    cid:              cid,              ; DERIVED, see §4
    decider_cid:      cid,              ; ProvenanceMemento, receipt, run, or pubkey reference
    decision_payload: json-value,       ; gate-specific decision evidence VALIDATED BY policy_cid
    evidence_cids:    [+ cid],          ; supporting EvidenceMementos
    gate:             promotion-gate,
    kind:             "promotion-decision",
    policy_cid:       cid,              ; PromotionPolicyMemento (or content-addressed rule text) that defines the gate predicate AND the decision_payload schema
    promoted_cid:     cid,              ; post-promotion artifact
    result:           promotion-result, ; admitted | rejected | deferred
    schemaVersion:    "1"
  },
  metadata: {
    ? counterexample_cids: [* cid],
    ? note:                tstr,
    ? source_url:          tstr
  }
}
```

## §2. Field semantics

| Layer | Field | Required | Meaning |
|---|---|---|---|
| envelope | `declaredAt` | yes | ISO-8601 UTC minting timestamp. |
| envelope | `signature` | yes | Ed25519 over `JCS(header ++ metadata)`. |
| envelope | `signer` | yes | `ed25519:<base64>` minter public key. For `gate = "human"`, this key MUST match or be delegated by `decider_cid`. |
| header | `candidate_cid` | yes | CID of the hypothesis being admitted. Usually a candidate cluster that points at a proposed concept, contract clause, or catalog member. |
| header | `cid` | yes | Content CID. DERIVED per §4 from the JCS-canonical header bytes with `cid` elided. |
| header | `decider_cid` | yes | CID of the thing that made the decision: a provenance or pubkey reference for a human gate, a discharge receipt for a proof gate, a property-test receipt for a property gate, or a run receipt for a threshold gate. |
| header | `decision_payload` | yes | Gate-specific structured evidence whose SHAPE is governed by `policy_cid`. The substrate spec does NOT define what counts as enough (no "100 trials", no "3 languages"); the policy does. Example threshold payload: `{evaluated_score: 17, required_score: 12, distinctness_axis: "language", result: "admitted"}`. Example proof payload: `{discharge_receipt_cid: "blake3-512:...", solver_verdict: "unsat", model_or_trace_cid: null}`. The `policy_cid`'s referenced policy MUST define the payload schema; validators replay by interpreting the payload against the policy's schema, not against substrate-wide rules. |
| header | `evidence_cids` | yes | Non-empty list of `EvidenceMemento` CIDs supporting the candidate. The list MUST be sorted ascending by CID string before CID construction. |
| header | `gate` | yes | Promotion gate used for this decision. MUST be one of §3's labels. |
| header | `kind` | yes | MUST be `"promotion-decision"`. |
| header | `policy_cid` | yes | CID of the `PromotionPolicyMemento` (see TSavo/provekit#798) that defines the gate predicate AND the `decision_payload` schema. This memento cites the policy by CID; the policy's content is governed by a separate spec. The same gate + same evidence + different `policy_cid` yields a DIFFERENT promotion CID by federation. |
| header | `promoted_cid` | yes | CID of the artifact created or admitted by the promotion, such as a `FunctionContractMemento`, `CompoundContractMemento`, `ConceptAbstractionMemento`, catalog equation, or catalog member. |
| header | `result` | yes | Promotion result: `admitted` (claim is now substrate truth), `rejected` (gate evaluated to fail), or `deferred` (gate insufficient, more evidence needed). A `rejected` or `deferred` promotion is still a durable receipt: it records that the policy was applied and what it returned. Only `admitted` causes the catalog to grow. |
| header | `schemaVersion` | yes | MUST be `"1"` for v1.0.0. |
| metadata | `counterexample_cids` | no | CIDs of known counterexamples considered during the decision. If present, the array MUST be sorted ascending by CID string. |
| metadata | `note` | no | Human-readable rationale or operator note. Required by policy only if the referenced policy says so. |
| metadata | `source_url` | no | Human-readable URL for a PR, issue, run page, log page, or external source. Non-normative. |

## §3. Gates

### §3.1 `human`

The decider is a person or a delegated human-review identity. `decider_cid` MUST resolve to a `ProvenanceMemento`, pubkey reference, or equivalent content-addressed identity record that binds the decision to the signer or to a delegation accepted by the policy.

`metadata.note` SHOULD carry the rationale when the policy does not already encode the complete rationale. A verifier replays this gate by checking the signature, resolving `decider_cid`, and confirming that the policy admits that human identity for the candidate and evidence set.

### §3.2 `proof`

The decider is a mechanical discharge backend. `decider_cid` MUST resolve to a discharge receipt, such as the `DischargeReceipt` being specified under `TSavo/provekit#792`, whose bytes identify the backend, inputs, obligations, verdict, and replay material.

A verifier replays this gate by validating the receipt, checking that the receipt consumes `candidate_cid` and the listed `evidence_cids`, and confirming that `policy_cid` accepts the receipt verdict as sufficient for promotion.

### §3.3 `property`

The decider is a property-based testing run. `decider_cid` MUST resolve to a property-test receipt that records the property, generator surface, seed or seed family, sample bounds, shrinking result if any, and run verdict.

The detailed property-test receipt schema is out of scope for v1.0. A v1.0 verifier MAY accept the shape of this gate but MUST refuse to treat it as admissible unless it implements the referenced property-test receipt semantics and the policy explicitly permits the gate.

### §3.4 `threshold`

The decider is a count-based policy. `policy_cid` MUST define the threshold predicate, including the count source, distinctness rule, minimum confidence if any, and whether counterexamples block admission. `decider_cid` MUST resolve to the run or aggregation receipt that crossed the threshold.

A verifier replays this gate by resolving the evidence set, applying the policy's distinctness and confidence rules, subtracting or blocking on `counterexample_cids` when the policy requires it, and confirming that the threshold was crossed by the referenced run.

## §4. CID construction

`header.cid` is DERIVED. Producers MUST compute it as:

```text
cid_input = JCS({
  "candidate_cid":    <candidate_cid>,
  "decider_cid":      <decider_cid>,
  "decision_payload": <decision_payload>,
  "evidence_cids":    <evidence_cids sorted ascending>,
  "gate":             <gate>,
  "kind":             "promotion-decision",
  "policy_cid":       <policy_cid>,
  "promoted_cid":     <promoted_cid>,
  "result":           <result>,
  "schemaVersion":    "1"
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

`envelope` and `metadata` do NOT participate in the content CID. They do participate in the signature bytes via `JCS(header ++ metadata)`.

**INVARIANT (promotion identity):** Two promotion decisions with byte-identical header fields except `cid` MUST produce the same `cid`. Changing the evidence set, policy, gate, decider, candidate, or promoted artifact produces a different promotion decision.

**INVARIANT (no evidence-order identity split):** `evidence_cids` is a set-like support list. Producers MUST sort it ascending by CID string before CID construction. If a policy needs weighted or ordered evidence, that structure belongs in the candidate or policy memento, not in `PromotionDecisionMemento.evidence_cids`.

## §5. Lifecycle

Promotion is minted after an evidence cluster crosses a gate predicate and after the promoted artifact's CID is known.

The lifecycle is:

1. Evidence mementos are minted and clustered into a candidate at `candidate_cid`.
2. A promotion policy at `policy_cid` is selected.
3. A gate fires: human approval, proof discharge, property-test run, or threshold crossing.
4. The post-promotion artifact is minted or selected at `promoted_cid`.
5. `PromotionDecisionMemento` is minted, tying `candidate_cid`, `evidence_cids`, `policy_cid`, `decider_cid`, `gate`, and `promoted_cid` into one content-addressed decision.
6. Downstream catalog mementos MAY cite `promoted_cid` as substrate truth, and MAY cite the promotion decision as the admission proof.

For `ConceptSiteMemento`, a cited concept or local contract whose basis was previously only a candidate can now point, directly or indirectly, at a promoted catalog artifact. This gives the site a stronger basis: the binding is no longer "this evidence resembles concept X"; it is "this evidence was promoted to catalog fact X by decision D under policy P."

Promotion decisions are append-only. New evidence or counterevidence does not mutate an existing decision. It mints a new candidate, a new promoted artifact if admitted, and a new promotion decision.

## §6. Federation

A verifier replays a promotion decision as follows:

1. Validate the CDDL shape and literals: `kind = "promotion-decision"` and `schemaVersion = "1"`.
2. Recompute `header.cid` per §4 and compare it to the asserted `cid`.
3. Verify `envelope.signature` over `JCS(header ++ metadata)` using `envelope.signer`.
4. Resolve `candidate_cid`, every `evidence_cid`, `policy_cid`, `decider_cid`, and `promoted_cid` from the pool or federated stores.
5. Confirm every `evidence_cid` is an `EvidenceMemento` and is part of the candidate's support set.
6. Interpret `policy_cid` as the gate predicate for the declared `gate`.
7. Replay the gate predicate against the candidate, evidence set, decider receipt or identity, and counterexamples.
8. Accept the promotion only if the replay result admits `promoted_cid`.

Federation relies on references, not transport. A peer may receive the promotion decision before the evidence, policy, decider receipt, or promoted artifact. Until all referenced CIDs resolve and replay succeeds, the promotion is a well-formed memento but not an admissible catalog fact in that verifier's pool.

## §7. Cross-references

- PEP 1.7.0 plugin protocol: `2026-05-12-plugin-protocol.md`.
- `EvidenceMemento`: `2026-05-13-compound-contract-memento.md` §1.1.
- `ConceptAbstractionMemento`: `2026-05-15-concept-hub-abstraction-layer.md` §2.1.
- `ConceptSiteMemento`: `2026-05-12-concept-site-memento.md`.
- JCS canonicalization: `2026-04-30-canonicalization-grammar.md`.
- CID semantics for inter-memento references: `2026-05-03-contract-cid-vs-attestation-cid.md`.
- Admissibility-spine umbrella: `TSavo/provekit#796`.
- Discharge receipt follow-up for the `proof` gate: `TSavo/provekit#792`.

## §8. Out of scope for v1.0

- Property-based testing receipt details beyond the gate-level contract in §3.3.
- Multi-decider quorum, including mixed human plus proof or N-of-M review policies.
- Gate revocation. Later counterevidence is represented by new mementos and new promotion decisions, not by mutating or deleting this one.
