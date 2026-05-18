# R14: Floor / Ceiling Registration Tiers

**Date:** 2026-05-17
**Status:** Architectural ruling. Extends R1-R13 (`2026-05-17-realization-tag-kinds-and-marketplace-ruling.md`). Locks R14. **Interpretive only**: ratifies an existing cut in the substrate's data; introduces no new primitives, types, fields, schemas, or CLI flags.
**Authority:** T Savo (architect).
**Origin:** Conversational ruling following R1-R13. R1-R13 named the four tag locations and the marketplace dynamics. R14 names the cut between what a language kit MUST answer for vs what a vendor MAY answer for, grounded in the round-trip chain identity property and the functional-vs-conceptual loss distinction.

---

## TL;DR

The substrate's question space partitions into two tiers, derivable from data already present:

- **Floor**: questions a language KIT MUST answer for the chain identity to hold. Operationally measured by the existing classifier's `tag_kind` field: a kit is floor-complete for a concept iff `tag_kind` is anything other than `absent`.
- **Ceiling**: questions a VENDOR MAY answer to elevate sugar-carrier or boundary realizations into richer emission paths. Operationally: the per-library, per-framework enrichments that move a concept from `sugar-carrier` to `boundary` or `first-class` for a given language.

The cut is anchored in the chain identity property `kRust(kJava(kPython(I))) = I` and Sir's functional-vs-conceptual loss distinction: only functional losses (semantic divergences that must be reported) count as losses; conceptual losses preserved through sugar-carrier are not losses.

No new fields, schemas, or CLI flags are introduced. The existing `tag_kind` enum (5 values: first-class, composition, boundary, sugar-carrier, absent), `loss_record_contribution` in body templates, `propagate_effects` engine, `TransportGapMemento`, and the classify-realization-tags.py output are sufficient to compute floor / ceiling registration status.

---

## §1. The question this ruling answers

R1-R13 established the universal catalog (R1), four tag locations (R2), sugar-carrier as round-trip preservation (R3), sugar dicts as vendor plugins (R4), three observation shapes (R5), policy-mediated promotion (R6), multi-axis migration (R7), three federation layers (R8), vendor authorship (R9), IDE downstream (R10), manifest narrowing (R11), realization tagged enum (R12), and SDK tagging primitives (R13).

What R1-R13 did not state: WHICH questions in the exam manifest the language kit author MUST answer for the chain to function vs WHICH questions are vendor-extensible enrichments.

The architectural cut Sir surfaced:

> `+` is something every language MUST support. `http-request` is something a vendor CAN support. Think about how migrate works. Think about i32 to i64.

And later:

> The goal remains: `kRust(kJava(kPython(I))) = I' = kRust(kJava(kPython(I'))) = I`.

And the locked refinement:

> Round-tripping isn't just about invariance. It's about reporting side effects. Losses are functional losses that are reported, not conceptual losses that are dropped.

R14 names the cut and grounds it operationally in the chain identity property and the existing classifier output.

---

## §2. The ruling: R14

### R14: Registration tiers are floor and ceiling.

The substrate's exam questions partition into two tiers.

**Floor**: questions the language kit author MUST answer for the kit to participate in the chain identity property. A kit is floor-complete for a concept iff the kit's emission for that concept produces both:

1. A term that preserves concept identity through round-trip (one of the four positive tag locations: first-class, composition, boundary, or sugar-carrier), AND
2. A functional-loss report (if the kit's chosen emission path produces execution semantics that diverge from the concept's contract; otherwise the loss-record is empty).

**Ceiling**: questions vendors MAY answer to elevate a kit's emission from sugar-carrier to a more executable form. Vendors ship sugar dicts (R4), boundary contract bindings, witness adapters (R5), policy profiles (R6), and loss-record libraries naming per-vendor divergences. None of these affect chain identity; all of them affect what the lower can EXECUTE rather than carry as comment.

The cut is operationally definable from data the substrate already has (§3). The cut is the answer to "what is the kit's minimum bootstrap commitment" vs "what does the marketplace fill in over time."

---

## §3. The chain identity property defines floor

The chain `k_Ln(...k_L1(I)...)` is the round-trip composition of N language kits applied to a substrate term I. For the property `chain(I) = (I', report)` to hold for all I and all chains:

1. **Term identity**: every concept in I must round-trip through every kit in the chain. R3's sugar-carrier ensures this for concepts a kit doesn't natively realize.
2. **Report fidelity**: every kit's emission must produce its loss-record contribution alongside the emitted term. Without the report, the kit's claim of round-trip is fraudulent: it preserved syntactic identity while silently changing execution semantics.

The kit's exam answer per concept is a tuple `(emission_path, loss_record_contribution)`. Both legs must be present. Floor coverage = "every (concept, language) pair has a non-absent emission path AND its report is correctly wired."

If any `(concept, language)` pair in the universal catalog is `tag_kind=absent`, the chain identity property breaks for any I containing that concept. The kit is not floor-complete and is not a registered language.

---

## §4. Functional vs conceptual loss (Sir's locked refinement)

The exam asks: "for each universal concept, what functional losses do you incur when you emit?" Not: "what concepts can't you express?" The latter is free via sugar-carrier.

The five emission paths under this lens:

| `tag_kind` value | Term emitted | Functional loss reported | Trichotomy lens |
|---|---|---|---|
| `first-class` | native syntax | none required | **exact** |
| `composition` | built from concept-tier primitives | none required | **exact** |
| `boundary` | library binding | per-library divergence in loss-record contribution | **loudly-bounded-lossy** |
| `sugar-carrier` (no exec divergence) | concept-citation-comment | none required | **exact-at-carrier** |
| `sugar-carrier` (with exec divergence) | concept-citation-comment + loss-record naming what the language can't promise | per-divergence in loss-record | **loudly-bounded-lossy** |
| `absent` | nothing | nothing reported | **refuse**: chain breaks |

**Sugar-carrier is identity-preserving at the term level.** Whether it is functionally lossy depends on whether the language's default semantics happen to satisfy the concept's contract. The kit declares that, per concept, in its exam answer.

**The only true loss is the silent drop.** A kit that emits something but doesn't tell anyone what semantic it changed is unsound. The exam structurally prevents this by requiring per-emission loss-record reporting OR explicit `tag_kind=absent` (which the chain treats as refusal). Conceptual losses preserved through sugar-carrier are not losses; functional losses that are reported are bounded; functional losses that are silently dropped are unsound.

---

## §5. Where the cut already lives in existing code

R14 is interpretive. The data structures and code paths that make the cut computable already exist.

### §5.1 `tools/classify-realization-tags.py` as the floor-completeness check

The classifier already produces a `ClassificationRow` per `(concept, language)` pair with two fields directly relevant:

```python
@dataclass(frozen=True)
class ClassificationRow:
    language: str
    concept: str
    concept_class: str       # "primitive" | "abstraction"
    tag_kind: str            # "first-class" | "composition" | "boundary" | "sugar-carrier" | "absent"
    evidence: str
    source_paths: tuple[str, ...]
```

R14 reads this as follows:

- `tag_kind in {first-class, composition, sugar-carrier}` AND `concept_class == "primitive"` → floor coverage, native or carried
- `tag_kind == "composition"` AND `concept_class == "abstraction"` → floor coverage, abstraction composed from primitives
- `tag_kind == "boundary"` → ceiling enrichment (kit OR vendor MAY ship this elevation)
- `tag_kind == "absent"` → **registration gap**; the chain identity breaks for any I containing this concept in this language

A language is **floor-complete** iff every row in its classification output has `tag_kind != "absent"`. The classifier already runs; the classification rows are content-addressed in `docs/audits/2026-05-12-concept-library-completeness-probe.md`.

### §5.2 `libprovekit/src/effect_propagation.rs` as the report-composing engine

The PropagationDecision enum (Widen / Halt / Refuse) IS the per-function side-effect report for the floor-axis (effect propagation). The `propagate_effects` engine composes per-function decisions across the call graph. Every kit's lower integration that introduces an effect change produces a PropagationDecision per affected function, which composes into the chain's report.

This is the existing implementation of the chain identity property's report leg. No new infrastructure needed; R14 names that it exists.

### §5.3 Body templates with `loss_record_contribution` as the ceiling-axis report

`menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-aiosqlite.json` (and siblings per language) carry a `loss_record_contribution` field per emission entry. Example:

```json
"loss_record_contribution": {
  "form": "literal",
  "value": {
    "api_tier_concept_cid": { ... },
    "last_insert_id_loss_claim": {
      "head": "atomic",
      "name": "aiosqlite-last-insert-id-from-lastrowid"
    }
  }
}
```

When a kit lowers a concept via the aiosqlite body template, it emits BOTH the term AND the loss-record contribution naming the functional divergence (aiosqlite's `cursor.lastrowid` semantics differ from sqlite3's native return). This is the existing implementation of the per-emission report on the ceiling axis (per-library boundary divergences).

R14 names that body templates are the ceiling-side report-emission infrastructure.

### §5.4 `provekit-cli/src/cmd_bind_migrate.rs` as the orchestration

The migrate command builds per-callsite propagation input, runs `propagate_effects`, collects per-function decisions, and emits a receipt:

```rust
"{} functions widened to async",
receipt.aggregate_summary.widened
```

The receipt is content-addressed and federates by CID. This is the existing chain-identity-with-report implementation. R14 names that `provekit migrate` is the operational venue where the chain identity property's report leg lands.

### §5.5 `TransportGapMemento` with `exam_question_cid` as the citation surface

After #1126 (citation wiring), every gap record cites the specific exam question it addresses. A `tag_kind == "absent"` row in the classifier output corresponds to a TransportGapMemento with `exam_question_cid` pointing at the concept-realization question for that `(concept, language)` pair.

R14 names that a kit's floor-completion gap is content-addressed via the citation surface already wired.

---

## §6. The chain identity property under R14

The substrate's central claim is `kRust(kJava(kPython(I))) = (I', report)` where I' decodes back through the chain to I and `report` is the composition of per-kit loss-record contributions and per-function propagation decisions.

Under R14:

- **Floor-axis migration** (e.g., i32 to i64 sort migration; effect taxonomy changes): propagates through every registered kit's first-class / composition realizations via `propagate_effects`. Every kit is in the chain because every kit is floor-registered for the migrating concept. No vendor consultation; the report is composed from the propagation engine's decisions across the universal call graph.
- **Ceiling-axis migration** (e.g., requests to httpx; sqlite-dialect to postgres-dialect): propagates only through consumers of the relevant vendor sugar dict or body template. The vendor's loss-record-contribution library names the divergences. Other languages and consumers without the vendor plugin are unaffected.
- **Two-axis migration** (e.g., §3.3's TS sqlite-sync to postgres-async): both plans run simultaneously. The floor-axis plan widens TS functions to async per the kit's effect-classification answers; the ceiling-axis plan rewrites SQL strings per the dialect vendor's loss-record contributions. User reviews both; the migrate receipt composes both reports.

The chain identity property holds in all three cases because every kit in the chain has a non-absent emission path for every concept in I (floor-complete), and every emission produces its loss-record contribution (report-emission wired). The trichotomy lives in the report: empty loss-record = exact; non-empty loss-record = loudly-bounded-lossy; `tag_kind=absent` = refuse.

---

## §7. Federation under R14

R8's three federation layers (concept-tier composition, first-class morphism, boundary contract) get distinct staling behavior under R14:

**Concept-tier composition CID equality**: stays valid across floor-axis migrations. Because every floor-registered kit is in the migration, both sides of the federation re-hash against the migrated concept simultaneously. No version skew. Federation never stales at the concept tier.

**First-class morphism CID equality**: stays valid across floor-axis migrations for the same reason; both sides participate.

**Boundary contract CID equality**: CAN stale across ceiling-axis migrations. If consumer A loads the requests-to-httpx vendor migration and consumer B does not, A's boundary contract CIDs change while B's stay. Federation at concept-tier still works; federation at boundary-tier refuses on the staled contract with explicit divergence in the loss-record citation.

The federation guarantee falls out of the floor-axis universal propagation and the ceiling-axis per-vendor scope. No new federation machinery needed; R8's three-layer trichotomy with per-layer staling semantics already covers it.

---

## §8. What this changes

**Nothing in the code.** R14 is interpretive.

The data structures and code paths it names already exist:
- `ClassificationRow` with `tag_kind` and `concept_class` (classify-realization-tags.py)
- `PropagationDecision` Widen / Halt / Refuse (effect_propagation.rs)
- `loss_record_contribution` per body template entry (body-templates JSON)
- `TransportGapMemento` with `exam_question_cid` (after #1126)
- `propagate_effects` engine and `cmd_bind_migrate.rs` orchestration

The audit document at `docs/audits/2026-05-12-concept-library-completeness-probe.md` already contains per-language `tag_kind` rows. A reader applying R14's interpretation can compute floor-completeness for any registered language without modifying any code.

Per the substrate's first principle and Sir's no-sidechain directive: R14 does NOT propose any new schema fields, CLI flags, plugin manifest extensions, or memento variants. The existing primitives carry the cut.

---

## §9. What R14 enables in narration and documentation

Operator-facing language sharpens:

- **Bootstrap pitch**: "Register your language by answering the exam: produce a non-absent `tag_kind` for every concept in the universal catalog, with the loss-record contribution correctly populated for any path that diverges in execution semantics. That's floor-complete. Vendors fill in boundary elevations and per-library loss-records over time."
- **Coverage report**: a probe output reading classification rows can render per-tier coverage by inspecting `tag_kind` distribution. Floor-complete = zero absent rows. Ceiling-rich = high boundary-realization count from loaded vendor sugar dicts.
- **Federation contract**: "We federate at the concept tier with any floor-complete kit. We federate at the boundary tier with any vendor sharing our loaded sugar dicts. Staling is per-layer and reported via citation CIDs."

None of this requires code changes. It's how the substrate's existing data is read.

---

## §10. Open questions

R14 does NOT lock the following:

- The completeness probe (`tools/concept-library-completeness-probe.py`) is welcome to OPTIONALLY render per-tier summaries by inspecting `tag_kind` distribution. This is a presentation choice, not a schema change.
- The bootstrap-onboarding documentation rewrite (§9 above) is downstream prose work.
- The audit pass over `tag_kind == "absent"` rows to either fill with sugar-carrier or escalate concept-demotion as architect-call is operational follow-up.
- The relationship between R14's cut and the in-flight #1107 (probe consumes exam manifest) and #1108 (PEP 1.7.0 federation handshake) is harmonious: both PRs operate on the existing data this ruling interprets.

---

## §11. Closing

R1-R13 named the universal catalog, the four tag locations, the marketplace dynamics, the federation surfaces, and the migration axes. R14 names the cut already present in the substrate's data:

- **Floor**: every kit's commitment to non-absent emission with correct loss-record reporting. Measured by the classifier's `tag_kind != absent` per (concept, language). Required for the chain identity property to hold.
- **Ceiling**: vendor-shipped elevations from sugar-carrier to boundary or first-class, with vendor-shipped loss-record libraries naming per-library divergences. Per-consumer, per-vendor, additive.

The chain identity property `chain(I) = (I', report)` carries both legs: term identity through R3's sugar-carrier and report fidelity through the per-emission loss-record contribution and the per-function propagate_effects decision. The trichotomy lives in the report. Functional losses are reported; conceptual losses preserved by sugar-carrier are not losses.

Per the substrate's first principle (Supra omnia, rectum: above all, correctness), R14 introduces no new primitives. The existing classifier, propagation engine, body templates, transport gap mementos, and migrate command carry the cut. R14 names what is already there.

---

*End of ruling.*
