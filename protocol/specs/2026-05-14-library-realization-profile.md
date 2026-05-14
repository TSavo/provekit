# LibraryRealizationProfile Normative Spec

**Status:** v1.0.0 normative draft (draft 5; drafts 1-4 superseded by Opus and Codex reviews).
**Date:** 2026-05-14
**Author:** T Savo
**Related:**
- `2026-05-15-concept-hub-abstraction-layer.md` §2.2 (`RealizationDesugaringMemento`, abstraction-tier semantic realization)
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.2 (`PartialMorphismMemento`), §1.4 (`LossyMorphismMemento`)
- `2026-05-13-effect-occurrence-memento.md` (occurrence_kind vs signature_cid layer split)
- `2026-05-13-proof-run-memento.md` and stage-receipt schema (admission audit)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)
- `TSavo/provekit#856` (substrate admissibility rule and honesty gradient)
- `TSavo/provekit#848` (Bridge C: HTTP cells reshape against this profile)
- Paper 21 (`docs/papers/21-after-cross-language-every-cross-x-dissolves.md`)
- Paper 22 placeholder (*After Vendoring*)

## §0. Purpose

`LibraryRealizationProfile` is an **index / coordinate memento** over an already-valid, already-discharged semantic realization. It answers exactly one query:

> For `concept_cid`, targeting `target_lang` / `target_library` / `target_surface`, which already-discharged semantic-realization CID should the realizer consider?

The profile carries no semantic content of its own. It does not prove the mapping, carry the loss, declare effects, or describe emission. The semantic memento cited at `semantic_realization_cid` does all of that; the profile is a coordinate-indexed citation of it.

The query the profile answers is the missing piece for `provekit migrate` and cross-library transport: the existing semantic memento families (`RealizationDesugaringMemento`, `PartialMorphismMemento`, `LossyMorphismMemento`) carry `target_lang` but not `target_library` or `target_surface`. The profile adds those coordinates and indexes the semantic chain underneath.

### §0.1 Disjointness

This profile is NOT and does NOT do the following:

- **NOT a semantic primitive.** It carries no `pre`, `post`, `wp_rule`, `homomorphism_obligation`, or any other proof or discharge data. The cited semantic memento carries those.
- **NOT extending or subclassing any parent CDDL.** It is a separate bare-object memento whose only links to the rest of the substrate are explicit CID citations.
- **NOT a carrier of `loss_record` or `effects` data.** Both stay on the cited semantic memento. The profile MAY include documentary `capability_profile` tags for query convenience; those tags are not the source of truth for what the realization preserves or loses.
- **NOT a `SugarDictMemento`.** Sugar dicts describe emission templates (the text the realize pass writes). Profiles describe target-coordinate indexing of semantic realizations. They are different layers.
- **NOT carrying its own envelope or signer.** This memento is a bare object. Authorial trust is supplied by catalog admission and promotion-decision mementos that cite the profile.

### §0.2 The `NetworkRequest` layer split

A clarification this spec resolves explicitly so consumers do not confuse the layers:

- `EffectOccurrence.occurrence_kind` is the v1 operational family. For HTTP it is normally `Io`. The v1 list defined in `2026-05-13-effect-occurrence-memento.md` §3 is closed: `Reads`, `Writes`, `Io`, `Panics`, `OpaqueLoop`, `UnresolvedCall`, `AtomicAccess`, `EarlyReturn`, `Unsafe`, `ClosureCapture`, `PinnedReference`, `RawPointerProvenance`, `PossibleAliasing`, `Drop`, with `LegacyUnknown` for back-compat. HTTP function-contract occurrences SHOULD use `Io { channel: "network", operation: ... }`.
- `EffectOccurrence.signature_cid` cites an `EffectSignatureMemento`. The signature catalog is extensible per language and per domain. An HTTP signature MAY be named `NetworkRequest` and minted into the catalog; an occurrence with `occurrence_kind: "Io"` MAY cite that `NetworkRequest` signature CID.
- Bridge B's `concept:http-request` shape carries `{kind: "effect-signature", name: "NetworkRequest"}` at the SIGNATURE layer. This is signature-name metadata on the concept shape, NOT an occurrence_kind. Bridge B needs no amendment.

The profile carries no effect data of either kind. Effects ride on the cited semantic memento and its discharge.

## §1. Wire shape (CDDL)

```cddl
; Imports:
;   cid                            ; tstr, BLAKE3-512 hex-encoded with prefix
;
; Locked JCS key order at the top level: alphabetical. The profile is a
; bare object, not envelope-wrapped. Authorial trust is supplied by the
; catalog admission and promotion-decision mementos that cite the profile,
; not by this memento itself.

concept-tier = "algorithm"
             / "abstraction"
             / "operation"
             / tstr   ; namespaced extensions, e.g. "ext:paradigm"

semantic-realization-kind = "realization-desugaring"   ; concept-hub §2.2; abstraction-tier
                          / "partial-morphism"          ; transport-gap §1.2; per-op morphism with precondition
                          / "lossy-morphism"            ; transport-gap §1.4; per-op morphism with loss-record
                          / tstr                        ; namespaced extension, e.g. "ext:generated"

capability-form = "yes"
                / "no"
                / "manual"
                / "partial"
                / "refused"
                / tstr   ; library-specific tag, e.g. "AbortController"

capability-entry = {
  ? detail:  tstr,                 ; documentary elaboration; not discharge-able
  form:      capability-form,
  ? since:   tstr                  ; semver since-version where the capability appeared
}

capability-profile = { * tstr => capability-entry }   ; open vocabulary; documentary query metadata only

target-version-constraint = {
  ? max:   tstr,
  ? min:   tstr,
  ? range: tstr                   ; semver range expression, e.g. "^1.0.0", ">=2.5,<3.0"
}

library-realization-profile = {
  capability_profile:        capability-profile,
  concept_cid:               cid,
  concept_tier:              concept-tier,
  semantic_realization_cid:  cid,
  semantic_realization_kind: semantic-realization-kind,
  target_lang:               tstr,                          ; LSP language id
  target_library:            tstr,                          ; library or stdlib module name
  target_surface:            tstr,                          ; API surface contract label
  ? target_version:          target-version-constraint      ; applicability filter, NOT admission gate
}

cid = tstr
```

Locked top-level JCS key order (alphabetical): `capability_profile`, `concept_cid`, `concept_tier`, `semantic_realization_cid`, `semantic_realization_kind`, `target_lang`, `target_library`, `target_surface`, `target_version`. The CID is BLAKE3-512 of the JCS-canonical bytes of the full object. There is no embedded `cid` field, no envelope, no signature on the profile itself.

### §1.1 Why `language-morphism` is NOT in the v1 enum

`LanguageMorphismMemento` (LSP §1.4) describes whole-language signature-to-signature translations. That granularity is wrong for a `(target_library, target_surface)` index, which is per-op. The three included kinds (`realization-desugaring`, `partial-morphism`, `lossy-morphism`) all admit per-op citations. A future spec MAY add a `language-morphism` value if a consumer needs whole-language indexing; until then, the enum is tighter without it.

## §2. Field semantics

### §2.1 `concept_cid` and `concept_tier`

`concept_cid` is the CID of the concept whose realization the profile indexes.

`concept_tier` declares which tier the concept lives at: `algorithm` for algorithm-shape mementos like `concept:http-request` (per `menagerie/concept-shapes/specs/`), `abstraction` for `ConceptAbstractionMemento` instances (concept-hub spec), `operation` for operation-tier ops. `concept_tier` is documentary metadata that lets consumers dispatch validation without re-inspecting the cited memento at every step.

The substrate's flat address space (per `docs/explanation/api-tier-concept-tagging.md`) lets concepts at any tier share one CID space. `concept_tier` is the index-side label.

### §2.2 `semantic_realization_cid` and `semantic_realization_kind`

`semantic_realization_cid` is the CID of the semantic memento that proves and carries the realization. The profile does NOT prove the realization; it cites a memento that has already discharged.

`semantic_realization_kind` identifies the memento family:

- `realization-desugaring`: cites a `RealizationDesugaringMemento` from concept-hub §2.2 (abstraction-tier desugaring of an abstraction-concept into a target-language operation-layer term, with `target_lang`, `loss_record`, `discharge_receipt`)
- `partial-morphism`: cites a `PartialMorphismMemento` from transport-gap §1.2 (per-op morphism with `validity_precondition`, valid under a side-condition)
- `lossy-morphism`: cites a `LossyMorphismMemento` from transport-gap §1.4 (per-op morphism with `loss-record`, valid into a characterized coarsening of the target)

Each kind has its own validator (the consumer per memento family); the profile's admission requires the cited memento to validate under that validator (§3 rule 5).

### §2.3 `target_lang`, `target_library`, `target_surface`

Three coordinates that together form the profile's index key (§5):

- `target_lang`: LSP language identifier (`c`, `c11`, `java`, `python`, `rust`, ...).
- `target_library`: library or stdlib module name (`urllib.request`, `httpx`, `aiohttp`, `libcurl`, `java.net.http`, `okhttp3`, `reqwest`). Conventional identifier; the substrate does not maintain a registry of valid library names.
- `target_surface`: human-readable label for the API surface contract within `target_library` (`Client`, `AsyncClient`, `urlopen-direct`, `easy-perform`). The authoritative surface identifier is the cited morphism's `target_shape_cid` (for partial/lossy morphism) or the realization-desugaring's `post.rhs` operator. `target_surface` on the profile is the index-side string label that makes the catalog queryable by readable name; consumers MUST treat `target_shape_cid` as authoritative for actual selection.

### §2.4 `target_version` (applicability, NOT admission)

Optional semver constraint declaring which versions of `target_library` honor the cited realization. The profile is admitted to the registry regardless of whether the local environment satisfies the constraint. Realize-time candidate selection filters by environment compatibility; a profile whose constraint is not satisfied is SKIPPED at selection, not REJECTED at admission.

This separation matters: catalog admission is a federation-stable decision (any party seeing the bytes agrees), while applicability is environment-local (depends on what is installed). Mixing them would break federation correctness.

### §2.5 `capability_profile` (documentary)

Open-vocabulary map of capability-name to capability-entry. Documentary query metadata. NOT the source of truth for what the realization preserves or loses; that lives in the cited semantic memento's `loss_record` or `validity_precondition`. The substrate consumer MAY use `capability_profile` for query convenience (filter realizations by `cancellation: "AbortController"`); it MUST NOT use `capability_profile` for soundness decisions.

Conventional dimensions for HTTP realizations (illustrative, not normative):

```
capability_profile: {
  cancellation:    { form: "AbortController" },
  cookie_jar:      { form: "yes" },
  retries:         { form: "partial", detail: "transport-level only" },
  streaming_body:  { form: "yes" },
  sync_vs_async:   { form: "async" },
  timeout:         { form: "yes" },
  tls_pinning:     { form: "manual" }
}
```

## §3. Validation rules

A consumer admitting a `LibraryRealizationProfile` to its registry MUST enforce:

1. **Recomputed CID matches the claimed CID.** Loader-level check (BLAKE3-512 of JCS bytes equals the address used to fetch).
2. **`concept_cid` resolves** in the local catalog or a referenced federation snapshot. Dangling concept references rejected.
3. **`semantic_realization_cid` resolves.** Dangling realization references rejected.
4. **The cited semantic memento validates under its declared `semantic_realization_kind`.** Per-kind validators (the `RealizationDesugaringMemento` consumer, the `PartialMorphismMemento` consumer, the `LossyMorphismMemento` consumer) run their own admission rules. A profile whose cited memento fails its own validator is rejected.
5. **The cited semantic memento has discharged.** For `realization-desugaring`: its `discharge_receipt` field resolves to a valid `MorphismDischargeReceipt`. For `partial-morphism`: a `PartialMorphismDischargeReceipt` exists citing the morphism CID. For `lossy-morphism`: a `LossyMorphismDischargeReceipt` exists citing the morphism CID. A profile citing an undischarged realization is rejected.
6. **`concept_cid` matches the cited realization's source.** Per-kind check:
   - `realization-desugaring`: the cited memento's `post.lhs` is `concept:<X>(<slots>)` and `<X>` resolves to the profile's `concept_cid`. The lhs operator IS the abstraction-concept identifier.
   - `partial-morphism`: the cited memento's `source_contract_cid` equals the profile's `concept_cid`.
   - `lossy-morphism`: the cited memento's `source_contract_cid` equals the profile's `concept_cid`.
   - `ext:*`: the extension defines the check.
7. **`concept_tier` is consistent with `semantic_realization_kind`.** `realization-desugaring` requires `concept_tier` of `abstraction`. `partial-morphism` and `lossy-morphism` admit `algorithm` or `operation` tier. `ext:*` defines its own compatibility.
8. **`target_lang`** is a known LSP language identifier.
9. **`target_library` and `target_surface`** are non-empty strings.
10. **`target_version`** (if present) is a well-formed semver constraint. The local environment's library version does NOT enter the admission decision.
11. **`capability_profile`** is a well-formed map. Each entry's `form` is a recognized value or a library-specific tag string.
12. **No em-dashes, no en-dashes** in any string field.

A profile failing any of these is rejected from admission. The rejection is recorded by the admitting consumer in the active `RunMemento` / `StageReceipt` (per `2026-05-13-proof-run-memento.md` and the stage-receipt schema) with the violated rule cited in stage diagnostics. The profile CID is NOT recorded in the registry.

`CompositionRefusalMemento` is NOT used for admission rejections; that memento is CCP-shaped (`compose_input_cid`, `atoms_cids`, `effect_set_cids`, `ccp_version`, `failure_kind`) and reserved for contract-composition failures.

## §4. CID derivation

```
JCS(profile_object) -> bytes
cid = "blake3-512:" || hex(BLAKE3-512(bytes))
```

The CID is computed externally and not embedded in the object. Same pattern as the bare-object semantic mementos this profile cites. Consumers reading a profile by claimed CID MUST re-hash and compare before admitting.

Authorial trust (signer, declaredAt) is supplied by the catalog admission and promotion-decision mementos that cite the profile, not by this object.

## §5. Index shape

The substrate maintains a `LibraryRealizationRegistry` indexed by:

```
(concept_cid, target_lang, target_library, target_surface) -> [profile_cid]
```

The four-tuple is the primary key. `target_surface` is a primary-key component (not a list discriminator) because one library may expose multiple surface contracts that must be addressable independently.

Secondary indices recommended for consumer convenience:
- `(concept_cid, target_lang) -> [profile_cid]` for "all realizations of concept in language"
- `(concept_cid, target_lang, target_library) -> [profile_cid]` for "all surfaces in this library that realize concept"
- `(semantic_realization_kind) -> [profile_cid]` for "all profiles citing this kind of semantic memento"

`find_profile(concept_cid, target_lang, target_library, target_surface)` returns profile candidates. When `target_version` is present on multiple candidates, the caller (bind / transport / migrate) MAY filter by local environment compatibility before selecting.

## §6. Substrate consumer contract

The generic plugin loader does NOT semantically understand `LibraryRealizationProfile`. Its job is generic: register by `(kind, CID)`, CID-check, expose bytes by `kind` and `CID`.

The `LibraryRealizationRegistry` is the typed consumer. It owns §3 validation, §5 indexing, and rejection accounting. Per `#856` admissibility rule:

> Identity-valid but behavior-inert CIDs are not admissible default substrate cells.

A profile cannot enter the default registry unless §3 validation passes AND §5 indexing succeeds. Validation includes delegating to the per-kind validator for the cited `semantic_realization_cid` (rule 4) and confirming discharge (rule 5). Failures are recorded in the active `RunMemento` / `StageReceipt`.

### §6.1 Refusal posture

If `find_profile(...)` returns no candidates for the requested key, the realize pass records the missing-realization fact in the active `StageReceipt` with diagnostics citing `(concept_cid, target_lang, target_library, target_surface)` and stops. Falling back to a sibling library or surface is a POLICY decision the caller MUST request explicitly via a separate query; silent degradation is forbidden.

## §7. Federation

Federated parties holding the same JCS bytes compute the same CID and admit the same registry entry. The four-tuple primary key is federation-stable.

Cross-citation correctness: a federated registry admitting a profile MUST also have admitted (or be able to admit) the cited `semantic_realization_cid` under its appropriate per-kind consumer. Federation snapshots (`CatalogSnapshotMemento` per `2026-05-13-catalog-snapshot-memento.md`) MUST list both the profile and its cited semantic chain, or the snapshot is incomplete and a consumer MUST refuse the profile until the cited memento is available.

## §8. Per-kind citation table

| `semantic_realization_kind` | Cited memento | `concept_cid` matches | `concept_tier` compatible with |
|---|---|---|---|
| `realization-desugaring` | concept-hub §2.2 `RealizationDesugaringMemento` | operator of `post.lhs` (the abstraction concept being realized) | `abstraction` |
| `partial-morphism` | transport-gap §1.2 `PartialMorphismMemento` | morphism's `source_contract_cid` | `algorithm`, `operation` |
| `lossy-morphism` | transport-gap §1.4 `LossyMorphismMemento` | morphism's `source_contract_cid` | `algorithm`, `operation` |
| `ext:*` | extension-defined | extension-defined | extension-defined |

The morphism direction is **realize-direction**: concept on the source side, library surface on the target side. `provekit transport` uses a profile by reading the cited morphism and emitting the target surface for source-program sites bound to the concept.

## §9. Bridge C migration: four-mint chain per cell

Bridge C (`TSavo/provekit#848`) authored six HTTP cells with an invented `concept_bindings[]` field inside opaque sugar-dict content. Those cells did not conform to any minted spec.

Per `#856` admissibility rule, those cells were never admissible to the substrate registry. The reshape (gated on this spec landing) produces **four mints per `(concept, target)` cell**:

1. **Target operation contract**: a `FunctionContractMemento` describing the per-library op (e.g. `python:httpx.Client.get` with its formal sorts, pre, post, effects). This is the `target_shape_cid` the morphism will reference.
2. **Realize-direction morphism**: a `LossyMorphismMemento` (or `PartialMorphismMemento` when the realization is exact under a side-condition) with `source_contract_cid = concept:http-request.cid` and `target_shape_cid = <target op contract CID>`. Carries the loss-record (or precondition).
3. **Discharge receipt**: a `LossyMorphismDischargeReceipt` (or `PartialMorphismDischargeReceipt`) certifying the morphism is wp-preserving modulo its loss-record (or under its precondition).
4. **LibraryRealizationProfile**: this spec's object, citing the morphism CID by `semantic_realization_cid` and recording `(target_lang, target_library, target_surface, target_version?, capability_profile)`.

For six cells (C+libcurl, Java+java.net.http, Python with four libraries) the reshape is 24 substrate artifacts. The drafting branch `bridge-c-http-sugar-cells` on origin carries per-library capability content (cancellation model, retry behavior, etc.) that survives into the profile's `capability_profile`; its invented schema fields are discarded.

A cell whose target-op contract / morphism / discharge has not been minted cannot have an admissible profile. Profiles are minted LAST in the chain.

## §10. Future work (out of scope)

1. **Publisher attestation memento family** for paper 22's Self-Attested path. Library authors sign alongside package release; the substrate accepts the attestation as catalog admission evidence. Not invented by this spec.
2. **`loss_summary_cid`**: a future field that cites a derived, query-friendly summary of the cited semantic memento's loss-record. The authoritative loss stays on the cited morphism; the summary is a denormalization for query speed. Not included in v1.0.0.
3. **Capability vocabulary minting** (`CapabilityVocabularyMemento`): typed enum vocabularies per capability dimension, lifting `capability_profile` from open-string to typed-enum.
4. **`language-morphism`** as a semantic kind: when a consumer needs whole-language signature-to-signature indexing rather than per-op. Add only when a real use case demands it.
5. **Cross-library equivalence proofs**: when two profiles in different `(language, library)` cells cite morphisms that compose to identity, the substrate may prove their equivalence by composing the cited morphisms. Paper 22's cross-library trinity receipt is one shape of this.

## §11. Conclusion

`LibraryRealizationProfile` is a thin index over an already-valid semantic realization. It cites; it does not prove. Three legitimate semantic kinds in v1.0.0: `realization-desugaring` (abstraction tier, concept-hub §2.2), `partial-morphism` (operation/algorithm tier, transport-gap §1.2), `lossy-morphism` (operation/algorithm tier, transport-gap §1.4). The profile adds `target_library`, `target_surface`, `target_version`, `capability_profile` over those cited mementos.

Admission requires the cited `semantic_realization_cid` to have already validated and discharged under its own per-kind consumer. The profile carries no proof, no loss, no effects, no emission. It carries coordinates and a citation.

The substrate's commitment behind this spec is the admissibility rule:

> Identity-valid but behavior-inert CIDs are not admissible default substrate cells.

A `LibraryRealizationProfile` is admissible only when (a) the `LibraryRealizationRegistry` validates it AND (b) the cited `semantic_realization_cid` has validated and discharged under its own per-kind consumer. Index-only-without-semantic-grounding is rejected. Semantic-without-index does not participate in `find_profile`. Both layers must be present for cross-library transport to work.
