# Sugar Selection Policy Memento (`sugar-selection-policy/1`)

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-18
**Related:** `2026-05-12-sugar-dict-memento.md`, `2026-05-12-loss-function-memento.md`, `2026-05-14-policy-profile-memento.md`, `2026-05-13-promotion-decision-memento.md`, `2026-04-30-canonicalization-grammar.md`, `TSavo/sugar#889`.

## Purpose

`SugarSelectionPolicyMemento` is the content-addressed declaration of the sugar selection policy used by a realization consumer. Vendors can ship and federate their own sugar selection policies as mementos, then cite those CIDs from profiles, receipts, and promotion records.

This memento does not define a new selection algorithm. It codifies the mechanism already specified in `2026-05-12-sugar-dict-memento.md` §4: enumerate matching sugar entries, score each candidate with a loaded loss function, rank candidates, apply the deterministic tie-break, and emit according to the selected mode.

The policy is a sibling in the PolicyMemento family because it is policy input, not sugar content. A sugar dict declares surfaces. A sugar selection policy declares which loaded surfaces are eligible, which are forbidden, how candidates are scored, and which emission posture the consumer applies.

## Wire Shape

```cddl
; Imports:
;   cid

; Locked JCS key order:
;   applies_to, cid, eligible_sugars, forbidden_sugars, kind, mode,
;   schemaVersion, scoring, tie_breaking
sugar-selection-policy-memento = {
  applies_to:        [+ sugar-selection-applies-to],
  cid:               cid,
  ? eligible_sugars: [+ cid],
  ? forbidden_sugars: [+ cid],
  kind:              "sugar-selection-policy",
  mode:              sugar-selection-mode,
  schemaVersion:     "1",
  ? scoring:         cid,
  tie_breaking:      sugar-selection-tie-breaking
}

sugar-selection-mode = "best-only" / "inclusive" / "strict"

; v1.0.0 codifies the deterministic tie-break from sugar-dict §4.4.
sugar-selection-tie-breaking = "load-order-then-entry-index"

; Locked JCS key order: concept, language
sugar-selection-applies-to = {
  concept:  tstr,
  language: tstr
}
```

## Field Semantics

`kind` is the memento discriminator and MUST be `sugar-selection-policy`.

`schemaVersion` is the schema discriminator and MUST be `1`.

`mode` selects the emission posture:

| Mode | Meaning |
|------|---------|
| `best-only` | Emit exactly one selected candidate, the lowest-loss candidate after scoring and tie-breaking. |
| `inclusive` | Emit every applicable candidate that survives enumeration, ordered by scoring and tie-breaking. |
| `strict` | Apply the strict sugar refusal rule from `2026-05-12-sugar-dict-memento.md` §5 when no matching entry exists. |

`scoring` is an optional CID reference to a `LossFunctionMemento`. When absent, the consumer uses the loaded default loss function for the run. When present, the consumer MUST score every sugar candidate through that loss function CID.

`tie_breaking` declares the deterministic tie-breaking strategy. The v1.0.0 canonical value is `load-order-then-entry-index`, meaning:

1. later sugar dicts in registry `load_order` win over earlier dicts;
2. lower entry indexes within the same sugar dict win; and
3. the final impossible ambiguity case follows the refusal or inclusion behavior in sugar-dict §4.4.

`eligible_sugars` is an optional allow-list of sugar-dict CIDs. When present, a consumer MUST consider only candidates from those sugar dicts.

`forbidden_sugars` is an optional deny-list of sugar-dict CIDs. A consumer MUST refuse to emit candidates from those sugar dicts even if they match and score well.

`applies_to` is the non-empty set of per-(concept, language) match criteria for the policy. `concept` is the concept or concept family identifier the policy covers. `language` is the target language or surface family the policy covers.

## Federation

CID construction follows the `PolicyProfileMemento` pattern:

```text
cid_input = JCS(sugar-selection-policy-memento with cid elided)
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

Changing the mode, scoring CID, tie-breaking strategy, allow-list, deny-list, or applicability criteria changes the policy CID.

`PolicyProfileMemento` uses its `decision_kind = "sugar-selection"` lane to cite the applicable `SugarSelectionPolicyMemento` through `policy_cid`. A consumer that resolves a profile CID therefore gets the exact sugar selection policy CID alongside the witness consensus and emission gating policy CIDs.

`PromotionDecisionMemento` MAY cite the `SugarSelectionPolicyMemento` CID applied at promotion time through its policy reference surface, either directly when the promotion decision is the sugar selection decision or through the cited `PolicyProfileMemento` when the decision is replayed as part of a profiled run.

Vendors can federate policy mementos the same way they federate sugar dicts: publish the bytes, publish the CID, and let consumers decide whether to trust or load that policy. The CID names the exact policy input, not a marketplace label.

## Relationship To Sugar Dict §4

`2026-05-12-sugar-dict-memento.md` §4 defines the selection and emission algorithm. This memento turns that algorithm's policy inputs into content-addressed bytes:

1. §4.1 enumeration is constrained by `eligible_sugars`, `forbidden_sugars`, and `applies_to`.
2. §4.2 scoring is pinned by `scoring` when present.
3. §4.3 selection is controlled by `mode`.
4. §4.4 tie-breaking is pinned by `tie_breaking`.
5. §4.5 emission records the selected candidates and can carry this policy CID in the audit trail or promotion record.

The algorithm remains in the sugar dict spec. The selected policy becomes replayable because the consumer can cite this memento CID.

## Validation

Registries admitting a `SugarSelectionPolicyMemento` MUST:

1. validate `kind = "sugar-selection-policy"` and `schemaVersion = "1"`;
2. recompute `cid` with the `cid` field elided;
3. require non-empty `applies_to`;
4. reject malformed `scoring`, `eligible_sugars`, or `forbidden_sugars` CIDs;
5. reject duplicate CIDs inside either sugar list;
6. reject a sugar CID that appears in both `eligible_sugars` and `forbidden_sugars`; and
7. reject unknown `mode` or `tie_breaking` values.

The registry does not evaluate loss records. It resolves the policy input so the sugar selection consumer can replay the sugar-dict §4 mechanism against the loaded registry and loss function.
