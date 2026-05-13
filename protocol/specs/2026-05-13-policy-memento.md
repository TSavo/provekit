# PolicyMemento Family

Status: Draft

Date: 2026-05-13

## 0. Purpose

`PolicyMemento` records the content-addressed admission policy input that a
`PromotionDecisionMemento` references through `policy_cid` and validates against
through `decision_payload`.

This specification defines the wire shape of policy mementos. It does not define
which thresholds, signers, reviewers, proof systems, or evidence values are
correct. Those values live in concrete policy instances. In the admissibility
spine:

1. Specs define shape.
2. Policies define thresholds.
3. Receipts record decisions.
4. Verifiers replay receipts.
5. Catalogs retain admitted semantics.

Therefore this specification preserves policy decisions as content-addressed
inputs. It does not answer policy.

## 1. Wire Shape

The CDDL below defines the common envelope and the canonical admission-gate
profiles. Object keys are listed in alphabetical order and producers MUST emit
JCS-canonical JSON with alphabetical key order before CID construction.

```cddl
policy-memento = threshold-policy-memento
               / property-policy-memento
               / signature-policy-memento
               / human-acceptance-policy-memento
               / proof-gate-policy-memento

cid = tstr
json-value = false / true / null / int / float / tstr / json-array / json-object
json-array = [* json-value]
json-object = {* tstr => json-value}
namespaced-kind = tstr .regexp "[A-Za-z][A-Za-z0-9._-]*:[A-Za-z][A-Za-z0-9._-]*"
policy-kind = "human_acceptance"
            / "proof_gate"
            / "property"
            / "signature"
            / "threshold"
            / namespaced-kind
rule = tstr / json-object

policy-common-fields = (
  admission_rule: rule,
  decision_payload_schema: json-value,
  input_requirements: json-value,
  policy_kind: policy-kind,
  policy_version: tstr,
  provenance_cid: cid,
  refusal_rule: rule,
)

threshold-policy-memento = {
  admission_rule: rule,
  count_field_path: [1* tstr],
  decision_payload_schema: json-value,
  input_requirements: json-value,
  policy_kind: "threshold",
  policy_version: tstr,
  provenance_cid: cid,
  refusal_rule: rule,
  score_field_path: [* tstr],
  threshold_comparator: "eq" / "gte" / "gt" / "lte" / "lt" / namespaced-kind,
  threshold_value: int / float,
  ? weight_field_path: [1* tstr],
}

property-policy-memento = {
  admission_rule: rule,
  decision_payload_schema: json-value,
  generator_cid: cid,
  input_requirements: json-value,
  policy_kind: "property",
  policy_version: tstr,
  property_cid: cid,
  provenance_cid: cid,
  refusal_rule: rule,
  result_field_path: [1* tstr],
  ? shrinker_cid: cid,
  success_criteria: rule,
}

signature-policy-memento = {
  admission_rule: rule,
  allowed_signature_suites: [+ tstr],
  decision_payload_schema: json-value,
  ? delegation_policy_cid: cid,
  input_requirements: json-value,
  policy_kind: "signature",
  policy_version: tstr,
  provenance_cid: cid,
  quorum_size: uint,
  refusal_rule: rule,
  required_signers_cids: [+ cid],
  signature_payload_schema: json-value,
}

human-acceptance-policy-memento = {
  acceptance_record_schema: json-value,
  admission_rule: rule,
  ? conflict_policy_cid: cid,
  decision_payload_schema: json-value,
  delegation_policy_cid: cid,
  input_requirements: json-value,
  policy_kind: "human_acceptance",
  policy_version: tstr,
  provenance_cid: cid,
  refusal_rule: rule,
  required_acceptances: uint,
  reviewer_roster_cid: cid,
}

proof-gate-policy-memento = {
  admission_rule: rule,
  checker_cid: cid,
  decision_payload_schema: json-value,
  input_requirements: json-value,
  policy_kind: "proof_gate",
  policy_version: tstr,
  proof_artifact_schema: json-value,
  proof_system: tstr / namespaced-kind,
  provenance_cid: cid,
  refusal_rule: rule,
  theorem_ref: tstr,
  trusted_base_cid: cid,
}
```

Canonical `policy_kind` values are:

* `threshold`
* `property`
* `signature`
* `human_acceptance`
* `proof_gate`

Other policy kinds MUST be namespaced as `<namespace>:<kind>`. Bare extension
strings are invalid.

## 2. Field Semantics

`admission_rule` identifies the rule that admits the decision payload. It is
opaque to this specification. A producer MAY use a structured object for a
namespaced rule vocabulary, but verifiers MUST treat unrecognized rule
vocabularies as non-admitting unless an implementation explicitly supports them.

`decision_payload_schema` is a JSON value that pins the expected shape of the
`PromotionDecisionMemento.decision_payload` validated by this policy. It MAY be
a JSON Schema document, a content-addressed schema reference encoded as JSON, or
another namespaced schema description.

`input_requirements` describes the evidence shape the policy expects before a
decision can be replayed. It is descriptive shape data, not a statement that the
evidence is sufficient.

`policy_kind` is the discriminator. The five canonical values are listed in
section 1. A conforming `PolicyMemento` uses one of those values. Unknown values
fail closed unless they are namespaced and the verifier explicitly implements
that namespace as an extension profile.

`policy_version` identifies the policy profile version. It is a string so policy
families can use semver, dates, or namespaced version schemes.

`provenance_cid` links to the provenance record for the policy instance,
including authorship, review, generation context, or catalog lineage as
applicable.

`refusal_rule` is parallel to `admission_rule` and identifies when the same
policy refuses admission. It is also opaque to this specification.

All kind-specific fields are part of the content-addressed policy input. Changing
any such field changes the policy CID.

## 3. Gate-Specific Kinds

### 3.1 ThresholdPolicyMemento

`ThresholdPolicyMemento` models count or score based admission. It records which
field supplies the observed count or score, which comparator applies, and which
threshold value the policy instance uses.

Example:

```json
{
  "admission_rule": "provekit.threshold:v1",
  "count_field_path": ["trial_summary", "passed"],
  "decision_payload_schema": {
    "required": ["trial_summary"]
  },
  "input_requirements": {
    "required": ["trial_summary"]
  },
  "policy_kind": "threshold",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "provekit.threshold_refusal:v1",
  "score_field_path": [],
  "threshold_comparator": "gte",
  "threshold_value": 100
}
```

### 3.2 PropertyPolicyMemento

`PropertyPolicyMemento` models property-test admission. It pins the property,
generator, result path, and success criteria while leaving the actual acceptance
criteria to the policy instance.

Example:

```json
{
  "admission_rule": "provekit.property:v1",
  "decision_payload_schema": {
    "required": ["property_result"]
  },
  "generator_cid": "b3.example.generator",
  "input_requirements": {
    "required": ["property_result"]
  },
  "policy_kind": "property",
  "policy_version": "2026-05-13",
  "property_cid": "b3.example.property",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "provekit.property_refusal:v1",
  "result_field_path": ["property_result"],
  "success_criteria": "provekit.property_success:v1"
}
```

### 3.3 SignaturePolicyMemento

`SignaturePolicyMemento` models multi-party signature admission. It records the
eligible signer set, accepted signature suites, quorum size, and payload shape.

Example:

```json
{
  "admission_rule": "provekit.signature:v1",
  "allowed_signature_suites": ["ed25519-jcs-blake3-512"],
  "decision_payload_schema": {
    "required": ["signatures"]
  },
  "input_requirements": {
    "required": ["signatures"]
  },
  "policy_kind": "signature",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "quorum_size": 2,
  "refusal_rule": "provekit.signature_refusal:v1",
  "required_signers_cids": [
    "b3.example.signer.alice",
    "b3.example.signer.bob",
    "b3.example.signer.carol"
  ],
  "signature_payload_schema": {
    "required": ["payload_cid", "signature", "signer_cid"]
  }
}

```

### 3.4 HumanAcceptancePolicyMemento

`HumanAcceptancePolicyMemento` models human-reviewer admission with delegation.
It records the reviewer roster, delegation policy, required acceptances, and the
shape of the acceptance record.

Example:

```json
{
  "acceptance_record_schema": {
    "required": ["accepted_at", "reviewer_cid"]
  },
  "admission_rule": "provekit.human_acceptance:v1",
  "decision_payload_schema": {
    "required": ["acceptances"]
  },
  "delegation_policy_cid": "b3.example.delegation",
  "input_requirements": {
    "required": ["acceptances"]
  },
  "policy_kind": "human_acceptance",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "provekit.human_acceptance_refusal:v1",
  "required_acceptances": 2,
  "reviewer_roster_cid": "b3.example.reviewers"
}
```

### 3.5 ProofGatePolicyMemento

`ProofGatePolicyMemento` models mechanical-discharge admission. It pins the
proof system, checker, theorem reference, proof artifact shape, and trusted base.

Example:

```json
{
  "admission_rule": "provekit.proof_gate:v1",
  "checker_cid": "b3.example.checker",
  "decision_payload_schema": {
    "required": ["proof_artifact"]
  },
  "input_requirements": {
    "required": ["proof_artifact"]
  },
  "policy_kind": "proof_gate",
  "policy_version": "2026-05-13",
  "proof_artifact_schema": {
    "required": ["artifact_cid", "checker_output"]
  },
  "proof_system": "lean4",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "provekit.proof_gate_refusal:v1",
  "theorem_ref": "Example.Theorem",
  "trusted_base_cid": "b3.example.trusted-base"
}
```

## 4. PromotionDecisionMemento Interaction

A `PromotionDecisionMemento` identifies a gate, references a policy with
`policy_cid`, and carries `decision_payload`.

Verification resolves the interaction as follows:

1. Load the policy memento addressed by `policy_cid`.
2. Confirm the object is a valid `PolicyMemento`.
3. Confirm the `gate` in the promotion decision is compatible with
   `policy_kind`.
4. Validate `decision_payload` against `decision_payload_schema`.
5. Replay the policy instance's `admission_rule` or `refusal_rule` over the
   payload and required inputs using only explicitly supported rule semantics.
6. Treat the replayed outcome as the recorded admission decision for that gate.

The policy memento is an input to the promotion decision. It is not replaced by
the verifier's local preferences.

Compatibility between `PromotionDecisionMemento.gate` and `policy_kind` is:

| `gate` | compatible `policy_kind` |
|---|---|
| `human` | `human_acceptance`, `signature` |
| `proof` | `proof_gate` |
| `property` | `property` |
| `threshold` | `threshold` |

Namespaced gates and namespaced policy kinds are compatible only when the
verifier explicitly implements that namespace and mapping.

## 5. Fail-Closed Behavior

Verifiers MUST refuse admission when:

* `policy_kind` is unknown.
* `policy_kind` is an unimplemented namespaced extension.
* `policy_kind` is a bare noncanonical extension string.
* `policy_cid` is missing.
* `policy_cid` cannot be resolved.
* the resolved object is not a valid `PolicyMemento`.
* `decision_payload` is missing.
* `decision_payload` does not match `decision_payload_schema`.
* required evidence described by `input_requirements` is missing.
* `admission_rule` or `refusal_rule` uses unsupported semantics.

## 6. CID Construction

The policy CID is constructed over the complete policy memento object:

1. Serialize as JCS-canonical JSON.
2. Emit object keys in alphabetical order.
3. Hash the canonical bytes with BLAKE3-512.
4. Encode the digest with the repository CID encoding profile.

Any change to an envelope field, gate-specific field, schema, rule, or provenance
CID produces a different policy CID.

## 7. Cross-References

This specification is part of the admissibility-spine work in issue #796.

It is referenced by `PromotionDecisionMemento` issue #791 through `policy_cid`
and `decision_payload`. That earlier spec may use the transitional term
`PromotionPolicyMemento`; this specification defines that referenced policy
family as `PolicyMemento`.

It is adjacent to the gate semantics tracked in issue #792.

It is intended to compose with `compound-contract-memento` where a compound
contract needs to preserve admitted semantics through explicit policy inputs.

## 8. Out of Scope

This specification does not define a rule language. `admission_rule` and
`refusal_rule` are opaque to this specification. Future specifications MAY define
rule sublanguages or namespaced rule vocabularies.

This specification does not define which thresholds, properties, signers,
reviewers, proof systems, or proof obligations are correct for a project.
