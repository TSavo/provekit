# Parametric Realization Mementos

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Author:** T Savo
**Related:**
- TSavo/provekit#793
- TSavo/provekit#794
- TSavo/provekit#796
- TSavo/provekit#798
- `2026-05-10-realizer-protocol-v2.md`
- `2026-05-12-concept-site-memento.md`
- `2026-05-12-loss-function-memento.md`
- `2026-05-12-sugar-dict-memento.md`
- `2026-05-13-body-template-memento.md`
- `2026-05-13-effect-occurrence-memento.md`
- `2026-05-13-sort-morphism-memento.md`

## §0. Purpose

`ParametricRealizationMemento` defines a reusable catalog template for realizing a concept pattern into a target language pattern without minting one equation for every concrete sort tuple. `RealizationPlanMemento` records the per-site selection that instantiates that template for one `ConceptSiteMemento`.

The two mementos have distinct roles:

- `ParametricRealizationMemento` is catalog algebra. It states that a pattern such as `concept:option<T>` can realize as `java:Optional<U>`, names the sort-morphism slots required to bind `T` to `U`, and points at emission templates and sugar dictionaries.
- `RealizationPlanMemento` is a selection receipt. It records which parametric realization was selected at a site, which sort morphisms filled the slots, which effects were transformed, which candidate set and loss function were used, and what total loss was accepted.

### §0.1 Two-stage composition rationale

Concept realizations must compose without expanding the catalog into one equation per `(concept, sort tuple, language)` instance. For a site whose concept shape is `concept:option<i64>` and whose target is Java, the substrate uses two stages:

1. Look up the parametric concept realization `concept:option<T>` to `java:Optional<U>`.
2. Fill the required sort-morphism slot with a `SortMorphismMemento` such as `i64` to `long`.

The composed realization identity is therefore the tuple `(parametric_realization_cid, [sort_morphism_cid])`. It is NOT a separately minted realization equation. This keeps the catalog linear in reusable facts rather than multiplicative across every concrete sort substitution.

## §1. Wire shapes

The CDDL below is normative for the two content objects. All object keys MUST be JCS-canonicalized in alphabetical order before CID construction. Arrays preserve their declared semantic order.

### §1.1 ParametricRealizationMemento

```cddl
; Imports:
;   cid        ; "blake3-512:" tstr
;   json-value ; any JSON value
;
; Locked JCS key order: body_template_cids, concept_pattern,
; effect_transform_slots, loss_record_template, provenance_cid,
; required_sort_morphism_slots, sugar_cids, target_pattern, type_variables

parametric-realization-memento = {
  body_template_cids:            [* cid],
  concept_pattern:               json-value,
  effect_transform_slots:        [* effect-slot-descriptor],
  loss_record_template:          json-value,
  provenance_cid:                cid,
  required_sort_morphism_slots:  [+ slot-descriptor],
  sugar_cids:                    [* cid],
  target_pattern:                json-value,
  type_variables:                [+ tstr],
}

; Locked JCS key order: slot_name, source_type_variable, target_type_variable
slot-descriptor = {
  slot_name:              tstr,
  source_type_variable:   tstr,
  target_type_variable:   tstr,
}

; Locked JCS key order: concept_effect, slot_name, target_effect
effect-slot-descriptor = {
  concept_effect: tstr,
  slot_name:      tstr,
  target_effect:  tstr,
}
```

### §1.2 RealizationPlanMemento

```cddl
; Imports:
;   cid        ; "blake3-512:" tstr
;   json-value ; any JSON value
;
; Locked JCS key order: candidate_set_cid, concept_site_cid,
; effect_occurrence_transform, loss_function_cid, observation_wrapper_cid,
; provenance_cid, selected_candidate_cid, selected_realization_cid,
; sort_morphism_cids, total_loss_record

realization-plan-memento = {
  candidate_set_cid:             cid,
  concept_site_cid:              cid,
  effect_occurrence_transform:   json-value,
  loss_function_cid:             cid,
  observation_wrapper_cid:       nil / cid,
  provenance_cid:                cid,
  selected_candidate_cid:        cid,
  selected_realization_cid:      cid,
  sort_morphism_cids:            [* cid],
  total_loss_record:             json-value,
}
```

## §2. Field semantics

### §2.1 ParametricRealizationMemento fields

| Field | Required | Meaning |
|---|---:|---|
| `body_template_cids` | yes | Ordered list of `BodyTemplateMemento` CIDs available to emit the target-side body for this realization. Empty means this realization has no body template attached. |
| `concept_pattern` | yes | JCS-canonical JSON pattern for the concept side, for example `{ "args": ["T"], "head": "concept:option" }`. Pattern language semantics are out of scope in §11. |
| `effect_transform_slots` | yes | Ordered descriptors for mapping concept-side effect slots to target-side effect slots. Empty means this realization does not transform effects. |
| `loss_record_template` | yes | JCS-canonical JSON template naming the loss dimensions that must be instantiated while minting a plan. Empty object `{}` means no intrinsic loss template. |
| `provenance_cid` | yes | CID for the provenance statement supporting the catalog template. |
| `required_sort_morphism_slots` | yes | Ordered non-empty list of type-variable pairings that MUST be filled by `SortMorphismMemento` CIDs before the realization can be used. |
| `sugar_cids` | yes | Ordered list of `SugarDictMemento` CIDs available when this realization emits target-side clauses. Empty means no sugar dictionary is attached. |
| `target_pattern` | yes | JCS-canonical JSON pattern for the target-language side, for example `{ "args": ["U"], "head": "java:Optional" }`. |
| `type_variables` | yes | Non-empty ordered list of type-variable names in scope for `concept_pattern`, `target_pattern`, slots, and templates. |

Each `slot-descriptor` names one required sort-morphism slot. `slot_name` is the stable handle used by plans and diagnostics. `source_type_variable` names the concept-side variable. `target_type_variable` names the target-side variable. A descriptor whose variable names are not present in `type_variables` is invalid and MUST be refused at load.

Each `effect-slot-descriptor` names one effect transform. `concept_effect` is the source occurrence or occurrence class. `target_effect` is the target occurrence or occurrence class. `slot_name` is the stable handle used when constructing `effect_occurrence_transform`.

### §2.2 RealizationPlanMemento fields

| Field | Required | Meaning |
|---|---:|---|
| `candidate_set_cid` | yes | CID of the candidate set considered before selection. It MUST include every candidate admitted by §5 for this site. |
| `concept_site_cid` | yes | CID of the `ConceptSiteMemento` being realized. |
| `effect_occurrence_transform` | yes | JCS-canonical JSON mapping from concrete concept-side effect occurrences at this site to concrete target-side effects or wrappers. Empty object `{}` means no site effects were transformed. |
| `loss_function_cid` | yes | CID of the `LossFunctionMemento` used to rank candidates. This is part of the plan so replay uses the same scorer. |
| `observation_wrapper_cid` | yes | CID of the wrapper used for witness, monitor, emitter, gate, or legacy dispatcher emission, or `null` when no wrapper is emitted. The wrapper MUST NOT mutate the wrapped function contract memento's `effects`. |
| `provenance_cid` | yes | CID for the provenance statement supporting this site-specific selection. |
| `selected_candidate_cid` | yes | CID of the selected candidate from `candidate_set_cid`. This records the concrete winner after slot filling and loss calculation. |
| `selected_realization_cid` | yes | CID of the selected `ParametricRealizationMemento`. |
| `sort_morphism_cids` | yes | Ordered list of `SortMorphismMemento` CIDs. It MUST have the same length and order as `selected_realization.required_sort_morphism_slots`. |
| `total_loss_record` | yes | JCS-canonical JSON loss record after combining the realization's intrinsic loss template, sort-morphism losses, effect transforms, template losses, and wrapper losses. |

## §3. Two-stage composition flow

The orchestrator MUST execute realization in this order:

1. **Parametric lookup.** Match the site's concept shape against each loaded `ParametricRealizationMemento.concept_pattern`. Drop non-matches. A candidate whose target pattern cannot be made compatible with the requested target language is a non-match.
2. **Sort-morphism instantiation.** For each remaining candidate, solve every `required_sort_morphism_slots` entry against the site's concrete sort tuple and the target pattern's concrete sort tuple. Resolve each slot to one `SortMorphismMemento` CID. If any slot has no admissible fill, drop the candidate or refuse if no candidates remain.
3. **Effect transform.** Instantiate `effect_transform_slots` against the site's concrete effect occurrences and write the concrete mapping into `effect_occurrence_transform`. If an effect slot cannot be filled without inventing an unrecorded effect, drop the candidate or refuse if no candidates remain.
4. **Loss-function select.** Form a candidate set with all candidates that survived matching, slot filling, and effect transformation. Compute each candidate's loss record, then rank the set with the selected `LossFunctionMemento`.
5. **Plan mint.** If there is a unique winning candidate, mint a `RealizationPlanMemento` recording the selected parametric realization CID, ordered sort-morphism CIDs, concrete effect transform, candidate set CID, loss-function CID, selected candidate CID, total loss record, optional observation wrapper CID, and provenance CID.

The result is a site-local plan. It does not create a new catalog equation for the fully instantiated concept and sort tuple.

## §4. Realize-side lookup responsibility

The orchestrator owns lookup and selection. It reads the concept site, enumerates applicable parametric realizations, fills sort-morphism slots, scores candidates, and mints the `RealizationPlanMemento`.

The language kit owns emission from the plan. It MUST consume the selected plan, resolve the referenced `ParametricRealizationMemento`, `SortMorphismMemento` list, body templates, sugar dictionaries, and optional wrapper, then emit target-language artifacts. A language kit MUST NOT silently choose a different parametric realization or replace the ordered sort-morphism list during emission. If it cannot honor the plan, it MUST refuse.

## §5. Candidate-set formation

Multiple parametric realizations MAY apply to one site. For example, a target language might offer an idiomatic optional type, a nullable encoding, and a tagged union encoding. All candidates that match the concept pattern, can fill every required sort-morphism slot, and can instantiate required effect transforms MUST be represented in the candidate set before scoring.

The `candidate_set_cid` is the content address of that pre-selection set. The set MUST be deterministic: same catalog inputs, same site, same target request, same policies, and same loss function yield the same candidate members and ordering. Candidate ordering is part of the CID only to make replay byte-identical; it MUST NOT be used as an implicit semantic tiebreaker unless a referenced policy explicitly says so.

## §6. Loss-function selection recording

The selected `LossFunctionMemento` CID is mandatory in every plan. Selection is not replayable if the scorer is implicit, version-dependent, or inferred from local configuration. A verifier MUST resolve `loss_function_cid` and verify that `selected_candidate_cid` is a unique minimum over `candidate_set_cid` under that loss function.

If the referenced loss function is unavailable, unknown, nondeterministic, or rejects its own parameters, plan verification MUST refuse. If the loss function reports multiple equally-low-loss candidates and there is no explicit tie-break policy, the orchestrator MUST refuse instead of minting a plan.

## §7. Observation wrapper emission

Witness, monitor, emitter, gate, and legacy dispatcher emission is represented by `observation_wrapper_cid`. When absent, the field value is `null`. When present, it names a wrapper artifact that observes, witnesses, emits, enforces, dispatches, or records boundary behavior around the realized object function.

The wrapper's effects belong to the wrapper. They MUST NOT be inserted into, removed from, or otherwise used to mutate the wrapped object function's `FunctionContractMemento.effects`. This follows TSavo/provekit#793: observer effects belong to wrappers, not wrapped programs. A plan or emitter that needs observer behavior MUST attach a wrapper and cite it through `observation_wrapper_cid`.

## §8. Fail-closed rules

The realization machinery is fail-closed:

- If `concept_pattern` does not match the `ConceptSiteMemento`, the candidate is inapplicable.
- If any required sort-morphism slot cannot be filled, the candidate is inapplicable.
- If any required effect transform slot cannot be filled, the candidate is inapplicable.
- If no candidate remains after inapplicable candidates are dropped, the orchestrator MUST refuse.
- If multiple equally-low-loss candidates remain and no deterministic tiebreaker is recorded, the orchestrator MUST refuse.
- A plan MAY admit a tie only when an explicit `PolicyMemento` per TSavo/provekit#798 records the tie-break policy and the policy CID is included in the candidate-set or provenance material used to mint the plan.
- If the selected candidate is not a member of `candidate_set_cid`, the plan is invalid and MUST be refused.
- If `sort_morphism_cids.length` differs from `required_sort_morphism_slots.length`, the plan is invalid and MUST be refused.

## §9. CID construction

Both content objects are content-addressed from JCS-canonical bytes using BLAKE3-512:

```text
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS(memento_content)))
```

Objects MUST use alphabetical key order at every object level before hashing. Arrays whose order is semantic, including `required_sort_morphism_slots`, `effect_transform_slots`, `body_template_cids`, `sugar_cids`, and `sort_morphism_cids`, MUST preserve that order. Arrays that represent unordered pools inside a separately minted candidate-set object MUST define a deterministic ordering before hashing.

Producers MUST NOT hash pretty-printed JSON bytes, host-language map iteration order, or non-JCS encodings. Verifiers MUST recompute JCS bytes and BLAKE3-512 before accepting a CID.

## §10. Cross-references

- TSavo/provekit#793: observer effects belong to wrappers, not wrapped programs. This spec applies that rule through `observation_wrapper_cid`.
- TSavo/provekit#794 and `2026-05-13-sort-morphism-memento.md`: required sort-morphism slots are filled by `SortMorphismMemento` CIDs.
- TSavo/provekit#796: this spec is part of the admissibility spine and preserves fail-closed selection.
- TSavo/provekit#798: explicit policy mementos may record tie-break policies when equal-loss candidates are admitted.
- `2026-05-12-sugar-dict-memento.md`: `sugar_cids` point at sugar dictionaries used during target-side clause rendering.
- `2026-05-12-loss-function-memento.md`: `loss_function_cid` records the scorer used to select the winning candidate.
- `2026-05-13-body-template-memento.md`: `body_template_cids` point at body templates available for emission.
- `2026-05-10-realizer-protocol-v2.md`: this spec refines the realizer's selection receipt for parametric concept realizations.

## §11. Out of scope

This spec intentionally does not define:

- The parametric-pattern unification algorithm.
- The loss-function evaluation algorithm.
- Sugar-dict semantics.
- The candidate-set memento wire shape beyond the requirement that `candidate_set_cid` identify the pre-selection pool.
- The observation-wrapper memento wire shape beyond the requirement that wrappers do not mutate the wrapped function contract's effects.
