# CompositionRefusalMemento

**Status:** Normative draft for CCP refusal artifacts.
**Scope:** Contract Composition Protocol refusals that need replayable, content-addressed evidence.

## §0 Purpose

`CompositionRefusalMemento` is the canonical refusal artifact emitted when CCP cannot compose a requested chain of contract atoms and effect sets. CCP already returns handle-level errors through:

```rust
compose_chain_contracts(atoms, effect_sets) -> Result<ComposedFunctionContract, CompositionError>
```

That `Err(CompositionError)` is not sufficient for downstream replay. A downstream consumer refusing to compose through a failed composition needs a CID it can cite. This memento closes that gap by giving every normative CCP refusal a stable, signed, content-addressed record.

A conforming CCP producer MUST emit a `CompositionRefusalMemento` whenever it returns a composition failure that may be observed outside the local call frame. Local developer diagnostics MAY still carry richer process-local data, but any protocol-facing refusal MUST be represented by this memento shape.

### §0.1 Deterministic refusal identity

Per the admissibility-spine framing (#796): refusals must be REPLAYABLE. Two invocations of `compose_chain_contracts` on the same canonical inputs against the same CCP version MUST produce the same refusal CID. The header therefore excludes wall-clock timestamps and signer identity; those live in `envelope` and `metadata` so re-attestation by a different signer or a later retry does not change the refusal identity.

## §1 Wire shape (CDDL)

```cddl
composition-refusal-memento = {
  envelope: composition-refusal-envelope,
  header: composition-refusal-header,
  metadata: composition-refusal-metadata,
}

composition-refusal-envelope = {
  declaredAt: iso8601,
  signature: signature,
  signer: signer,
}

composition-refusal-header = {
  atoms_cids: [+ cid],
  ? blocking_effects: [+ blocking-effect],
  ccp_version: tstr,
  cid: cid,
  compose_input_cid: cid,
  ? effect_occurrences: [* effect-occurrence],
  effect_set_cids: [+ cid],
  failure_detail: tstr,
  failure_kind: composition-failure,
  ? incompatible_pair: incompatible-pair,
  kind: "composition-refusal",
  ? missing_memento_requirements: [+ missing-requirement],
  schemaVersion: "1",
}

blocking-effect = {
  atom_cid: cid,
  classification: "block" / "memento-required" / "informational-dischargeable",
  discharge_key: tstr,
  occurrence_kind: tstr,
}

effect-occurrence = {
  args: json-value,
  discharge_key: tstr,
  locator: json-value,
  occurrence_kind: tstr,
  role: "pre" / "post" / "inv" / "declared",
  signature_cid: cid,
}

incompatible-pair = {
  atom_a_cid: cid,
  atom_b_cid: cid,
  effect_a: tstr,
  effect_b: tstr,
}

missing-requirement = {
  ? expected_cid: cid,
  ? memento_kind: tstr,
  reason: tstr,
  ? role: tstr,
}

composition-refusal-metadata = {
  ? note: tstr,
  ? provenance_cid: cid,
  ? refused_at: iso8601,
  ? source_url: tstr,
}

composition-failure = "determinism-violation"
                    / "discharge-budget-exceeded"
                    / "effect-tracking-gap"
                    / "impure-input"
                    / "incompatible-effects"
                    / "memento-required-missing"
                    / "ordering-conflict"
                    / "schema-version-mismatch"
                    / "unsatisfiable-precondition"
                    / tstr

cid = tstr
iso8601 = tstr
signature = tstr
signer = tstr
```

The CDDL member order above is the locked human-readable form of the JCS key order: alphabetical within each object. Optional keys are omitted when absent and appear in sorted position when present.

## §2 Field semantics

| Field | Required | Semantics |
| --- | --- | --- |
| `envelope.declaredAt` | yes | ISO-8601 UTC timestamp for when the signed envelope was minted. NOT part of `header.cid`. |
| `envelope.signature` | yes | Signature over `JCS({header, metadata})`, excluding `envelope`. The signature algorithm is the same Ed25519 envelope signing convention used by contemporary mementos. |
| `envelope.signer` | yes | Signer identity or public-key reference that verifies `envelope.signature`. NOT part of `header.cid`; re-attestation by a different signer preserves refusal identity. |
| `header.atoms_cids` | yes | Ordered non-empty list of atom CIDs passed to CCP for this composition attempt. The order is the attempted chain order. |
| `header.blocking_effects` | no | Structured list of effect occurrences whose classification (per the discharge-classification spec) is responsible for the refusal. Present whenever `failure_kind` is `impure-input`, `incompatible-effects`, or any other failure where one or more effects classify as `block` or `memento-required`. Each entry names the atom that owns the effect, the occurrence kind, the discharge key, and the classification verdict. |
| `header.ccp_version` | yes | Version of the Contract Composition Protocol used by the producer (e.g., `"1.0.0"`). A failure under one CCP version is not assumed valid under another; consumers SHOULD cite the version when replaying. |
| `header.cid` | yes | CID of this refusal header, computed by §4 with `header.cid` elided. |
| `header.compose_input_cid` | yes | CID of the canonical composition request bytes (JCS over `{atoms_cids, effect_set_cids, ccp_version}` at minimum; producers MAY include additional inputs they intend to pin). This is THE input identity of the failed composition; two refusals with the same `compose_input_cid` against the same `ccp_version` MUST agree on `failure_kind` and `failure_detail`. |
| `header.effect_occurrences` | no | List of `EffectOccurrence` values (per `2026-05-13-effect-occurrence-memento.md`) observed during composition. Empty when CCP refused before occurrences were enumerated (e.g., schema mismatch). Required whenever `failure_kind` is `impure-input`, `incompatible-effects`, `effect-tracking-gap`, or `memento-required-missing`. |
| `header.effect_set_cids` | yes | Ordered non-empty list of effect-set CIDs consulted for the same composition attempt. Producers MUST preserve the order used by the failed composition. |
| `header.failure_detail` | yes | Human-readable, deterministic diagnostic string. It MUST be stable for the same failure inputs and MUST NOT include process-local paths, random identifiers, or wall-clock text. |
| `header.failure_kind` | yes | Machine-readable failure category from §3. |
| `header.incompatible_pair` | no | Present when `failure_kind` is `incompatible-effects` and the producer can identify a decisive conflicting pair. MAY be absent for aggregate or solver-derived incompatibilities. |
| `header.kind` | yes | Literal discriminator. MUST be `"composition-refusal"`. |
| `header.missing_memento_requirements` | no | Required when `failure_kind` is `memento-required-missing`. Structured list of the memento requirements that could not be satisfied: each entry names the role (e.g., `"loop-invariant"`, `"call-target-proof"`, `"return-shape-proof"`), the expected memento kind, the expected CID when known, and a reason. |
| `header.schemaVersion` | yes | Literal schema version. MUST be `"1"`. |
| `metadata.note` | no | Operator or producer note. It participates in the signature but not in `header.cid`. |
| `metadata.provenance_cid` | no | CID of a `ProvenanceMemento`, `ProofRunMemento`, or other run-level memento that locates this refusal within a broader pipeline run. Lives in `metadata` (NOT in `header.cid` input) so the same failed composition over identical `compose_input_cid` and `ccp_version` mints the same refusal CID regardless of which enclosing run observed it. Optional because not all CCP invocations occur inside an enclosing run. |
| `metadata.refused_at` | no | ISO-8601 UTC timestamp for the refusal decision. NOT part of `header.cid` so deterministic refusals over the same inputs share the same CID across retries. MAY equal `envelope.declaredAt`. |
| `metadata.source_url` | no | Optional URL for a source issue, build log, or composition request record. It participates in the signature but not in `header.cid`. |

The `atoms_cids`, `effect_set_cids`, and `compose_input_cid` triple are part of the refusal identity. A producer that retries the same logical composition with reordered inputs is producing a different refusal unless CCP specifies the same normalized order before calling the composition engine.

## §3 Failure-kind taxonomy

`failure_kind` is an open string taxonomy. The following values are reserved and normative. Each row names the canonical CCP error variant it surfaces and the header fields it requires.

| Failure kind | CCP error variant | Meaning | Required additional header fields |
| --- | --- | --- | --- |
| `determinism-violation` | `CompositionError::DeterminismViolation` | Composition succeeded once but produced different results on a second run with identical canonical inputs; CCP MUST refuse rather than ship a nondeterministic composed contract. | None required beyond the base set. `failure_detail` SHOULD identify the divergence (which sub-result differed). |
| `discharge-budget-exceeded` | n/a (solver budget) | CCP found obligations that might be satisfiable, but the configured discharge budget was exhausted before proof search completed. | None required. Include the budget class or limit name in `failure_detail`, not volatile timing data. |
| `effect-tracking-gap` | n/a (lifter soundness) | One or more input atoms carry a lifter-soundness memento declaring incomplete effect tracking, and CCP cannot soundly compose without complete tracking. | `effect_occurrences` SHOULD record what tracking the producer DID observe. `missing_memento_requirements` MAY name the soundness memento expected. |
| `impure-input` | `CompositionError::ImpureInput` | One or more input atoms have non-empty effect sets; composition is sound only over pure subtrees. | `effect_occurrences` and `blocking_effects` MUST be populated. `failure_detail` names the impure atom by CID. |
| `incompatible-effects` | `CompositionError::IncompatibleEffects` | Two or more effects cannot be legally ordered, merged, or discharged under the active effect semantics. | `effect_occurrences` MUST be populated. `blocking_effects` MUST list the offending occurrences. `incompatible_pair` MUST be present when a decisive pair is known. |
| `memento-required-missing` | n/a (classifier surface) | Composition requires an input memento (loop-invariant, call-target proof, return-shape proof, etc.) classified `memento-required` by the discharge classifier but absent from the local pool. | `missing_memento_requirements` MUST be populated. `effect_occurrences` MUST include the occurrence(s) that triggered the requirement. |
| `ordering-conflict` | n/a | The atom chain or effect ordering constraints are cyclic, contradictory, or otherwise impossible to linearize. | `failure_detail` MUST identify the conflicting ordering predicates or atom CIDs. |
| `schema-version-mismatch` | `CompositionError::SchemaVersionMismatch` | Input atoms use different `FunctionContractMemento` schema versions; consumers must upgrade atoms to a common version before composing. | `failure_detail` MUST list the conflicting schema versions and the atoms carrying each. |
| `target-compile-failure` | n/a (target toolchain) | Emitted target source was rejected by the target language's standard compiler. | `failure_detail` MUST carry the compiler stderr. |
| `target-behavior-divergence` | n/a (fixture observation) | Emitted target source compiled, but runtime behavior on declared fixture inputs differs from the original source's behavior. | `failure_detail` MUST carry the expected-vs-observed comparison. |
| `unsatisfiable-precondition` | n/a (solver verdict) | The composed precondition cannot be satisfied, or an intermediate postcondition cannot establish the next precondition. | `failure_detail` MUST identify the failed implication, obligation CID, or relevant atom pair. |

Custom values are allowed through the `tstr` arm of `composition-failure`. Custom values MUST be stable lowercase tokens and MUST use a `<namespace>:<kind>` form when not intended for standardization (e.g., `ext:multisig-quorum`, `lab:experimental-classifier`). A verifier MUST preserve unknown values and MUST NOT collapse them into a generic error.

## §4 CID construction (JCS-canonical, BLAKE3-512)

The refusal CID is the CID of the refusal header with `cid` elided:

```text
cid_input = JCS({
  "atoms_cids":          [...],
  // Only when present:
  "blocking_effects":    [...],
  "ccp_version":         "...",
  "compose_input_cid":   "...",
  // Only when present:
  "effect_occurrences":  [...],
  "effect_set_cids":     [...],
  "failure_detail":      "...",
  "failure_kind":        "...",
  // Only when present:
  "incompatible_pair":   { ... },
  "kind":                "composition-refusal",
  // Only when present:
  "missing_memento_requirements": [...],
  "schemaVersion":       "1"
})

header.cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

The canonical byte stream MUST use JCS. Keys are sorted alphabetically by code point, absent optional keys are omitted, strings are UTF-8 JSON strings, and arrays preserve their protocol order. The digest is the full 64-byte BLAKE3-512 output encoded as 128 lowercase hexadecimal characters with the `blake3-512:` prefix.

`envelope.declaredAt`, `envelope.signer`, `envelope.signature`, `metadata.note`, `metadata.provenance_cid`, `metadata.refused_at`, and `metadata.source_url` are EXCLUDED from `cid_input` by construction. Two refusals over identical canonical inputs against the same `ccp_version` thus share the same `header.cid` even when minted by different signers, observed in different enclosing runs, or recorded at different wall-clock times.

The enclosing memento is signed after `header.cid` is inserted:

```text
signed_input = JCS({
  "header": header,
  "metadata": metadata
})

envelope.signature = Ed25519_sign(envelope.signer, signed_input)
```

`envelope` fields do not participate in `header.cid`. This allows re-attestation by a different signer without changing the refusal identity. Changes to `metadata` require a new signature but do not change `header.cid`.

## §5 Producer changes to CCP

CCP producers MUST map every externally visible `CompositionError` into a `CompositionRefusalMemento` before returning or publishing the refusal across a protocol boundary.

The revised normative behavior is:

```rust
compose_chain_contracts(atoms, effect_sets)
  -> Result<ComposedFunctionContract, CompositionRefusalMemento>
```

Implementations MAY keep the existing process-local `CompositionError` type internally. At the boundary, the producer MUST:

1. Preserve the attempted atom chain in `header.atoms_cids`.
2. Preserve the effect-set order in `header.effect_set_cids`.
3. Compute `header.compose_input_cid` over the canonical composition input (`atoms_cids`, `effect_set_cids`, `ccp_version`, plus any additional inputs the producer intends to pin).
4. Record `header.ccp_version`.
5. Classify the error into `header.failure_kind` using §3.
6. Populate `header.effect_occurrences` and `header.blocking_effects` for any failure_kind that requires them per §3.
7. Populate `header.missing_memento_requirements` when `failure_kind` is `memento-required-missing`.
8. Add `header.incompatible_pair` when the failure is a known pairwise effect incompatibility.
9. Emit deterministic `header.failure_detail`.
10. Compute `header.cid` per §4.
11. Sign `JCS({header, metadata})`.
12. Return or publish the memento CID with the memento body.

If the producer cannot classify the error into a reserved failure kind, it MUST use a stable namespaced custom `failure_kind` (`<namespace>:<kind>`) rather than returning a handle-only error.

## §6 Downstream semantics

A downstream consumer MAY refuse to compose through a failed composition by citing `header.cid` of a `CompositionRefusalMemento`. That cited CID is replayable evidence that the attempted composition failed under the producer's declared inputs and failure semantics.

Consumers MUST verify:

1. CDDL shape acceptance.
2. `header.kind == "composition-refusal"`.
3. `header.schemaVersion == "1"`.
4. `header.cid` recomputes from §4.
5. `envelope.signature` verifies over `JCS({header, metadata})`.
6. Referenced atom and effect-set CIDs are available or are explicitly recorded as unavailable by a higher-level pool policy.
7. `header.compose_input_cid` recomputes from the canonical composition input bytes whenever those bytes are available.

Consumers MUST NOT treat a refusal as evidence that no future composition can succeed. The refusal is scoped to the exact `compose_input_cid` and `ccp_version` in the header. A later composition with different atoms, different effect sets, a larger discharge budget, a revised CCP version, or upgraded schemas can produce a different result and a different CID.

When a consumer refuses to compose through a failed composition, it SHOULD cite the refusal CID and SHOULD include the producer signer when presenting the refusal to users or other protocol participants.

## §7 Cross-references

- `2026-05-09-contract-composition-protocol.md`: defines CCP composition behavior, the `CompositionError` enum (`ImpureInput`, `SchemaVersionMismatch`, `IncompatibleEffects`, `DeterminismViolation`), and the lifter-effect-tracking-gap soundness rule that this memento surfaces.
- `2026-05-06-effect-discharge-classification.md`: defines the per-occurrence classification (`block` / `memento-required` / `informational-dischargeable`) whose decisions surface here as `failure_kind = incompatible-effects` or `memento-required-missing`.
- `2026-05-13-effect-occurrence-memento.md` (issue #793): the structured occurrence payload that CCP consumes and that populates `effect_occurrences` and `blocking_effects[].discharge_key` here.
- `2026-04-30-canonicalization-grammar.md`: defines the JCS canonicalization rules used for CID and signature bytes.
- `2026-04-30-memento-envelope-grammar.md`: defines the shared memento envelope conventions.
- Issue #795: requests the canonical CCP refusal memento.
- Issue #796: admissibility-spine framing that refusals must be replayable and therefore content-addressed.

## §8 Out of scope

This specification does not define a new composition algorithm, new effect semantics, solver behavior, discharge-budget policy, pool replication policy, or UI wording for refusal display. It also does not require historical handle-level `CompositionError` values to be retroactively minted as mementos unless a producer republishes them across a protocol boundary.
