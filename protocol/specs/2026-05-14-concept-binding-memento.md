# ConceptBindingMemento Normative Spec

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-14
**Author:** T Savo
**Related:**
- `2026-05-12-plugin-protocol.md` (PEP 1.7.0 plugin protocol envelope)
- `2026-05-09-language-signature-protocol.md` (LSP; sorts are CID-named)
- `2026-05-13-effect-occurrence-memento.md` (`EffectOccurrence` typed payload)
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` (`LossRecord` typed values)
- `2026-05-15-concept-hub-abstraction-layer.md` (`ConceptAbstractionMemento` realization framing)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)
- `TSavo/provekit#856` (substrate honesty gradient + admissibility rule)
- `TSavo/provekit#848` (Bridge C: HTTP sugar cells; cells are reshaped per this spec)
- Paper 21 (`docs/papers/21-after-cross-language-every-cross-x-dissolves.md`)
- Paper 22 placeholder (*After Vendoring*)

## §0. Purpose

The substrate's `concept:*` hub names abstract operations (`concept:http-request`, `concept:add`, `concept:dynamic-dispatch`) that have, per target `(language, library)` cell, a **realization**: a way the language-and-library combination encodes the abstraction in what it has. The realize step at `provekit bind --target=python:httpx` needs to answer one query:

> Given `concept_cid` C, `target_language` L, and `target_library` Lib, what surface elements in L+Lib realize C, with what typed loss against C's contract?

That query is structurally distinct from two adjacent concerns the substrate already handles:

1. **Surface emission templates** (existing `SugarDictMemento`): how to write the target code as text once a binding has been chosen. Emission is downstream of binding selection.
2. **Surface recognition / lifting** (existing per-language lifters in `implementations/<lang>/`): how to identify, at lift time, that a surface call IS a realization of `concept:X`. Lifting is upstream of binding selection.

ConceptBindingMemento is the substrate's first-class artifact for the binding itself: the per-realization mapping from concept formal parameters to surface elements, the per-realization typed loss-record, the per-realization effect occurrences, and the discharge evidence supporting the binding. It is queryable, validatable, indexable, and refusable.

### §0.1 Admissibility rule

This spec defines a substrate-admissible memento family per the rule articulated in `#856`:

> Identity-valid but behavior-inert CIDs are not admissible default substrate cells.

A `ConceptBindingMemento` is admissible to the substrate's default registry only when ALL FOUR conditions hold:

1. **Parse**: the JSON conforms to the CDDL in §1.
2. **Validate**: each typed field (sorts, effects, loss-record values) round-trips through the relevant family's typed parser (`SortMorphismMemento` resolution, `EffectOccurrence` validation, `LossRecord` typing).
3. **Index**: the binding is recorded into `(concept_cid, target_language, target_library) -> [binding_cid]` and is queryable from the bind/transport/migrate commands.
4. **Refuse**: malformed content (missing required fields, untyped values, dangling CID references, contradictory loss-record shape) is REJECTED before any CID is admitted to the registry.

Without all four, the artifact is **drafting material**, not substrate. Drafting material may live in `menagerie/` for human review; it MUST NOT be added to the default registry.

### §0.2 What this is not

- Not a `SugarDictMemento`. Sugar dicts describe emission templates; bindings describe concept-to-surface mappings. A realize run consults BOTH: the binding to identify the cell, the sugar dict to author the target text. They are different mementos and live in different consumers.
- Not a `RealizationDesugaringMemento` (operation-tier desugaring). Bindings sit at the API tier above primitive ops; desugarings sit at the operation tier. Bindings reference desugarings (via the surface_mapping's compiled term) when the binding's surface element itself decomposes algebraically.
- Not a `ConceptAbstractionMemento` (abstraction-layer hub node). The abstraction-layer node defines the concept; the binding realizes it. One concept abstraction MAY have many bindings.
- Not an authoring-format file. This spec defines the wire format; the SBL DSL conversation in `project_provekit_libraries_ship_sugar.md` is about a future authoring source format that compiles to this wire shape.

### §0.3 Provenance modes

A `ConceptBindingMemento` may originate by one of four admission paths (per paper 21 §6 and the 2026-05-13 fourth-path realization):

- **Authored**: a substrate maintainer signs the binding.
- **Inferred**: the substrate clusters two surfaces' lifted terms, recognizes structural equivalence, mints a binding with the equivalence proof as evidence.
- **Generated**: a model proposes the binding; structural-equivalence discharge produces or refuses it. See `GenerativeCompletionProtocol` (`#841`).
- **Self-Attested**: the library author signs the binding alongside their package release. The `provenance_cid` cites the author's release-key provenance. This is the scaling path; paper 22 (*After Vendoring*) is built on it.

The substrate registry does not discriminate by path. Verifier policy decides which paths to trust; the wire format is uniform.

## §1. Wire shape (CDDL)

```cddl
; Shared scalar types:
;   cid, signature, pubkey, iso8601, json-value
;
; Locked JCS key order: alphabetical within each object.
; Producers MUST emit objects in JCS-canonical key order and MUST omit
; optional metadata fields when absent.

provenance-mode = "authored"
                / "inferred"
                / "generated"
                / "self-attested"
                / tstr   ; namespaced extensions, e.g. "ext:multi-sig"

surface-element = {
  ? args:               json-value,         ; surface-specific argument schema
  kind:                 tstr,               ; "method-call" / "function-call" / "class-method" / "macro" / "decorator" / ...
  locator:              tstr,               ; surface-language fully-qualified name, e.g. "urllib.request.urlopen"
  ? surface_signature:  json-value          ; surface-side typed signature (target-language types as strings)
}

surface-mapping = {
  ? executing_operations: [+ surface-element], ; surface elements that EXECUTE the concept (e.g. urlopen, fetch). At least one required when binding kind is "executes".
  ? request_slots:        { * tstr => [+ surface-element] }, ; concept formal name -> surface element(s) that PROJECT that formal slot (read or write)
  ? request_types:        [* tstr],         ; surface-language types that REPRESENT the constructed request value
  ? response_slots:       { * tstr => [+ surface-element] }, ; concept response-formal name -> surface element(s) that PROJECT that slot
  ? response_types:       [* tstr]          ; surface-language types that REPRESENT the response value
}

sort-binding = {
  cid:           cid,                       ; SortMorphismMemento CID for the concept-sort -> target-sort mapping
  concept_sort:  tstr,                      ; the formal sort name in the concept's signature, e.g. "HttpMethod"
  target_sort:   tstr                       ; the target-language type, e.g. "java.net.http.HttpMethod"
}

binding-kind = "executes"          ; binding declares the executing operation (concept:http-request → urlopen)
             / "constructs"        ; binding declares a typed value constructor (concept:url → URI(...))
             / "destructures"      ; binding declares a destructuring projection
             / "observes"          ; binding declares an observation surface (read-only)
             / tstr                ; namespaced extension

; Locked JCS key order (top level): envelope, header, metadata
concept-binding-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,                  ; over JCS(header ++ metadata)
    signer:     pubkey
  },
  header: {
    binding_kind:        binding-kind,      ; what the binding declares (see §2.3)
    cid:                 cid,               ; DERIVED, see §4
    concept_cid:         cid,               ; the concept this binding realizes
    effect_occurrences:  [* effect-occurrence], ; structured EffectOccurrence per #793. May be empty when the realization is observation-only.
    evidence_cids:       [+ cid],           ; discharge / equivalence / documentation evidence supporting the binding
    kind:                "concept-binding",
    loss_record:         loss-record,       ; per-dimension TYPED value; the dimension names come from the concept's loss_dimensions field
    provenance_cid:      cid,               ; ProvenanceMemento CID; for self-attested, the library author's release-key provenance
    provenance_mode:     provenance-mode,
    schemaVersion:       "1",
    sort_bindings:       [* sort-binding],  ; concept-sort -> target-sort morphisms; one per formal sort in the concept's signature
    surface_mapping:     surface-mapping,   ; surface element layout. At least one branch (executing_operations OR request_slots OR response_slots) MUST be non-empty.
    target_language:     tstr,              ; language identifier per LSP, e.g. "c", "java", "python", "rust"
    target_library:      tstr,              ; library identifier scoped to target_language, e.g. "libcurl", "java.net.http", "urllib.request", "httpx"
    target_surface:      tstr               ; surface-axis scope identifier, e.g. "stdlib-http-call-api", "okhttp3", "axios-v1". Distinguishes BINDING-SCOPE concerns from library-name. See §2.7.
  },
  metadata: {
    ? note:        tstr,
    ? rationale:   tstr,
    ? source_url:  tstr,
    ? tags:        [+ tstr]
  }
}

cid = tstr
signature = tstr
pubkey = tstr
iso8601 = tstr
json-value = any
```

`effect-occurrence` is defined in `2026-05-13-effect-occurrence-memento.md` §1.

`loss-record` is defined in `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §3. Its keys MUST be the loss-dimension names declared by the concept-shape's `loss_dimensions` field. Its values MUST be typed (per the host LossRecord schema), not prose strings.

## §2. Field semantics

### §2.1 `concept_cid`

The CID of the concept this binding realizes. MUST resolve to a `ConceptAbstractionMemento` (abstraction-tier) or an algorithm-shape memento (operation-tier, e.g. `concept:http-request_shape`). The binding adapts to the tier of its target.

### §2.2 `target_language`, `target_library`, `target_surface`

Three orthogonal identifiers that together form the index key.

- `target_language` names the source-language ecosystem (`c`, `java`, `python`, `rust`, `typescript`, ...). Aligned with LSP language identifiers.
- `target_library` names the specific library or surface family within that language (`libcurl`, `java.net.http`, `urllib.request`, `httpx`, `aiohttp`, `requests`). Two bindings differ in `target_library` when their surface elements come from different installable packages or stdlib modules.
- `target_surface` is a binding-scope identifier that names the API surface CONTRACT, not the install. Distinct from `target_library` because:
  - A library may expose MULTIPLE surface contracts (`httpx` exposes both a sync `httpx.Client` surface and an async `httpx.AsyncClient` surface; each gets its own binding with a different `target_surface`).
  - A surface contract may live in MULTIPLE libraries (the `axios-v1`-shape API is exposed by `axios` itself and by drop-in replacements). When a developer specifies their preferred surface in `provekit migrate --target-surface=axios-v1`, the realize can pick whichever installed library implements it.

The triple `(target_language, target_library, target_surface)` is the canonical addressing. Two of these can be sufficient when the third is implicit; the wire format always carries all three.

### §2.3 `binding_kind`

What the binding declares.

- `executes`: the bound surface element performs the concept's operation. `concept:http-request` is `executes`. The `surface_mapping.executing_operations` field is mandatory.
- `constructs`: the bound surface element constructs a typed value of the concept's return-sort. `concept:url` typically has bindings of kind `constructs`. The `surface_mapping.request_types` or `response_types` field is mandatory.
- `destructures`: the bound surface element projects fields out of a concept value. `concept:http-response` has `destructures` bindings on `response_slots`.
- `observes`: a read-only projection. Used when a surface element exposes the concept's state without modifying it.

A single concept may have multiple binding kinds in the same `(language, library)` cell. They are distinct mementos.

### §2.4 `surface_mapping`

The structural mapping from concept formal slots to target-language surface elements. At least one of `executing_operations`, `request_slots`, `request_types`, `response_slots`, `response_types` MUST be non-empty.

For `binding_kind = "executes"`:
- `executing_operations` MUST be non-empty and list every surface element that, when called, performs the concept's operation.
- `request_slots` SHOULD be populated when the surface accepts structured arguments: it maps each concept formal slot name (e.g. `method`, `url`, `headers`, `body`) to the surface element(s) that project that slot. Empty when the surface accepts only positional arguments.
- `response_slots` SHOULD be populated when the surface exposes structured response fields, mapping each concept response-formal slot name to the surface element(s) that project it.

For `binding_kind = "constructs"` / `"destructures"` / `"observes"`: the relevant branches of `surface_mapping` are populated; the rest are omitted.

### §2.5 `sort_bindings`

Each entry is a CID reference to a `SortMorphismMemento` (per `2026-05-13-sort-morphism-memento.md`) that proves the concept's formal sort can be represented by the named target-language type. One entry per formal sort in the concept's signature. If the concept declares N sorts, `sort_bindings` MUST have N entries.

A binding without `sort_bindings` covering every concept sort is REFUSED at admission.

### §2.6 `effect_occurrences`

Per-realization EffectOccurrence entries (per `2026-05-13-effect-occurrence-memento.md`). Captures what observable effects this specific realization triggers when its `executes` surface element runs. For `concept:http-request` bound to `urllib.request.urlopen`, the effect occurrence is `NetworkRequest { target: locator, kind: "send", payload: ... }`. May be empty when the realization is observation-only.

`effect_occurrences` is the substrate's primary handle for telling the realize pass which side-effect family the emitted code will activate. Loss-record's `effect_divergence` dimension references this field.

### §2.7 `loss_record`

A typed `LossRecord` (per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §3). Its keys MUST EXACTLY EQUAL the loss-dimension names declared by the concept-shape's `loss_dimensions` field (per Bridge B). Its values MUST be typed per the LossRecord schema. Prose strings wrapped in atomic-predicate-clothing are REFUSED as a substrate-anti-pattern per `#856`.

Each loss-dimension VALUE describes what this binding's realization does on that dimension:
- A typed enum where the dimension has a known finite vocabulary (e.g. `cancellation: "AbortController" | "CancelToken" | "task-cancel" | "manual-only" | "refused"`).
- A typed structured value where the dimension has structured semantics (e.g. `retry_policy: { kind: "exponential-backoff", max_attempts: N, base_ms: ... }`).
- An explicit `refused` typed value with a refusal reason, when the dimension cannot be supported.

The substrate consumer (§6) is responsible for validating each value against the dimension's typed schema. Schemas for the loss-dimension value vocabulary are minted as `LossDimensionValueMemento` (out of scope for this spec; tracked as a follow-up).

In the interim, before per-dimension value schemas are minted, this spec defines a TYPED-PROSE fallback: `{kind: "value", representation: "prose", description: tstr}`. Bindings using this fallback are admissible only when the binding's `provenance_mode` is `authored`; `self-attested` and `generated` bindings MUST use minted dimension-value schemas. This rule pushes the value-schema minting work forward without blocking authored migration.

### §2.8 `evidence_cids`

At least one CID supporting the binding's correctness. Acceptable evidence types:
- `DischargeReceipt` from a solver verifying the binding's contract correspondence
- `EvidenceMemento` (per `2026-05-13-compound-contract-memento.md`) carrying test-assertion, type-signature, docstring, or native-surface evidence
- `EquivalenceClaim` for inferred bindings
- `DocumentationCitation` for authored or self-attested bindings (a content-addressed snapshot of the library's official documentation page declaring the API)

The `evidence_cids` array MAY be heterogeneous. The validating consumer (§6) checks that at least one evidence CID resolves and discharges; bindings with zero discharging evidence are REFUSED.

### §2.9 `provenance_cid`, `provenance_mode`

The `provenance_cid` is a `ProvenanceMemento` (existing substrate type) recording who minted the binding, with what signing key, against what trust anchor. The `provenance_mode` enum tags the admission path; verifier policy weights modes differently.

For `self-attested` bindings, the `provenance_cid` MUST resolve to a provenance memento whose signer is the library's published release-key. The substrate consumer (§6) does NOT verify the library author's identity; it verifies that the signature on the binding matches the signature on a recent release of the named library at the named version. Trust-by-author is a policy decision; the substrate's job is to make the assertion content-addressable and federation-checkable.

## §3. Validation rules

A consumer admitting a `ConceptBindingMemento` to the registry MUST enforce ALL of the following before recording the CID:

1. **Envelope signature verifies** against the `signer` public key over `JCS(header ++ metadata)`.
2. **Header `cid`** equals the recomputed CID per §4. The producer's stored `cid` field is informational only.
3. **`concept_cid` resolves** in the local catalog or in a referenced federation snapshot. Dangling concept references are REFUSED.
4. **`sort_bindings` covers every concept formal sort** with a resolved `SortMorphismMemento`.
5. **`effect_occurrences` typed-validate** per `EffectOccurrence` v1.0.0 schema. Untyped or malformed occurrences are REFUSED.
6. **`loss_record` keys equal the concept's `loss_dimensions` exactly**, and each value typed-validates per §2.7. No silent omissions; no extra keys.
7. **`surface_mapping`** has at least one populated branch consistent with `binding_kind` (per §2.4).
8. **At least one `evidence_cid` resolves and discharges** per its evidence-type validation rules.
9. **`provenance_cid` resolves** to a `ProvenanceMemento`; the binding's `signer` matches the provenance memento's recorded signer.
10. **No em-dashes, no en-dashes** in any string field. The substrate's `no-em-dash` rule applies.

A binding failing any of these is admitted to a `CompositionRefusalMemento` (per `2026-05-13-composition-refusal-memento.md`) with `refusal_kind: "binding-admission-refused"` and `refusal_detail` naming the violated rule. The refusal CID is durable and citeable; the binding CID is NOT recorded in the registry.

## §4. CID derivation

The CID is computed as:

```
JCS(header_without_cid_field) -> bytes
cid = "blake3-512:" || hex(BLAKE3-512(bytes))
```

The `header.cid` field is elided before canonicalization, then the stored `cid` field is set to the computed value before signing. The signature covers the canonicalized `header ++ metadata` with the populated `cid` field.

Producers MUST verify that re-canonicalization of the populated memento produces the recorded `cid`. Implementations whose `cid` field disagrees with the canonical recomputation are buggy and MUST be rejected by the validating consumer.

## §5. Index shape

The substrate maintains a `ConceptBindingRegistry` indexed by:

```
(concept_cid, target_language, target_library) -> [binding_cid]
```

The value is a list because a given `(concept, language, library)` cell may have multiple bindings differing in `target_surface` (sync vs async API, builder pattern vs simple call). Realize-time selection within a list is policy-driven (see §6).

The registry SHOULD also expose secondary indices on:
- `(concept_cid, target_language) -> [binding_cid]` (for "find any realization in this language")
- `(target_language, target_library) -> [binding_cid]` (for "all concepts this library realizes")
- `(provenance_mode) -> [binding_cid]` (for "all self-attested bindings"; useful for trust filtering)

Producers MUST NOT mint multiple `ConceptBindingMemento` artifacts with identical `(concept_cid, target_language, target_library, target_surface, provenance_mode)` and the same `provenance_cid`; the registry deduplicates by CID, but the intent here is that a single author cannot create competing bindings for the same cell without changing one of these scoping fields.

## §6. Substrate consumer contract

The plugin loader does NOT semantically understand `ConceptBindingMemento` content. The loader's job is generic: register by `(kind, CID)`, CID-check, expose loaded memento bytes by `kind` and `CID`. Per `#856` admissibility rule, the consumer that does typed validation MUST live elsewhere and MUST be installed as a precondition for bindings to enter the default registry.

The consumer is named `ConceptBindingRegistry`. Its location is implementation-defined (likely `libprovekit` or an adjacent module). Its contract is:

1. **Load**: take a `ConceptBindingMemento` from the plugin loader by `(kind = "concept-binding", CID)`.
2. **Validate**: enforce §3 rule-by-rule. On failure, mint a `CompositionRefusalMemento` and do not index.
3. **Index**: on success, record the binding under the primary key from §5 and any secondary indices.
4. **Query**: expose a query API to bind/transport/migrate:
   - `find_binding(concept_cid, target_language, target_library, target_surface?) -> Option<BindingHandle>`
   - `list_bindings(concept_cid, target_language) -> [BindingHandle]`
   - `list_languages_for_concept(concept_cid) -> [target_language]`
5. **Selection policy**: when `find_binding` returns multiple candidates, selection is delegated to the caller. The registry returns candidates ordered by `(provenance_mode rank, declaredAt desc)`, where `authored > self-attested > inferred > generated` is the default rank. Callers MAY override.

The registry is a separate Rust crate from the plugin loader, so the loader can stay protocol-generic. New memento families that need typed validation register their own consumers; the admissibility rule does not require ONE consumer per family, only that SOME consumer covers each family.

### §6.1 Initialization order

When a binding cites a sort-morphism CID, an evidence CID, or a provenance CID that resolves to a memento family with its own consumer (e.g. `SortMorphismMemento` has a `SortMorphismRegistry`), the binding's admission is gated on the cited mementos being indexed in their respective registries FIRST. The bind/transport/migrate flow MUST construct registries in dependency order: provenance, sort-morphism, effect-occurrence, then concept-binding.

### §6.2 Refusal posture

A binding cannot be silently fail-overed at realize time. If `find_binding(...)` returns `None`, the realize pass mints a `CompositionRefusalMemento` with `refusal_kind: "no-binding-for-target"` and STOPS. Falling back to a sibling library or to the canonical-of-language behavior is a policy decision the caller makes explicitly with a fallback-binding query, not a default the registry silently performs.

This posture aligns with the broader substrate refusal pattern: refusal is preferable to silent loss; loudly-bounded-lossy stubs are preferable to silent emission of the wrong target; silent emission of any kind is the failure mode this protocol exists to prevent.

## §7. Federation

`ConceptBindingMemento` is content-addressed and signed. Two federated parties holding the same JSON bytes compute the same CID; two federated registries indexing the same binding CID at the same key produce identical query results.

Federation correctness REQUIRES that the consumer at every federation node enforce the §3 validation rules identically. A node that admits a binding with a missing sort_binding is producing a different graph than a node that refuses it. The substrate's `Supra omnia, rectum` first principle is at stake: do not loosen validation for convenience.

Federation snapshots (`CatalogSnapshotMemento` per `2026-05-13-catalog-snapshot-memento.md`) MUST list the `ConceptBindingMemento` CIDs included in the snapshot, alongside their `(concept_cid, target_language, target_library)` index keys.

## §8. Relationship to other mementos

| Memento family | Role | Relationship to ConceptBindingMemento |
|---|---|---|
| `ConceptAbstractionMemento` | Defines the abstract concept | Cited by `concept_cid` |
| Algorithm-shape spec memento (`concept:X_shape`) | Operation-tier concept definition | Also citable by `concept_cid` for op-tier concepts |
| `SugarDictMemento` | Emission templates (text formatting in target) | Co-located with binding at realize time; bindings ARE NOT sugar dicts |
| `SortMorphismMemento` | Concept-sort to target-sort proof | Cited by `sort_bindings[].cid` |
| `EffectOccurrence` (embedded in FCM) | Per-occurrence effect payload | Embedded in `effect_occurrences` |
| `LossRecord` (in PartialMorphism/LossyMorphism) | Per-dimension typed values | Embedded in `loss_record`; same typed schema |
| `EvidenceMemento` (in compound contracts) | Discharge / equivalence evidence | Cited by `evidence_cids` |
| `ProvenanceMemento` | Who signed, with what key | Cited by `provenance_cid` |
| `RealizationDesugaringMemento` | Operation-tier desugaring | Bindings reference these when the surface element decomposes algebraically |
| `RealizationPlanMemento` | Realize-pass decision artifact | Realize PRODUCES this from a binding; the binding is INPUT |
| `CompositionRefusalMemento` | Refusal of admission or realize | MUST be minted when binding fails §3 or §6.2 |
| `CatalogSnapshotMemento` | Federation snapshot | MUST list bindings by `(concept, lang, lib)` |

The bindings consumer does NOT consume or produce sugar dicts. The relationship between bindings and sugar dicts is: realize selects a binding, then consults the sugar dict for that target to author the text. Two separate query steps, two separate consumers, two separate memento families.

## §9. Migration: existing sugar-dict invention

Bridge C (`#848`) authored six HTTP cells with a `concept_bindings[]` field invented inside the sugar dict's opaque `content` blob. Those cells do not conform to this spec. Per `#856` admissibility rule, they are NOT admissible to the substrate registry.

The Bridge C reshape (gated on this spec landing) rewrites the six cells as `ConceptBindingMemento` artifacts conforming to §1. The loss-record values authored in the original cells are preserved as drafting material; the surface_mapping data carries over. The reshape's outputs are six new CIDs (the original `concept_bindings[]`-inside-sugar-dict CIDs are not re-used).

The original branch (`bridge-c-http-sugar-cells` on origin) is marked as drafting material; no PR is opened against main; the loss-record content is reference for the reshape.

## §10. Future work (out of scope for this spec)

1. **LossDimensionValueMemento family**: per-dimension typed value schemas, replacing the typed-prose fallback in §2.7. Each loss-dimension declared on a concept gets a value-schema memento. Bindings reference the schema to validate their values.
2. **SBL (Sugar Binding Language)**: authoring DSL that compiles to ConceptBindingMemento + SugarDictMemento pairs. The DSL is paper-22-prep tooling, not substrate. Locked declarative grammar per `project_provekit_libraries_ship_sugar.md`.
3. **Self-attested signing flow**: tooling for library authors to mint and sign ConceptBindingMemento alongside `npm publish` / `cargo publish` / `pip upload`. Includes registry hooks for ecosystem package managers.
4. **Binding equivalence proofs**: when two bindings in different `(language, library)` cells claim to realize the same concept, the substrate may prove their equivalence by composing their sort_bindings + surface_mapping + loss_record. This is a separate equivalence-claim memento and is paper-22-receipt material for the cross-library trinity.
5. **Generated-binding discharge**: tooling and protocol for `#841` GenerativeCompletionProtocol to propose ConceptBindingMemento candidates and discharge them through structural-equivalence checking before admission.

## §11. Conclusion

A `ConceptBindingMemento` is the substrate's first-class artifact for the answer to "given concept C and target (L, Lib, Surface), how does the surface realize C, what is preserved, what is lost, and what supports the claim?" It is content-addressed, signed, validated by a dedicated typed registry, and refusable on failure of any of nine validation rules. The plugin loader stays generic; the binding consumer does the work the loader explicitly is not built to do. The four-path provenance model (authored / inferred / generated / self-attested) is on the wire and uniform; trust filtering is verifier policy.

The substrate's commitment behind this spec is the admissibility rule:

> Identity-valid but behavior-inert CIDs are not admissible default substrate cells.

A `ConceptBindingMemento` is admissible only when SOME substrate consumer can parse, validate, index, and refuse malformed instances. This spec is what gives that consumer a target to be built against.
