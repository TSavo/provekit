# Witness Consensus Promotion (`witness-consensus/1`)

**Status:** v1.0.0 operational draft for empirical promotion from `WitnessMemento` sets.
**Date:** 2026-05-14
**Related:** `2026-05-13-compound-contract-memento.md`, `2026-05-13-bind-ir-lift-result.md`

## Purpose

`WitnessMemento` records one empirical observation. Witness consensus is the local promotion rule that turns enough agreeing observations into a substrate upgrade event.

The output is not a new receipt family. It is a `PromotionDecisionMemento` with `gate = "threshold"` and `result = "admitted"` when the configured policy admits the witness set.

## Operational Shape

There is no public `provekit witness` command. Promotion is substrate computation
over already minted `WitnessMemento` data and belongs in the language-blind CLI,
but it must be exposed through a current gate command such as `prove`/`verify` or
a future dedicated promotion command. It must not read raw user-authored ProofIR
formula files.

Catalog inputs MAY be repeated. Each path is walked recursively. Consumers admit standalone `WitnessMemento` JSON files and migration receipts containing `witnesses[]`.

## Selection

A witness participates when all predicates hold:

- `witness_for == --concept`
- `fixture_state_cid == --require-fixture`
- `outcome == "pass"`

If fewer than `--min-witnesses` witnesses remain, the command rejects and emits no promotion decision.

## Agreement

For `concept:sql-query`, the first operational agreement axis is:

```json
measurements.row_schema
```

The command JCS-canonicalizes each selected `measurements.row_schema` value and requires byte equality across the selected set.

If the selected witnesses disagree, the command rejects and emits no promotion decision. The disagreement names the loss axis (`measurements.row_schema`) so later slices can mint an explicit `LossRecordMemento` instead of treating disagreement as a generic failure.

## Promotion Payload

On admission, the `PromotionDecisionMemento.header.decision_payload` carries at least:

```json
{
  "agreement": "byte-equal",
  "fixtures_consulted": ["blake3-512:<fixture>"],
  "min_witnesses": 4,
  "promotion": "documentary -> empirically-witnessed",
  "promoted_op": "concept:sql-query",
  "reason": "<human summary>",
  "row_schema": {},
  "subjects_consulted": [],
  "total_observations": 4,
  "witnesses_consulted": []
}
```

`header.evidence_cids` MUST contain the consulted witness CIDs. `header.policy_cid` identifies the consensus policy. `header.decider_cid` identifies the consensus command implementation or equivalent policy runner.
