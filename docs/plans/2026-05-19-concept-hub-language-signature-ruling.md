# Concept Hub Language Signature

Date: 2026-05-19
Status: Active. Ratifies the substrate-canonical concept hub as a LanguageSignatureMemento per `protocol/specs/2026-05-09-language-signature-protocol.md`. Required prerequisite for cross-language morphism mementos that target substrate-canonical sorts, ops, or effects.

## Ruling

The substrate's concept hub (the union of all canonical sorts, op algorithms, dimensions, and effects under `menagerie/concept-shapes/catalog/`) is itself a language in the LSP sense (Language Signature Protocol per `2026-05-09-language-signature-protocol.md`). A `LanguageSignatureMemento` for the concept hub is minted under `menagerie/concept-hub-language-signature/specs/`. Its CID anchors substrate-canonical targets for cross-language morphism mementos.

This is the rectum-correct answer to the question that surfaced in #1284: when a `SortMorphismMemento` answers a sort-classification question ("how does language X realize concept:Float?"), the `target_language_signature_cid` field has a real meaning instead of being a sentinel or a duplicate of the source signature.

## §1. Why this exists

`SortMorphismMemento` (per `protocol/specs/2026-05-13-sort-morphism-memento.md` §1) requires `target_language_signature_cid` for every memento. The field "pins the language version and ABI within which target_sort_cid is interpreted" (§2). For language-to-language morphisms (e.g., Rust `i64` ↔ Java `long`), both signature CIDs are existing per-language LanguageSignatureMementos. For language-to-substrate morphisms (e.g., Rust `f64` → `concept:Float`), the target side has no language-signature anchor today.

Three candidate interpretations were considered:

1. **Mint a substrate-concept-hub-signature memento.** Selected. Per paper 13 §1 (programming language grammars are algebras), the concept hub IS a language in the substrate's terms. Treating it as a peer language follows the existing pattern.
2. **Reuse the source signature CID for both fields.** Rejected. Loses semantic content; `target_language_signature_cid` becomes a free field that carries no claim.
3. **Use a sentinel like `blake3-512:0...0`.** Rejected. Documentary placeholder; substrate-honesty-incompatible.

## §2. The signature memento shape

Following `menagerie/<lang>-language-signature/specs/language_signature_<lang>.spec.json` precedent, the concept hub's signature lives at:

```
menagerie/concept-hub-language-signature/specs/language_signature_concept_hub.spec.json
```

Content shape (per `protocol/specs/2026-05-09-language-signature-protocol.md` §1.3, mirroring existing per-language signatures):

```json
{
  "kind": "language_signature",
  "fn_name": "concept-hub:v1",
  "sorts": [
    "<sort-name-1>.spec.json",
    "<sort-name-2>.spec.json",
    ...
  ],
  "operations": [
    "<algorithm-name-1>.spec.json",
    ...
  ],
  "equations": [],
  "effect_signatures": []
}
```

The `sorts` field enumerates the substrate-canonical sorts under `menagerie/concept-shapes/catalog/sorts/`. The `operations` field enumerates substrate-canonical concept-op algorithms under `menagerie/concept-shapes/catalog/algorithms/` whose `fn_name` is `concept:*` (excluding `morphism:*`, `sort-morphism:*`, and similar cross-language morphism mementos that are NOT concept-hub ops themselves).

Equations and effect_signatures stay empty for v1; substrate-canonical equations and effects mint later if needed.

CID computation: standard JCS+blake3-512 over the spec content. The resulting CID is the substrate's pin for "concept hub v1." Future evolutions of the hub (new sort mints, new concept-ops) increment the version (`fn_name: "concept-hub:v2"`) and produce a new CID. Old CIDs remain valid pins to their version of the hub.

## §3. Versioning

The concept hub is content-addressed; its identity changes when its content changes. Specifically:

- A new sort mints (e.g., adding `concept:Char` in a future PR) requires extending the signature's `sorts` array and re-minting the signature spec. CID changes.
- A new concept-op mints (e.g., adding `concept:literal` per #1282) requires extending the `operations` array. CID changes.
- Existing morphism mementos that pin an outdated `target_language_signature_cid` REMAIN VALID: they pin a specific version of the hub; the substrate respects content-addressed history.
- Consumers who want to target the LATEST hub version pin the latest signature CID. They DO NOT modify old morphism mementos; they mint new ones if needed.

This matches paper 04's rank-N tuple pinning model: each artifact (sort, op, signature, morphism) has its own content-only CID; consumer policy decides which version to admit.

## §4. Minting cadence

The concept-hub-signature is minted ALONGSIDE substrate evolution. Specifically:

- Initial mint (this PR's deliverable per #1284's prerequisite): pin the current set of `catalog/sorts/` entries (Bool, Bytes, Cid, EffectName, Float, Formula, Int, List<T>, Map<K,V>, Null, OpCid, SortCid, String, Term) plus the current `catalog/algorithms/concept:*` ops.
- Subsequent re-mints: any PR that adds a substrate-canonical sort or concept-op extends the signature spec and re-mints. The PR's diff includes both the new canonical entry AND the signature update.
- Re-minting is mechanical: a script (sibling to `mint_concept_literal.py`) recomputes the signature from current `catalog/` state.

## §5. Use cases

This signature anchors cross-language morphism targets:

- **SortMorphismMemento (sort-classification answers).** Per `2026-05-13-sort-morphism-memento.md`. `target_language_signature_cid` = concept-hub-signature CID; `target_sort_cid` = substrate-canonical sort CID (e.g., concept:Float).
- **Future: ConceptOpMorphismMemento (op-realization answers).** When the substrate mints a parallel memento class for cross-language op realizations targeting concept-hub ops. Same anchor.
- **Future: EffectMorphismMemento (effect-classification answers).** Same anchor.

In each case, the substrate-canonical target gets pinned to a specific hub version; consumers verify against that pin.

## §6. What this ruling deliberately does NOT do

- Does NOT change `SortMorphismMemento`'s wire shape. The existing required-field set holds.
- Does NOT introduce a new dispatcher, comparison primitive, or verifier path. The substrate's existing kit-dispatch + compare machinery loads the hub signature like any other LanguageSignatureMemento.
- Does NOT enforce a particular versioning cadence. Each substrate-canonical addition decides whether to bump the hub-signature version inside its own PR.
- Does NOT define how language kits' published exam-answer morphisms reference this hub signature. Per #1284 implementation: each kit's sort-classification SortMorphismMemento answer cites this hub-signature CID as `target_language_signature_cid`. Per future op/effect classification answers: same anchor.

## §7. Implementation

Land via a dedicated PR:

1. Create directory `menagerie/concept-hub-language-signature/specs/`.
2. Create `menagerie/concept-hub-language-signature/specs/language_signature_concept_hub.spec.json` with the full enumeration of current substrate-canonical sorts AND concept-ops. The enumeration script reads `catalog/sorts/` filenames + `catalog/algorithms/concept:*.json` filenames and emits the spec deterministically.
3. Compute the signature's CID via JCS+blake3-512.
4. Add the CID to `menagerie/concept-shapes/cids.tsv`.
5. Add a script `menagerie/concept-shapes/scripts/mint_concept_hub_signature.py` for deterministic re-mints; document its invocation in the README.
6. Schema validation test: pin the signature CID; assert deterministic recompute.

After landing: #1284 can be unblocked. Each SortMorphismMemento for Float/Null per language uses this hub-signature CID as `target_language_signature_cid`.

## §8. Cross-references

- Language Signature Protocol: `protocol/specs/2026-05-09-language-signature-protocol.md`.
- SortMorphismMemento spec: `protocol/specs/2026-05-13-sort-morphism-memento.md`.
- Paper 13 (After Grammars): `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`.
- Substrate-uniform pattern: `docs/explanation/substrate-uniform-pattern.md`.
- Existing per-language signature template: `menagerie/rust-language-signature/specs/language_signature_rust.spec.json`.
- Substrate-canonical sort catalog: `menagerie/concept-shapes/catalog/sorts/`.
- Substrate-canonical concept-op catalog: `menagerie/concept-shapes/catalog/algorithms/concept:*.json`.
- Concept:literal mint (#1282 landing): `68d2f1f8a`. First substrate-canonical concept op that benefits from this signature anchor.
- Issue #1284: sort-classification SortMorphismMemento answers (blocked on this ruling + the prerequisite mint issue).

## §9. Discipline

When any future substrate-canonical sort or concept-op mints, the same PR re-mints the concept-hub-signature spec to reflect the new content. The signature CID changes; downstream morphism mementos pinning the OLD CID remain valid (anchored to the old version). Consumers wanting to target the new hub mint new morphisms.

If a PR proposes minting a substrate-canonical sort or op WITHOUT re-minting the hub signature, reviewer raises this ruling and asks: "is this addition intentionally outside the current hub version?" Default answer: no; re-mint the signature.
