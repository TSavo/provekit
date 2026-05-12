# Loss Function Memento (`provekit-plugin/1`, kind = `"loss-function"`)

**Status:** v1.0.0 normative draft. Second consumer of the universal plugin protocol.
**Date:** 2026-05-12
**Author:** T Savo
**Related:**
- `2026-05-12-plugin-protocol.md` (the protocol this spec consumes; NORMATIVE)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization)
- `2026-05-12-sugar-dict-memento.md` (first co-consumer; consults the loaded loss function at selection time)
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3 (the `loss-record` shape this spec orders)
- `2026-05-15-concept-hub-abstraction-layer.md` §2.4 (loss-record dimensions normative)
- `2026-05-13-compound-contract-memento.md` (compound verdicts whose loss-records are scored by the loaded loss function)

## §1. Purpose

A loss function is a content-addressed scorer over `loss-record` candidates. The substrate produces multiple candidate emissions, transports, or compositions, each carrying a loss-record (per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3); the loss function picks among them. Selection algorithms across the substrate (sugar-dict ranking, transport-gap resolution ranking, lossy-morphism acceptance, compound-aggregation tie-breaking) all consult the loaded loss function.

The default loss function uses the substrate's preorder over `loss-dimension` (§6). Loss functions are PLUGGABLE so users can:

- Re-weight dimensions (e.g., a safety-critical project weights `ub_introduction` as effectively infinite).
- Add custom dimensions and rank them (extension labels per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3).
- Implement domain-specific scoring via JSON-RPC.

### §1.1 What a loss function is NOT

- Not a discharge backend. A loss function does NOT prove anything; it RANKS already-discharged candidates by their characterized loss.
- Not a budget. A budget (the per-dimension limits a project sets on acceptable loss) is a separate memento type (`LossBudgetMemento`, deferred to a follow-up). The loss function ORDERS; the budget REFUSES. Both consult the same loss-record.
- Not policy. The loss function is pure: same loss-record inputs produce the same total order. Policy decisions about which losses are acceptable live in the budget layer.

### §1.2 Trichotomy mapping

| Outcome                 | Condition                                                                                              |
|-------------------------|--------------------------------------------------------------------------------------------------------|
| `exact`                 | The loss function produced a total order; one candidate is the unique minimum.                         |
| `loudly-bounded-lossy`  | The loss function produced a partial order with non-trivial ties; tie-break per §3.4 (deterministic).  |
| `refuse`                | The loss function's `algorithm` is unknown to the runtime AND no `custom-rpc` fallback was loaded.     |

## §2. The `content` payload

The `content` payload of a loss-function plugin memento (`kind = "loss-function"`, per `2026-05-12-plugin-protocol.md` §1) is:

```cddl
; Imports:
;   loss-record       ; from 2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.3
;   loss-dimension    ; from same
;   ir-formula        ; from 2026-04-30-ir-formal-grammar.md
;   cid               ; "blake3-512:" tstr

; Locked JCS key order: algorithm, algorithm_params, function_name
loss-function-content = {
  algorithm:        algorithm-name,
  algorithm_params: json-value,                  ; CDDL'd per §3 per algorithm
  function_name:    tstr                         ; free-form label, e.g. "default", "security-critical", "perf-tolerant"
}

; Open enum. v1.0.0 wires "lexicographic-preorder", "weighted-sum", and
; "custom-rpc". Unknown algorithms MUST be rejected at load (refuse with
; reason_kind = "loss-function-unknown-algorithm" per
; 2026-05-12-plugin-protocol.md §8.1).
algorithm-name = "lexicographic-preorder"
               / "weighted-sum"
               / "custom-rpc"
               / tstr

; ----- algorithm-specific params -----

; Locked JCS key order: dimension_order, tie_break
lexicographic-preorder-params = {
  dimension_order: [+ loss-dimension],           ; ordered priority list (highest priority first)
  tie_break:       "by-dimension-name-ascending" / "by-formula-cid-ascending"
}

; Locked JCS key order: tie_break, weights, zero_dimension_treatment
weighted-sum-params = {
  tie_break:                "by-dimension-name-ascending" / "by-formula-cid-ascending",
  weights:                  { * loss-dimension => uint },    ; per-dimension weight; uint to keep JCS-canonical
  zero_dimension_treatment: "absent-is-zero" / "absent-is-infinity"
}

; Locked JCS key order: endpoint, timeout_ms
custom-rpc-params = {
  endpoint:    tstr,                              ; JSON-RPC endpoint per 2026-05-12-plugin-protocol.md §4.1
  timeout_ms:  uint                               ; per-call timeout; runtime MUST refuse on timeout
}
```

### §2.1 Field semantics

| Field                                  | Required | Meaning |
|----------------------------------------|----------|---------|
| `algorithm`                            | yes      | The scoring algorithm shape. v1.0.0 canonical values listed; unknown values are a refuse at load. |
| `algorithm_params`                     | yes      | The per-algorithm CDDL'd payload. The runtime MUST validate `algorithm_params` against the CDDL for the declared `algorithm`. |
| `function_name`                        | yes      | Free-form label. Part of the plugin CID (different names produce different CIDs). |

### §2.2 Scoring contract

A loss function defines a deterministic total order on the set of loss-records the substrate produces during a run. The contract:

- **Determinism.** Same loss-records in same order produce the same total order. The runtime MUST NOT randomize, time-slice, or otherwise non-determinize the score.
- **Transitivity.** If `score(A) < score(B)` and `score(B) < score(C)`, then `score(A) < score(C)`.
- **Empty-loss is minimum.** A loss-record with no dimensions (the empty record `{}`) MUST score strictly less than any non-empty loss-record under EVERY loaded loss function. Algorithms that cannot guarantee this MUST be refused at load.

The third bullet enforces Supra omnia rectum at the scoring layer: a lossless candidate MUST win against any lossy candidate, regardless of how the lossy candidate's dimensions are weighted. Algorithms that violate this are refused.

## §3. Built-in algorithms

### §3.1 `"lexicographic-preorder"`

Params: an ordered list of loss-dimension names plus a tie-break rule.

Scoring procedure for two loss-records `A` and `B`:

1. For each dimension `d` in `dimension_order` (highest priority first):
   - Let `a_d = A[d]` if present, else `false` (the empty-formula sentinel).
   - Let `b_d = B[d]` if present, else `false`.
   - If `a_d == false` and `b_d != false`, A wins (A has no loss in `d`, B does).
   - If `b_d == false` and `a_d != false`, B wins.
   - If both are non-`false` and their JCS-canonical bytes differ, the candidate with the LOWER JCS-canonical-bytes-as-CID wins. (Rationale: a formula's CID is its canonical identity; no semantic comparison of formula strength is implied by this rule. The `tie_break` field of the params can override this default for the equal-CID degenerate.)
   - If both are byte-identical, continue to the next dimension.
2. If every dimension agreed, apply `tie_break` (`by-dimension-name-ascending` or `by-formula-cid-ascending`).

Empty-loss-is-minimum (§2.2): the empty record has every dimension absent. By step 1, the empty record wins against any record with even one non-`false` dimension at the highest-priority dimension that differs. The contract holds.

### §3.2 `"weighted-sum"`

Params: a per-dimension weights map (uint weights for JCS-canonicality) plus a `zero_dimension_treatment` choice plus a tie-break rule.

Scoring procedure for a loss-record `R`:

1. For each dimension `d` in `R`:
   - If `R[d] == false`, add `0` to the score.
   - Else add `weights[d]` to the score. If `d` is not in `weights`:
     - If `zero_dimension_treatment == "absent-is-zero"`, add `0`.
     - If `zero_dimension_treatment == "absent-is-infinity"`, the score is INFINITE; this candidate is effectively refused unless every other candidate is also infinite (then tie-break).
2. Sum the per-dimension contributions.

Lower score wins. Tie-break per `tie_break`.

Empty-loss-is-minimum (§2.2): the empty record sums to `0`. Any non-empty record sums to at least the smallest non-zero weight (or infinity if any dimension is unweighted under `absent-is-infinity`). The contract holds iff every weight is strictly positive AND `zero_dimension_treatment` is consistent. A `weighted-sum` plugin with a zero weight on a dimension MUST be refused at load (the runtime MUST detect this by walking `weights` at load time).

### §3.3 `"custom-rpc"`

Params: an RPC endpoint plus a timeout.

The runtime delegates the entire ranking to the RPC plugin via `provekit.plugin.invoke` (per `2026-05-12-plugin-protocol.md` §4.2.2). The `params` shape for `invoke`:

```json
{
  "candidates": [
    { "candidate_id": "<opaque>", "loss_record": { /* loss-record */ } }
  ]
}
```

The expected `result` shape:

```json
{
  "total_order": ["<candidate_id_winning>", "<...>", "<candidate_id_losing>"]
}
```

The result MUST be a permutation of the input `candidate_id`s. The runtime MUST verify this; any deviation is a refuse.

The runtime MUST verify empty-loss-is-minimum POST-HOC: if the input contained an empty-loss-record candidate, that candidate MUST appear FIRST in `total_order`. Violation is a refuse.

RPC timeout per `timeout_ms` is a refuse (`reason_kind = "rpc-timeout"` per `2026-05-12-plugin-protocol.md` §8.1).

### §3.4 Tie-break determinism

When two candidates score equal under the algorithm's primary procedure, the tie-break MUST be deterministic:

- `"by-dimension-name-ascending"`: the candidate whose loss-record's first lexicographically-named dimension's formula has the lower JCS-canonical-bytes-as-CID wins.
- `"by-formula-cid-ascending"`: sort each candidate's non-`false` formulas by their CIDs ascending, then compare the resulting CID sequences pairwise; the candidate with the lower-CID sequence wins.

If candidates STILL tie after the tie-break (byte-identical loss-records), they are SEMANTICALLY THE SAME and the runtime MAY pick either; the choice MUST be deterministic across runs (e.g., the candidate-ID lexicographic order applied by the caller before invocation).

## §4. Composition

### §4.1 Multi-load via CLI

Multiple loss-function plugins MAY be loaded. The composition rule: the loaded loss functions are composed via LEXICOGRAPHIC STACKING in CLI flag order. For two candidates A and B and loss functions `f_1, f_2, ..., f_n`:

1. Compare A and B under `f_1`. If a winner emerges, return it.
2. Else, compare under `f_2`. If a winner, return.
3. ... continue.
4. If every `f_i` produced a tie, apply the LAST loss function's `tie_break` rule globally.

Rationale: the user's first `--loss-function` flag is their PRIMARY priority; subsequent flags break ties. This mirrors the `lexicographic-preorder` algorithm one level up.

### §4.2 Composed memento (alternative form)

A user MAY declare a SINGLE composed loss function as one plugin memento by writing a `lexicographic-preorder` algorithm whose `dimension_order` encodes the concatenation of multiple loss functions' priority lists. v1.0.0 does NOT define a higher-order `"composed"` algorithm explicitly; it is left to follow-up specs to define a `LossFunctionCompositionMemento` that takes a sequence of loss-function CIDs.

## §5. CLI surface

Per `2026-05-12-plugin-protocol.md` §3 and §7:

```
--plugin loss-function:<source>     # canonical
--loss-function <source>            # per-kind alias
```

Repeated loads compose (§4.1). The order is preserved into the registry's `load_order` and is consulted for composition (§4.1).

Additional loss-function-specific flags:

| Flag                              | Effect                                                                                                |
|-----------------------------------|-------------------------------------------------------------------------------------------------------|
| `--no-default-loss-function`      | Suppresses the built-in default. The user MUST supply at least one `--loss-function`.                |
| `--explain-loss-decisions`        | At every loss-function-consulting selection, emit a `LossDecisionMemento` recording inputs and pick.  |

## §6. The default loss function

When no `--loss-function` flag is supplied AND `--no-default-loss-function` is NOT set, the runtime registers a built-in default with the following declared content (per `2026-05-12-plugin-protocol.md` §6.3, the default MUST be content-addressable at the same CID a user would compute from the equivalent JSON):

```json
{
  "algorithm": "lexicographic-preorder",
  "algorithm_params": {
    "dimension_order": [
      "ub_introduction",
      "effect_divergence",
      "domain_narrowing",
      "value_divergence",
      "structural_divergence"
    ],
    "tie_break": "by-formula-cid-ascending"
  },
  "function_name": "default"
}
```

Rationale for the priority ordering:

1. `ub_introduction` first: introducing undefined behavior is the worst dimension; per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3, "a project porting safety-critical code might tolerate `value_divergence` (it knows its inputs are bounded) and absolutely refuse `ub_introduction`." The default reflects "absolutely refuse" via highest priority.
2. `effect_divergence` next: introducing or losing a named effect changes observable behavior in ways the contracts cannot recover from.
3. `domain_narrowing` next: refusing inputs the source op accepted is a domain change but is at least loudly observable at the call site.
4. `value_divergence` next: a different value on a characterized input set is the most recoverable form of loss (relations across the boundary CAN be written down).
5. `structural_divergence` last: cosmetic / prose / comment-level losses; the substrate's lowest-stakes dimension.

The default loss function's plugin memento header (full bytes, ready for JCS + BLAKE3-512):

```json
{
  "content": {
    "algorithm": "lexicographic-preorder",
    "algorithm_params": {
      "dimension_order": [
        "ub_introduction",
        "effect_divergence",
        "domain_narrowing",
        "value_divergence",
        "structural_divergence"
      ],
      "tie_break": "by-formula-cid-ascending"
    },
    "function_name": "default"
  },
  "critical": false,
  "kind": "loss-function",
  "protocol_versions": ["provekit-plugin/1"],
  "provenance_cid": "blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "schemaVersion": "1",
  "version": "1.0.0"
}
```

The CID of this header is `"blake3-512:" ++ hex(BLAKE3-512(JCS(<header bytes with cid elided>)))` per `2026-05-12-plugin-protocol.md` §6.1. The bytes above are intentionally complete; a reader running JCS + BLAKE3-512 MUST produce a determinate CID. The byte-exact CID-pinning test lives in a follow-up implementation crate.

## §7. Worked example: security-focused loss function

A safety-critical project loads a custom loss function that refuses any candidate introducing unbounded side-effects. The function uses `weighted-sum` with `effect_divergence` weighted at an effectively-infinite scaling.

```json
{
  "content": {
    "algorithm": "weighted-sum",
    "algorithm_params": {
      "tie_break": "by-formula-cid-ascending",
      "weights": {
        "domain_narrowing": 10,
        "effect_divergence": 1000000000,
        "structural_divergence": 1,
        "ub_introduction": 1000000000,
        "value_divergence": 100
      },
      "zero_dimension_treatment": "absent-is-zero"
    },
    "function_name": "security-critical"
  },
  "critical": true,
  "kind": "loss-function",
  "protocol_versions": ["provekit-plugin/1"],
  "provenance_cid": "blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "schemaVersion": "1",
  "version": "1.0.0"
}
```

Note `critical: true`: this project DEPENDS on this loss function; if it fails to load, the run MUST refuse rather than silently degrade to the default (§8 of `2026-05-12-plugin-protocol.md`).

Effect: any candidate whose loss-record has a non-`false` `effect_divergence` or `ub_introduction` formula contributes `1,000,000,000` per non-`false` dimension to the score. Any candidate with even one such dimension is effectively unselectable against any candidate without one. The user has installed a near-infinity weight without using `"absent-is-infinity"` (which would refuse silently); the score is finite but dominating. The function still satisfies empty-loss-is-minimum (§2.2): the empty record sums to `0` and wins.

### §7.1 Selection under the security-focused loss function

Given two candidates:
- Candidate A: loss-record `{ "structural_divergence": <some_formula> }` (comment, score = `1`).
- Candidate B: loss-record `{ "effect_divergence": <unbounded_alloc> }` (a transport that introduces an unaccounted allocation, score = `1000000000`).

A wins by ~1e9 to 1. The user PREFERS a documented comment-only emission (lossy in the cosmetic dimension) over an emission that introduces unbounded effects, regardless of any other consideration. This is the kill-switch outcome the priority weighting enforces.

### §7.2 Composition with the default

If the user loads BOTH `--loss-function security-critical.json --loss-function default.json` (in that order), composition (§4.1) applies: security-critical decides first; default breaks any security-critical ties. A candidate that the security-critical function scores equal (e.g., two candidates with identical loss-records) is then ranked by the default's lexicographic-preorder, which provides a fully deterministic total order. The two-stage composition gives the security-critical function veto authority while the default supplies the deterministic substrate beneath.

## §8. `LossDecisionMemento` (audit form)

Under `--explain-loss-decisions`, every loss-function-consulting selection emits a `LossDecisionMemento`:

```cddl
; Locked JCS key order: candidates, cid, decided_at, kind, loss_function_cid,
; schemaVersion, winner_candidate_id
loss-decision-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,
    signer:     pubkey
  },
  header: {
    candidates:           [+ { candidate_id: tstr, loss_record: any, score: tstr } ],   ; score is the loss function's per-candidate scalar (stringified to keep JCS-canonical for non-uint scores)
    cid:                  cid,
    decided_at:           iso8601,
    kind:                 "loss-decision",
    loss_function_cid:    cid,                  ; which loaded loss function decided
    schemaVersion:        "1",
    winner_candidate_id:  tstr
  },
  metadata: { ? note: tstr }
}
```

The CID construction follows the standard pattern per `2026-04-30-canonicalization-grammar.md`. `LossDecisionMemento`s SHOULD be aggregated into the run's provenance chain via the `PluginRegistryMemento` (per `2026-05-12-plugin-protocol.md` §9.4): a verifier replaying the run with the same registry MUST see the same loss-decisions emerge.

## §9. Cross-references

- The `loss-record` and `loss-dimension` shapes scored by this spec are normative per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3.
- The dimension catalogue and discharge interactions are elaborated in `2026-05-15-concept-hub-abstraction-layer.md` §2.4.
- The plugin memento envelope, CID rules, load procedure, and registry semantics are NORMATIVE per `2026-05-12-plugin-protocol.md`.
- The sugar-dict consumer that consults this spec at selection time is `2026-05-12-sugar-dict-memento.md` §4.2.
- Compound-aggregation tie-breaking per `2026-05-13-compound-contract-memento.md` §2 MAY consult a loaded loss function in future revisions (out of scope for v1.0.0).

## §10. Out of scope for v1.0.0

- A higher-order `"composed"` algorithm with a `LossFunctionCompositionMemento`.
- The `LossBudgetMemento` (the per-dimension limits a project sets on acceptable loss); this is a separate memento type, deferred.
- Implementation in any runtime. This spec is the WIRE shape and the SCORING contracts; the implementation lands in a follow-up PR.
- Probabilistic / sampling-based loss functions (deterministic only in v1.0.0).
- Cross-run loss-function memoization (every run re-scores; deferred follow-up MAY introduce a cache layer).
