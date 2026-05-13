# PipelineMemento and RunMemento

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Author:** T Savo
**Related:**
- TSavo/provekit#792 (`ProofRunMemento`, verifier profile)
- TSavo/provekit#795 (pipeline receipts and memento replay)
- TSavo/provekit#796 (admissibility spine)
- `2026-04-30-proof-file-format.md`
- `2026-05-12-plugin-protocol.md`
- `2026-05-13-proof-run-memento.md`

## §0. Purpose

ProveKit has bind, link, transport, compose, promotion, realization, and verifier pipelines. Each pipeline needs a replay receipt that says which pipeline vocabulary was used, which inputs were consumed, which stage receipts were produced, which outputs were emitted, and which predecessor runs form the replay graph.

This spec defines the generic substrate shape for that receipt pair:

1. `PipelineMemento` pins a pipeline kind, version, ordered stage vocabulary, accepted input kinds, emitted output kinds, failure kinds, and provenance.
2. `RunMemento` records one execution of one `PipelineMemento`: inputs, ordered stage receipt CIDs, outputs, predecessor runs, sealed plugin registry, verdict, and provenance.

This spec defines the shape of a run. It does not define the stage vocabulary of any particular pipeline. Pipeline vocabularies live in `PipelineMemento` instances. `ProofRunMemento` from TSavo/provekit#792 is the verifier-pipeline profile of this generic shape: it specializes the same fields for `provekit prove`, and its `verifier_pipeline_cid` is a `PipelineMemento` CID under this spec.

## §1. Wire shapes

The CDDL display order below follows locked JCS alphabetical key order within each object. JCS canonicalization is normative for objects before hashing, signing, or CID comparison.

```cddl
; Shared scalar types.
cid = tstr                 ; "blake3-512:" plus 128 lowercase hex digits
failure-kind = tstr        ; profile-defined refusal discriminator
kind-discriminator = tstr  ; profile-defined artifact or memento kind
stage-name = tstr          ; label pinned by a PipelineMemento

; Namespaced extensions use the textual grammar "<namespace>:<kind>".
; CDDL validates the scalar shape; §3 validates the namespace rule.
pipeline-kind =
  "verifier" /
  "bind" /
  "link" /
  "transport" /
  "compose" /
  "promotion" /
  "realization" /
  namespaced-pipeline-kind

namespaced-pipeline-kind = tstr

run-verdict = "succeeded" / "failed" / "refused"

; Locked JCS key order:
; accepted_input_kinds, emitted_output_kinds, failure_kinds,
; pipeline_kind, pipeline_version, provenance_cid, stage_vocabulary
PipelineMemento = {
  accepted_input_kinds: [+ kind-discriminator],
  emitted_output_kinds: [+ kind-discriminator],
  failure_kinds: [+ failure-kind],
  pipeline_kind: pipeline-kind,
  pipeline_version: tstr,
  provenance_cid: cid,
  stage_vocabulary: [+ stage-name],
}

; Locked JCS key order:
; input_cids, output_cids, pipeline_cid, plugin_registry_cid,
; predecessor_run_cids, provenance_cid, stage_receipt_cids, verdict
RunMemento = {
  input_cids: [+ cid],
  output_cids: [* cid],
  pipeline_cid: cid,
  plugin_registry_cid: cid,
  predecessor_run_cids: [* cid],
  provenance_cid: cid,
  stage_receipt_cids: [+ cid],
  verdict: run-verdict,
}
```

## §2. Field semantics

### §2.1 `PipelineMemento`

| Field | Required | Meaning |
|---|---:|---|
| `accepted_input_kinds` | yes | Non-empty list of input artifact or memento kind discriminators the pipeline accepts at its root. A run whose `input_cids` resolve to any other kind MUST be refused. |
| `emitted_output_kinds` | yes | Non-empty list of output artifact or memento kind discriminators the pipeline may emit. A run whose `output_cids` resolve to any other kind MUST be refused. |
| `failure_kinds` | yes | Non-empty list of failure or refusal discriminators the pipeline may emit. Unknown failure kinds are not silently accepted. |
| `pipeline_kind` | yes | Canonical kind from §3 or a namespaced extension kind of the form `<namespace>:<kind>`. |
| `pipeline_version` | yes | Pipeline version string. Producers SHOULD use semver where practical, but consumers identify the pipeline by CID, not by this string. |
| `provenance_cid` | yes | CID of the provenance memento that records who minted this pipeline vocabulary and from which source or governance action. |
| `stage_vocabulary` | yes | Non-empty ordered stage-name list. Its order is semantic and fixes the order expected in `RunMemento.stage_receipt_cids`. |

### §2.2 `RunMemento`

| Field | Required | Meaning |
|---|---:|---|
| `input_cids` | yes | Non-empty ordered list of root input CIDs consumed by this run. The consuming pipeline defines whether order is semantic. |
| `output_cids` | yes | Ordered list of durable output CIDs emitted by this run. Empty is allowed for refused or failed runs that produced no durable output. |
| `pipeline_cid` | yes | CID of the `PipelineMemento` that defines the pipeline kind, version, accepted inputs, emitted outputs, failure kinds, and ordered stage vocabulary for this run. |
| `plugin_registry_cid` | yes | CID of the sealed `PluginRegistryMemento` from `2026-05-12-plugin-protocol.md` used by this run. Replay MUST use this exact registry CID. |
| `predecessor_run_cids` | yes | Ordered list of prior `RunMemento` CIDs whose outputs were consumed. Empty array means no run predecessor. This is explicit lineage, not a reverse lookup. |
| `provenance_cid` | yes | CID of the provenance memento that records who executed and sealed this run. |
| `stage_receipt_cids` | yes | Non-empty ordered list of stage receipt CIDs. The length and order MUST match `pipeline_cid.stage_vocabulary`. |
| `verdict` | yes | Terminal run verdict. `succeeded` means every required stage replayed and outputs matched. `failed` means the pipeline completed with a profile-defined non-success result. `refused` means validation failed closed. |

## §3. Pipeline-kind catalog

Canonical pipeline kinds are reserved as follows.

| `pipeline_kind` | Description |
|---|---|
| `verifier` | Consumes proof artifacts, link context, and sealed plugins to decide whether substrate claims are admissible. |
| `bind` | Consumes source or source-derived facts and binds them into substrate claim or IR artifacts. |
| `link` | Consumes bound artifacts and resolves cross-artifact references, bridge targets, and link closure facts. |
| `transport` | Consumes source or IR artifacts and transports them across language, syntax, or representation boundaries while recording exact or lossy movement. |
| `compose` | Consumes admissible claims or contracts and emits composed claims, compound contracts, or composition refusal mementos. |
| `promotion` | Consumes candidate claims or decisions and emits promotion decisions into a stronger substrate status. |
| `realization` | Consumes contracts, obligations, or claims and emits realization artifacts or realization refusal mementos. |
| `<namespace>:<kind>` | Extension pipeline kind. The namespace MUST be non-empty, the kind MUST be non-empty, and exactly one colon separates them. |

Unknown `pipeline_kind` values MUST fail closed unless the value is a valid namespaced extension and the sealed plugin registry contains an implementation that declares support for that exact extension kind.

## §4. Stage vocabulary semantics

`PipelineMemento.stage_vocabulary` pins ordered stage names. `RunMemento.stage_receipt_cids` is valid only when it has the same length and order as that pinned vocabulary after each receipt is resolved to its declared stage name.

The `PipelineMemento` also pins the pipeline-level expected IO shape through `accepted_input_kinds`, `emitted_output_kinds`, and `failure_kinds`. A profile MAY define stricter per-stage IO rules for each `stage-name`; replay validators MUST apply those profile rules when present. If a profile does not define a per-stage receipt body, the generic validator still checks that the ordered receipt list exists, that each receipt CID resolves, and that final input and output kinds match the pipeline memento.

Stage names are labels, not executable authority. The executable authority comes from replaying the declared pipeline kind and version through the exact sealed plugin registry. An implementation MUST NOT infer a stage vocabulary from local source code when a `PipelineMemento` is present; the memento is the pinned vocabulary.

## §5. Lineage via `predecessor_run_cids`

`predecessor_run_cids` defines the replay graph explicitly. A consumer that wants to replay a run walks from the current `RunMemento` to each predecessor CID named by the run, then recursively replays those predecessors as needed.

Replay MUST NOT depend on reverse lookup such as "find all runs that produced this input." Reverse lookup can be useful for indexing, but it is not part of the trust chain. A run that consumes outputs of another run without naming that predecessor is incomplete and MUST be refused by validators that require replayable lineage.

Cycles in the predecessor graph MUST be refused. Duplicate predecessor CIDs SHOULD be canonicalized away by producers and MUST NOT change replay semantics for consumers.

## §6. Fail-closed rules

Validators MUST refuse a `RunMemento` when any of the following holds:

1. `pipeline_cid` does not resolve to a valid `PipelineMemento`.
2. `pipeline_kind` is unknown and is not a valid supported namespaced extension.
3. `stage_receipt_cids` length differs from `pipeline_cid.stage_vocabulary` length.
4. A resolved stage receipt declares a stage name that differs from the corresponding `stage_vocabulary` entry.
5. `plugin_registry_cid` differs from the sealed registry used for replay.
6. Any `input_cids` kind is absent from `accepted_input_kinds`.
7. Any `output_cids` kind is absent from `emitted_output_kinds`.
8. A replayed output CID differs from the corresponding `output_cids` entry or set, according to the profile's output ordering rule.
9. A failure or refusal kind is absent from `failure_kinds`.
10. Any named predecessor run is missing, invalid, cyclic, or inconsistent with the current run's consumed inputs.

The default behavior is refusal. A validator MUST NOT silently drop an unknown stage, unknown failure kind, plugin registry mismatch, or output recompute mismatch.

## §7. ProofRunMemento becomes the verifier profile

`ProofRunMemento` from TSavo/provekit#792 is the verifier-pipeline profile of this generic shape. Its fields map cleanly to `RunMemento`:

| `ProofRunMemento` field | Generic `RunMemento` field |
|---|---|
| `verifier_pipeline_cid` | `pipeline_cid` |
| `proof_envelope_cid`, `link_bundle_cid`, `input_artifact_cids` | `input_cids` |
| `stage_receipt_cids` | `stage_receipt_cids` |
| `output_artifact_cids` | `output_cids` |
| `input_run_cids` | `predecessor_run_cids` |
| `plugin_registry_cid` | `plugin_registry_cid` |
| `verdict` | `verdict` with verifier-profile verdict mapping |
| `envelope`, `metadata`, and sealing provenance | `provenance_cid` plus profile envelope fields |

The `verifier_pipeline_cid` named by #792 is a `PipelineMemento` under this spec with `pipeline_kind = "verifier"`. The verifier profile may keep its profile-specific envelope, metadata, timestamps, signatures, and `StageReceipt` body. Those profile fields are outside the generic `RunMemento` core but do not change the core replay relation.

## §8. CID construction

`PipelineMemento` and `RunMemento` CIDs are computed over their JCS-canonical object bytes:

```text
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS(object)))
```

Object keys MUST be alphabetically ordered before hashing. Arrays remain order-preserving under JCS. Therefore `stage_vocabulary`, `stage_receipt_cids`, `input_cids`, `output_cids`, and `predecessor_run_cids` retain their declared order unless a profile explicitly defines one of those arrays as a set with canonical sorting.

All CIDs in this spec use the full BLAKE3-512 digest with the `blake3-512:` prefix and 128 lowercase hexadecimal digits. Validators MUST recompute referenced object CIDs before trusting their fields.

## §9. Cross-references

- TSavo/provekit#792 defines `ProofRunMemento` and verifier `StageReceipt`, the first profile of this generic shape.
- TSavo/provekit#795 tracks replay receipt work that feeds this generic pipeline shape.
- TSavo/provekit#796 is the admissibility-spine umbrella: bind, link, transport, compose, promotion, realization, and verifier pipelines all need replay receipts.
- `2026-04-30-proof-file-format.md` defines the `.proof` bundle trust root consumed by verifier runs.
- `2026-05-12-plugin-protocol.md` defines `PluginRegistryMemento`, which a run cites through `plugin_registry_cid`.

## §10. Out of scope

Stage-receipt body details are out of scope for this generic spec. `StageReceipt` in TSavo/provekit#792 is the verifier-profile receipt. Other pipelines define their own profile receipts, refusal bodies, diagnostics, and per-stage IO contracts while preserving the generic `PipelineMemento` and `RunMemento` replay graph shape.

This spec also does not define pipeline scheduling, plugin discovery, UI report formatting, or storage indexes for reverse lookup.
