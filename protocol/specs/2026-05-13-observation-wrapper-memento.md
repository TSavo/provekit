# ObservationWrapperMemento

**Status:** Normative
**Date:** 2026-05-13
**Related:**
- `2026-05-13-effect-occurrence-memento.md`
- `2026-05-10-realizer-protocol-v2.md`
- admissibility spine #796
- `ParametricRealizationMemento` and `RealizationPlanMemento` #801
- `PromotionDecisionMemento` #791

## §0 Purpose

`ObservationWrapperMemento` is the durable substrate object that records a wrapper relationship between an unchanged object function and a wrapper function that emits observer effects.

The master-frame invariant is: observer effects belong to wrappers, not wrapped programs.

Witness, monitor, and dispatcher machinery emit wrapper-side effects. They do not mutate the wrapped object function's `FunctionContractMemento.effects`.

## §1 Wire Shape

```cddl
observation-wrapper-memento = {
  emitted_artifact_cid: cid,               ; emitted wrapper artifact
  mode: wrapper-mode,                      ; monitor, witness, dispatcher, or namespaced extension
  object_fcm_cid: cid,                     ; object FunctionContractMemento, unchanged by wrapping
  observer_effects: [+ effect-occurrence], ; per #793, live on the wrapper
  preservation_claim_cid: cid,             ; semantic relationship the wrapper preserves
  provenance_cid: cid,
  wrapper_fcm_cid: cid,                    ; wrapper FunctionContractMemento, carries observer effects
}

wrapper-mode = "monitor" / "witness" / "dispatcher" / namespaced-wrapper-mode
namespaced-wrapper-mode = tstr             ; MUST be namespace-qualified by §2
```

The CDDL member order is the locked human-readable form of JCS alphabetical key order.

## §2 Field Semantics

All keys are mandatory. Encoders MUST emit keys in JCS-canonical alphabetical order before CID construction.

`emitted_artifact_cid` is the CID of the artifact emitted for this wrapper.

`mode` identifies the wrapper behavior. The only core modes are `monitor`, `witness`, and `dispatcher`. Extension modes MUST be namespace-qualified strings in `<namespace>:<mode>` form. Verifiers MUST refuse extension modes when they do not implement that namespace and its semantics.

`object_fcm_cid` is the CID of the object `FunctionContractMemento`. It identifies the wrapped object function. The object FCM is unchanged by wrapping.

`observer_effects` is the non-empty list of `EffectOccurrence` records emitted by the wrapper side. These effects live on the wrapper, not on the object.

`preservation_claim_cid` is the CID of the preservation claim that states the semantic relationship preserved by the wrapper. It MUST be present.

`provenance_cid` is the CID of the provenance record for this wrapper memento.

`wrapper_fcm_cid` is the CID of the wrapper `FunctionContractMemento`. It is a separate substrate object and carries the observer effects. The resolved `wrapper_fcm_cid.effects` MUST include every entry of `observer_effects` (see §7); the wrapper FCM is the durable home of those occurrences.

## §3 Mode-Specific Behavior

### §3.1 monitor

A `monitor` wrapper observes invariants at call boundaries. Its `observer_effects` typically include `Reads` and `Io` effect occurrences used to inspect inputs, outputs, or boundary state.

The `preservation_claim_cid` MUST resolve to a claim asserting that the wrapper's preconditions and postconditions are equal to the object's preconditions and postconditions for the observed call boundary.

### §3.2 witness

A `witness` wrapper samples behavior and emits structured observation events. Its `observer_effects` typically include `Io` effect occurrences that publish or persist the witness sample.

The `preservation_claim_cid` MUST resolve to a claim asserting that the witness sample is causally downstream of the object call that it records.

### §3.3 dispatcher

A `dispatcher` wrapper interposes routing or dispatch. Its `observer_effects` MAY include `UnresolvedCall` effect occurrences on the routing target when the selected target is not statically resolved at memento construction time.

The `preservation_claim_cid` MUST resolve to a claim asserting that the dispatched function is a member of the declared dispatch set and that selection follows the declared selection semantics.

## §4 Object vs Wrapper Separation

`object_fcm_cid.effects` MUST NOT contain any of the wrapper's observer effects.

`wrapper_fcm_cid.effects` is the effect surface that carries the wrapper's observer effects. Verifiers MUST refuse to admit any composition that places wrapper-side observer effects on the object FCM or otherwise conflates object effects with wrapper effects.

Wrapping an object function MUST NOT change the CID-addressed object FCM. If observing the object requires new effects, those effects MUST be represented by a separate wrapper FCM and by this `ObservationWrapperMemento`.

## §5 Preservation-Claim Shape

The preservation claim memento shape is out of scope for this specification and is reserved for a separate spec slot.

For this specification, `preservation_claim_cid` is opaque, but it is mandatory. A verifier MAY inspect the pointed-to memento if it implements the preservation-claim spec. A verifier that cannot validate a required claim for an admission decision MUST fail closed.

## §6 RealizationPlanMemento Interaction

When a `RealizationPlanMemento` (TSavo/provekit#801) selects mode-specific observation emission, the plan's `observation_wrapper_cid` MUST be present and MUST resolve to an `ObservationWrapperMemento` under this spec.

The edge is one-directional: the plan points at the wrapper. An `ObservationWrapperMemento` does NOT carry the back-reference inside its content-addressed body, because including the plan CID inside the wrapper body and the wrapper CID inside the plan body would require a cryptographic-hash fixed point and is not operationally constructible.

If a verifier needs to walk from a wrapper back to the plan that selected it, the verifier MUST consult a `RunMemento` (TSavo/provekit#799), a `StageReceipt`, or other non-CID-body provenance that records which plan minted which wrapper. That provenance lives outside the wrapper's CID input and so does not feed the cycle.

## §7 Fail-Closed Rules

Verifiers MUST refuse admission when any of the following conditions hold:

1. `mode` is unknown, is not namespace-qualified when outside the core set, or is an unimplemented namespaced extension.
2. `preservation_claim_cid` is missing.
3. `object_fcm_cid.effects` intersects `observer_effects` (the master-frame invariant is violated: observer effects MUST NOT appear on the object FCM).
4. The resolved `wrapper_fcm_cid.effects` does NOT contain every entry of `observer_effects` (the wrapper FCM is the durable home of these occurrences; missing entries mean the wrapper is not actually carrying what this memento claims).
5. Any mandatory field is missing.
6. Any CID field does not resolve in the verifier's admission context.

## §8 CID Construction

`ObservationWrapperMemento` CIDs are computed over the JCS-canonical JSON representation of the memento using BLAKE3-512:

```text
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS(observation-wrapper-memento)))
```

The canonical representation MUST use alphabetical key order, no insignificant whitespace, and the exact field names in this specification. Arrays preserve their protocol order under JCS. The digest is the full 64-byte BLAKE3-512 output encoded as 128 lowercase hexadecimal characters with the `blake3-512:` prefix.

The wrapper CID is constructible without reference to any `RealizationPlanMemento` CID. A plan that wants to select this wrapper cites its CID; this wrapper does not need to know which plan selected it. That asymmetry avoids the cryptographic-hash fixed point that would arise if both bodies tried to embed the other's CID.

## §9 Cross-References

This specification depends on and composes with:

1. Issue #793, `EffectOccurrence`, for the structured semantic payload used in `observer_effects`.
2. Issue #801, `ParametricRealizationMemento` and `RealizationPlanMemento`, for the one-way `observation_wrapper_cid` plan-to-wrapper reference.
3. Issue #791, `PromotionDecisionMemento`, for promotion decisions that may depend on admissible observation wrappers.
4. Issue #796, the admissibility-spine master frame, for the invariant that observer effects belong to wrappers.
5. `realizer-protocol-v2`, for realization and admission context.

## §10 Out of Scope

This specification does not define:

1. The preservation-claim memento wire shape.
2. Per-mode runtime emission detail.
3. Cross-language wrapper composition.
