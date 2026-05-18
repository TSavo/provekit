# Policy Profile Memento (`policy-profile/1`)

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-14
**Related:** `2026-05-14-witness-consensus-promotion-v1.1-consensus-vector.md`, `2026-05-13-policy-memento.md`, `2026-05-12-sugar-dict-memento.md`, `2026-05-18-sugar-selection-policy-memento.md`, `2026-05-14-contract-comment-sugar.md`, `project_provekit_honesty_gradient.md` (#856).

## Motivation

Policy mementos define individual gates. A `ConsensusPolicyMemento` says how a witness consensus vector is judged. A `SugarSelectionPolicyMemento` says how loss and mode coverage are judged for the sugar selection lane. An emission policy says which runtime wrapper mode a realization may emit.

A run, however, does not choose one gate in isolation. It chooses a profile: local smoke checks often want permissive witness floors and gate wrappers, while production deployment often wants stricter witness diversity and monitor wrappers. If those choices live in CLI defaults, the audit trail loses the policy input that made two runs differ.

`PolicyProfileMemento` is the cross-decision bundle. A consumer cites a profile CID and gets back the concrete policy CID and threshold set for each decision lane: witness consensus, sugar selection, and emission gating. The profile is content-addressed so "smoke" and "prod" are not names with hidden meaning. They are signed bytes.

## Locus Choice

This spec introduces a new memento family rather than extending every existing policy memento with a `profile` field.

That is the least leaky shape. Existing policy mementos are gate-local facts: they define one predicate and one decision payload schema. A profile is not a predicate. It is an index over multiple predicates plus the runtime emission posture the caller selected. Putting `profile` inside each gate policy would duplicate the same smoke/prod fact across unrelated memento families and still would not provide a single CID a caller can cite before the run starts.

The profile therefore sits above the gate policies. It cites them by CID and records the per-decision thresholds that the consumer is about to enforce.

## Wire Shape

```cddl
; Imports:
;   cid
;
; Locked JCS key order: cid, decisions, kind, name, schemaVersion
policy-profile-memento = {
  cid:           cid,                  ; DERIVED, see §3
  decisions:     [3* policy-profile-decision],
  kind:          "policy-profile",
  name:          tstr,
  schemaVersion: "1"
}

profile-decision-kind = "witness-consensus"
                      / "sugar-selection"
                      / "emission-gating"
                      / namespaced-kind

; Locked JCS key order: decision_kind, emission_mode, policy_cid,
; required, requires_witnessed_decision, thresholds
policy-profile-decision = {
  decision_kind:                 profile-decision-kind,
  ? emission_mode:               "witness" / "monitor" / "emitter" / "gate" / tstr,
  policy_cid:                    cid,
  required:                      bool,
  requires_witnessed_decision:   bool,
  thresholds:                    [+ policy-profile-threshold]
}

; Locked JCS key order: axis, predicate
policy-profile-threshold = {
  axis:      tstr,
  predicate: tstr
}
```

The three canonical `decision_kind` values are required in every v1 profile:

1. `witness-consensus`
2. `sugar-selection`
3. `emission-gating`

Unknown bare decision kinds fail closed. Extension decision kinds MUST be namespaced as `<namespace>:<kind>`.

## Field Discipline

`policy_cid` points at the gate-local policy memento that owns detailed replay. For `decision_kind = "sugar-selection"`, it points at `SugarSelectionPolicyMemento` as specified in `2026-05-18-sugar-selection-policy-memento.md`. The profile does not replace that policy. It gives a caller one content-addressed input that resolves to the policies used by all decisions in the run.

`thresholds` is a query-friendly projection of the gate-local policy. A profile registry validates only the small predicate grammar (`metric>=N`, `metric<=N`, `metric==N`, `metric>N`, `metric<N`). Full policy replay remains the job of the referenced policy.

`emission_mode` is meaningful for `decision_kind = "emission-gating"`. Smoke profiles typically set it to `gate`: fail fast in local CI. Production profiles typically set it to `monitor`: observe and report without turning deployment into a runtime abort surface.

`requires_witnessed_decision` MUST be `true` for `emission-gating`. A runtime wrapper choice is itself a substrate decision. The profile must require a witnessed emission decision rather than letting a kit silently choose gate, monitor, witness, or emitter from local defaults.

## CID Construction

`cid` is DERIVED. Producers compute it as:

```text
cid_input = JCS(policy-profile-memento with cid elided, decisions sorted ascending by decision_kind)
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

`name` is a label, not identity. Changing any policy CID, threshold, required flag, witnessed-decision flag, emission mode, or canonical decision set changes the profile CID.

## Registry Behavior

`PolicyProfileRegistry` indexes profiles by `cid`. On admission it MUST:

1. validate `kind = "policy-profile"` and `schemaVersion = "1"`;
2. recompute `cid`;
3. require exactly one entry for each canonical decision kind;
4. require non-empty `thresholds` for every decision;
5. reject malformed threshold predicates;
6. reject invalid or non-content-addressed `policy_cid` values; and
7. reject `emission-gating` entries whose `requires_witnessed_decision` is not `true`.

The registry does not decide whether a run is admitted. It resolves a profile CID into the per-decision policy inputs that downstream consensus, sugar, and emission registries evaluate.

## Reference Profiles

The repository carries two reference profiles under `protocol/policies/`:

- `smoke.json`: permissive witness floor, `gate` emission mode, witnessed emission decision required.
- `prod.json`: stricter witness diversity and sample depth, `monitor` emission mode, witnessed emission decision required.

They are examples and stable test vectors. They are not universal truth. Consumers can mint stricter or looser profiles by changing the policy CIDs and thresholds, which naturally produces a different profile CID.

## What This Is Not

This spec does not define a new consensus policy. It uses the consensus-policy surface from `witness-consensus/1.1`.

This spec does not define sugar selection scoring. It cites `SugarSelectionPolicyMemento`, the sugar-selection policy that owns that evaluation.

This spec does not make `smoke` or `prod` magic strings. They are ordinary profile mementos with ordinary content CIDs.

This spec does not let a profile hide runtime emission defaults. Emission gating is always witnessed in v1.
