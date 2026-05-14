# Witness Consensus Promotion: Consensus Vector (`witness-consensus/1.1`)

**Status:** v1.1.0 amendment to `2026-05-14-witness-consensus-promotion.md`. Extends the promotion payload from a scalar cardinality threshold to a multi-dimensional consensus vector. Backwards-compatible with v1.0.0 mementos.

**Date:** 2026-05-14
**Related:** `2026-05-14-witness-consensus-promotion.md` (v1.0.0 baseline), `2026-05-13-compound-contract-memento.md`, `project_provekit_honesty_gradient.md` (#856).

## Motivation

The v1.0.0 spec uses `--min-witnesses N` as the consensus threshold. Cardinality is the FLOOR. It filters for nothing else: 1000 copies of one observation from one signer in one environment clears `--min-witnesses 1000` without producing meaningful evidence.

The substrate's honesty rule (#856) at the contract-promotion tier requires the same discipline applied at the field-name tier: a promotion's claimed strength must match the payload's actual refutation surface.

A `PromotionDecisionMemento.decision_payload.promotion = "documentary -> empirically-witnessed"` admitted by `(8 witnesses, 1 fixture, 1 signer, 1 sample each)` overclaims relative to one admitted by `(8 witnesses, 47 fixtures, 8 signers, 10000 samples each)`. Calling both "empirically-witnessed" is dishonest at the same gradient tier as a documentary `wp_note` claiming to be an `operation-contract`.

## The seven axes

Refutation-surface dimensions a consensus check SHOULD measure:

1. **Observer diversity**: number of distinct `signed_by` keys across the witness set. Distinguishes "8 from one CI" from "8 from 8 organizations."

2. **Environment diversity**: number of distinct `fixture_state_cid` values. Distinguishes "8 against one fixture" from "8 against 47 fixtures with varying DDL, row counts, runtime versions."

3. **Sample depth**: sum of `sample_count` across the witness set. Distinguishes "8 witnesses x 1 sample each" from "2 witnesses x 100,000 samples each."

4. **Input distribution coverage**: span of input arguments witnessed. Distinguishes "always observed against `id = 1`" from "observed against `id in {NULL, -1, 1, MAX_INT, 'sql injection attempt'}`."

5. **Loss-dimension coverage**: fraction of the concept-shape's named `loss_dimensions[]` that have at least one witness exercising them. Distinguishes "only `row-order` observed" from "all four named loss-dims of `concept:sql-query` observed."

6. **Temporal spread**: span between earliest and latest `observed_at`. Distinguishes "burst of 1000 witnesses in one second" from "1000 witnesses over six months."

7. **Failure-mode coverage**: counts of `outcome in {pass, fail, inconclusive}` across the unfiltered witness set, with named loss dimensions on each failure. Distinguishes "100 pass, 0 fail" from "98 pass, 2 fail on named dim X." A consensus admitted with non-zero `fail` count is still a valid empirical claim; it is just empirically-bounded-lossy at the named dim, not empirically-discharged.

## Extended `decision_payload`

The v1.0.0 baseline shape stays. The amendment adds an OPTIONAL `consensus_vector` field; consumers ignoring it MUST behave per v1.0.0.

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
  "witnesses_consulted": [],
  "consensus_vector": {
    "unique_signers": 1,
    "unique_signer_keys": ["<ed25519-pubkey-cid-or-unsigned>"],
    "unique_fixtures": 1,
    "total_sample_count": 4,
    "loss_dim_coverage": {
      "named_in_concept_spec": ["cursor-lifetime", "row-order", "sync-vs-async", "transaction-isolation"],
      "witnessed": ["row-order"],
      "unwitnessed": ["cursor-lifetime", "sync-vs-async", "transaction-isolation"]
    },
    "input_distribution_summary": {
      "shape": "named-bins",
      "bins": [
        { "name": "happy-path", "witness_count": 4 }
      ]
    },
    "temporal_spread": {
      "first_observed_at": "2026-05-14T16:26:06.052Z",
      "last_observed_at":  "2026-05-14T16:26:06.052Z",
      "span_seconds": 0
    },
    "failure_mode_distribution": [
      { "outcome": "pass", "count": 4 },
      { "outcome": "fail", "count": 0 },
      { "outcome": "inconclusive", "count": 0 }
    ]
  }
}
```

### Field discipline (per the honesty gradient)

- `unique_signers`, `unique_fixtures`, `total_sample_count`, counts in `failure_mode_distribution`, `span_seconds`: typed operational facts (Tier 2). Computed by the consensus runner; machine-checkable.
- `loss_dim_coverage.witnessed` and `.unwitnessed`: typed sets of dimension names cited from the concept spec; refusal if a witness names a loss dim not in `named_in_concept_spec`.
- `input_distribution_summary.shape`: enumerated (`named-bins`, `histogram`, `unspanned`). When `shape == "unspanned"`, downstream policy treats the input distribution as unverified.
- `temporal_spread.span_seconds`: derived from the two timestamps; redundant but stored explicitly for query-friendliness.

The field is `consensus_vector`, not `consensus_strength_score`: the substrate does not collapse a vector into a scalar. Policy collapses; the substrate exposes the components.

## Axis-named promotion tier

The v1.0.0 `decision_payload.promotion` is a single string (`"documentary -> empirically-witnessed"`). The amendment extends it to a list of axis-named transitions when multiple axes pass distinct thresholds:

```json
"promotion": [
  { "axis": "min-witnesses-floor",   "from": "documentary", "to": "minimally-witnessed",       "threshold": "n>=4" },
  { "axis": "environment-diversity", "from": "documentary", "to": "cross-fixture-witnessed",   "threshold": "fixtures>=3" }
]
```

A single contract can be promoted along multiple axes by the same consensus pass. Each axis carries its own threshold; the verifier policy selects which axes are gates.

A v1.0.0 single-string `promotion` is interpreted as one axis entry with `axis = "min-witnesses-floor"`. Backwards-compatible.

## `ConsensusPolicyMemento` (new family)

Verifier policies are content-addressed mementos. A policy names per-axis thresholds; consumers cite the policy CID when verifying. New family:

```json
{
  "kind": "consensus-policy",
  "schemaVersion": "1",
  "name": "<human label>",
  "thresholds": [
    { "axis": "min-witnesses-floor",   "predicate": "n>=4" },
    { "axis": "observer-diversity",    "predicate": "unique_signers>=3" },
    { "axis": "environment-diversity", "predicate": "unique_fixtures>=2" }
  ],
  "allow_failures": false,
  "require_loss_dim_coverage": "all-named",
  "signed_by": "<signer-cid>",
  "signature": "<ed25519-or-null>",
  "cid": "<self-cid>"
}
```

A `PromotionDecisionMemento.header.policy_cid` points at the `ConsensusPolicyMemento` that admitted the promotion. The audit trail is intact: the policy is signed, the policy's thresholds are content-addressed, the promotion cites both.

`ConsensusPolicyRegistry` (libprovekit, per #856 admissibility rule) parses these policies, validates `predicate` syntax against a small named grammar (`n>=N` / `unique_X>=N` / `X<=N` / boolean compositions), indexes by `policy_cid`, refuses malformed.

### Policy composition

Two consumers gating on different policies is the normal case:

- CI release gate cites a strict policy (e.g., `unique_signers>=3`, `unique_fixtures>=2`).
- Local dev cites a permissive policy (e.g., `n>=1`).

Both verify the same `PromotionDecisionMemento` against their own policy. Same memento, different gates, different verdicts. The verdict is a function of the policy CID, not of the substrate.

## Backwards compatibility with v1.0.0

- A v1.0.0 `PromotionDecisionMemento` (no `consensus_vector` field) remains admissible. The registry treats it as a vector with only `min-witnesses-floor` populated and all other axes `unspanned`.
- A v1.0.0 consensus runner reading a v1.1.0 memento ignores the `consensus_vector` field and the axis-named `promotion` list (falls back to the single-string `promotion` if present, otherwise refuses with a clear error).
- New mementos SHOULD carry the `consensus_vector` field; old mementos remain valid until a future v2 spec deprecates them.

## What a v1.1 consensus runner does

In addition to the v1.0.0 algorithm (filter, threshold, byte-equality check):

1. Compute `unique_signers` from `WitnessMemento.signed_by` (treating `null` signed_by as one synthetic "unsigned" key).
2. Compute `unique_fixtures` from `WitnessMemento.fixture_state_cid`.
3. Sum `WitnessMemento.sample_count` into `total_sample_count`.
4. Read the concept spec's `loss_dimensions[]`; intersect against `WitnessMemento.measurements.observer.loss_dims_exercised` (new optional field on witnesses; absent means "unwitnessed for this dim").
5. Bin the witness inputs into named bins (or use `unspanned` for v0 fixtures with no input variety).
6. Compute first/last `observed_at` and `span_seconds`.
7. Count outcome distribution across the unfiltered witness set (before the pass-only filter).
8. Compose `consensus_vector` and emit the memento with the axis-named `promotion` list.

The v1.0.0 fields (`min_witnesses`, `total_observations`, `agreement`, etc.) stay populated for backwards compatibility.

## Open questions

- **Policy DSL grammar.** The amendment uses simple predicates (`n>=N`, `unique_X>=N`). A full policy DSL with boolean composition (`AND`/`OR`/`NOT`) and per-axis weights is a separate spec extension; v1.1 keeps the grammar minimal so the parser can be byte-deterministic and rejects extensions until v1.2.
- **Loss-dim witness annotation.** The `measurements.observer.loss_dims_exercised` field on `WitnessMemento` is new. Adopting it requires a v1.1 to the witness spec as well, or a separate side-channel memento that asserts "witness X exercised loss-dim Y." Either path is admissible; the choice belongs to the witness-emitter side.
- **Input-distribution histograms.** For numeric SQL args, a named-bins shape (`small`, `boundary`, `large`, `out-of-range`) is more useful than a literal histogram. The bin definitions should be content-addressed per concept-shape so two consensus runners agree on what `boundary` means. Future spec extension.

## What this is NOT

- This amendment does not change the v1.0.0 selection or byte-equality discipline. Those remain.
- This amendment does not introduce a new memento family beyond `ConsensusPolicyMemento`. `PromotionDecisionMemento` is extended in-place via an optional field.
- This amendment does not collapse the vector into a scalar score. Policy may collapse; the substrate does not.
- This amendment does not change the `gate` field; both v1.0.0 (`"threshold"`) and any future gates remain valid.

## Why this matters

A contract's promotion to empirically-witnessed has been treated as a scalar event since the v0 design. Treating it as a vector aligns the consensus tier with the honesty gradient: the promotion's named strength matches the witness set's actual refutation surface.

This is paper 19's empirical-contract discharge made operational at multi-dimensional granularity. The substrate doesn't claim "this contract is true"; it claims "this contract has been observed under THESE axes at THESE strengths, signed, and here's the policy that admitted it."

Min-witnesses stays as the dumbest possible floor. Everything interesting lives in the consensus_vector.
