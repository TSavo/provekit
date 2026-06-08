# PlatformSemanticTag schema, LossRecord composition, refusal pattern

**Date:** 2026-05-16
**Status:** Independent architectural ruling, requested by T Savo.
**Scope:** Lock the schema for `PlatformSemanticTag` mementos, the substrate machinery that composes a `LossRecord` from two tags, and the refusal pattern when dimensions are structurally incommensurable.
**Inheritance:** Composes with the locked Platform-Semantics-via-LossRecord ruling (`docs/plans/2026-05-16-platform-semantics-via-loss-records.md`), the γ canonical-term-shape ruling (`docs/plans/2026-05-16-canonical-term-shape-form.md`), and the β operand-binding sidecar ruling (`docs/plans/2026-05-16-operand-binding-sidecar-schema.md`).

## TL;DR

T's correction is correct and the question is unified, not three.

1. **Schema:** `PlatformSemanticTag` is a flat memento with a `dimensions: BTreeMap<String, Cid>` field. Keys are open kit-minted dimension names. Values are CIDs of `DimensionValueMemento`s (each value is itself a separately-minted content-addressed memento). The substrate enumerates nothing.
2. **Composition machinery:** Substrate computes a `LossRecord` (already `BTreeMap<String, IrFormula>` per `sugar-ir-types/src/lib.rs:508`) by **pairwise dimension comparison on the intersection of key sets**, with CID-equality as the equivalence relation and the kit-minted `DimensionValueMemento`'s declared `compare_to` formula as the divergence-characterizer.
3. **Refusal pattern:** Asymmetric set-difference on dimension keys is a hard refusal, not a partial loss. The substrate emits a `RefusalMemento` with `kind: "uncharacterizable_platform_divergence"` listing `source_only_dimensions` and `target_only_dimensions`. The port halts. This is the third leg of the trichotomy at the platform-semantics boundary.

The three faces are one decision: open-keyed map at the schema layer forces pairwise-intersection at the composition layer forces hard-refuse-on-asymmetric-keys at the trichotomy layer. Each layer's correctness depends on the next layer's shape. They lock together.

## 1. The PlatformSemanticTag schema (exact JSON)

```json
{
  "kind": "platform-semantic-tag",
  "schemaVersion": "1.0.0",
  "kit_cid": "blake3-512:<kit-registration-cid>",
  "op_cid": "blake3-512:<target-op-cid>",
  "dimensions": {
    "<dim_name_1>": "blake3-512:<DimensionValueMemento-cid>",
    "<dim_name_2>": "blake3-512:<DimensionValueMemento-cid>",
    "..."
  },
  "cid": "blake3-512:<self-cid>"
}
```

Where `DimensionValueMemento` is its own canonical mintable shape:

```json
{
  "kind": "platform-dimension-value",
  "schemaVersion": "1.0.0",
  "dimension_name": "<dim_name>",
  "value_name": "<value_name>",
  "compare_to": "<IrFormula>",
  "cid": "blake3-512:<self-cid>"
}
```

### Why this shape

The substrate already chose `BTreeMap<String, IrFormula>` for `LossRecord` itself (line 508). The dimension-key set is **already open** at the loss-emission layer. Choosing anything other than an open `BTreeMap` at the tag layer would create an impedance mismatch: kits could mint platform behaviors that the LossRecord layer can express, but the tag layer can't carry. The tag schema must be **at least as open as the loss schema**, and any closure beyond what loss requires is denormalization.

`dimensions` is `BTreeMap<String, Cid>`, not `BTreeMap<String, serde_json::Value>` or `BTreeMap<String, String>`, because:

- **Content-addressing on dimension values is non-negotiable.** Two kits independently minting `"Wrapping"` for `ArithmeticOverflowMode` must produce byte-identical CIDs or federation breaks at the dimension-value layer. The string `"Wrapping"` doesn't enforce this. A canonical `DimensionValueMemento` with declared `dimension_name` + `value_name` + `compare_to` does.
- **The `compare_to` formula lives where it's content-addressed.** When source-kit and target-kit both reference the same `DimensionValueMemento` CID for `Wrapping`, they agree on the equation that defines "Wrapping" mathematically. When they reference different CIDs even with the same string name, the divergence is by construction made visible and characterizable: the substrate compares their two `compare_to` formulas via the existing IrFormula machinery.
- **Forward-compat with kit-specific dimensions.** A quantum kit minting `dimension_name: "MeasurementBasis"` with values `Computational`, `Hadamard`, `Bell` works without substrate involvement because the substrate only knows: keyed map, CIDs as values, compare via the value's own `compare_to`. The substrate's knowledge is structural, not enumerative.

### Why a flat BTreeMap and not the dimensions-as-list shape

The candidate `{kit_cid, op_cid, tag_name, dimensions: [DimensionMemento]}` (list of dimension mementos) was considered. Rejected for two reasons:

- **Sidecar-pattern precedent.** The platform-semantics sidecar on main is already `HashMap<Cid, PlatformSemanticTag>` (flat keyed map). The operand-binding ruling cited this as the established sidecar shape. Using `BTreeMap` inside the tag itself preserves shape-consistency one layer down. (`BTreeMap` not `HashMap` because canonical-key-order matters for the tag's own CID stability.)
- **Composition machinery needs keyed lookup.** Pairwise dimension comparison reads `source.dimensions["X"]` and `target.dimensions["X"]`. A list shape requires either O(N²) scan or building a map at compare time. Mint-once-in-the-shape-you'll-query.

### Why not the opaque-CID shape

The candidate `{kit_cid, op_cid, tag_cid: Cid}` (substrate sees only equality) was also considered. Rejected because it collapses the trichotomy's middle leg. Without dimensions exposed at the substrate boundary, every non-equal tag pair becomes uncharacterizable: the LossRecord is forced to be `{"opaque_platform_divergence": ...}` always. This violates the locked Platform-Semantics-via-LossRecord ruling's claim that "behavioral divergence is captured automatically via the substrate's existing LossRecord / LossRecordMemento family"; that claim requires substrate-visible structure to compose against.

## 2. LossRecord composition machinery

Given source-kit tag `A` and target-kit tag `B` (both with the same `op_cid`), the substrate computes:

```
fn compose_loss(A: &PlatformSemanticTag, B: &PlatformSemanticTag)
    -> Result<LossRecord, RefusalMemento>
{
    // Step 1: dimension-key set comparison
    let a_keys: BTreeSet<&String> = A.dimensions.keys().collect();
    let b_keys: BTreeSet<&String> = B.dimensions.keys().collect();

    if a_keys != b_keys {
        return Err(uncharacterizable_refusal(A, B, a_keys, b_keys));
    }

    // Step 2: pairwise CID-equality on shared dimensions
    let mut loss = BTreeMap::new();
    for dim_name in &a_keys {
        let a_value_cid = &A.dimensions[*dim_name];
        let b_value_cid = &B.dimensions[*dim_name];
        if a_value_cid == b_value_cid {
            continue;  // no loss in this dimension
        }

        // Step 3: load both DimensionValueMementos by CID,
        // produce divergence IrFormula by composing their compare_to formulas
        let a_value: DimensionValueMemento = load(a_value_cid)?;
        let b_value: DimensionValueMemento = load(b_value_cid)?;
        let divergence_formula =
            IrFormula::DivergenceBetween {
                source: Box::new(a_value.compare_to),
                target: Box::new(b_value.compare_to),
            };

        // dimension name keys directly into LossRecord
        loss.insert(dim_name.to_string(), divergence_formula);
    }

    Ok(LossRecord(loss))
}
```

### Structural reasoning

- **CID-equality is the equivalence relation, not string-name-equality.** Two `Wrapping` mementos minted in two kits that compute different `compare_to` formulas ARE different platform semantics, even if both call themselves `Wrapping`. The substrate must not be deceived by name collision.
- **Skip identical-CID dimensions (no loss).** A `BTreeMap` entry's absence already encodes "no loss in this dimension" per the `LossRecord` doc-comment on line 494. Identical CIDs produce no entry.
- **Use the kits' own `compare_to` formulas.** The substrate does not enumerate what `Wrapping` vs `Saturating` means. Each `DimensionValueMemento` declares its own `compare_to` formula in IrFormula. The substrate composes them via the existing IrFormula `DivergenceBetween` node (new variant; mintable). The substrate's knowledge of platform semantics is **referential, never definitional**.
- **The result is a plain `LossRecord`.** No new memento family. The result composes immediately with the existing `LossRecordMemento` envelope, the existing `MigrateReceiptEnvelope.loss_records: Vec<LossRecordMemento>` field, and the existing `MigrateReceiptIndex.loss_record_cids` audit channel. Zero ripple.

### Why pairwise, not lattice or kit-comparator

- **Subsumption lattice rejected.** Requires the substrate to know an order over dimension values. Closes the value set in practice (no kit can extend the lattice without substrate cooperation). Violates the open-value-set constraint.
- **Kit-comparator rejected.** Federation-fragile. Source-kit and target-kit could declare disagreeing comparators; the substrate would have no principled way to decide which is authoritative. CID-equality on the values, combined with the values' own self-declared `compare_to`, makes both kits' contributions independently auditable. The substrate is the neutral arbiter, not a chooser between competing kit code.

## 3. Refusal pattern: asymmetric dimension keys

When `A.dimensions.keys() != B.dimensions.keys()` (set-inequality), the substrate emits:

```json
{
  "kind": "refusal",
  "schemaVersion": "1.0.0",
  "forbidding_contract": "<platform-semantics-port-contract-cid>",
  "function_cid": "<op_cid>",
  "reason": "uncharacterizable_platform_divergence",
  "source_kit_cid": "<A.kit_cid>",
  "target_kit_cid": "<B.kit_cid>",
  "source_dimensions": ["X", "Y"],
  "target_dimensions": ["X", "Z"],
  "source_only_dimensions": ["Y"],
  "target_only_dimensions": ["Z"],
  "cid": "<self-cid>"
}
```

This is a hard halt. The port does not proceed with a partial LossRecord.

### Why hard-refuse, not per-dimension fallback

The trichotomy locked in `project_sugar_first_principle` is exact / loudly-bounded-lossy / refuse. A partial LossRecord covering only shared dimensions would be **lossy in the loss-characterization itself**: the receipt would say "loss in X, Y" but silently drop the fact that target couldn't even express dimension Y. That is exactly the silent-platform-behavior-loss this whole architecture exists to prevent.

The fact that source-kit declares a dimension target-kit doesn't expose is a structural statement: target-kit's model of this op's semantics is **categorically smaller** than source-kit's. The substrate cannot honestly bound the loss because it has no `compare_to` formula for the missing dimension on the target side. There is nothing to compose against. The honest answer is refusal.

The reverse asymmetry (target has dimensions source doesn't) is equally a refusal. Target-kit is claiming a richer semantic model than source-kit. Without a source-side `compare_to`, the substrate cannot verify that target-kit's extra semantics are actually instantiated by anything in source's behavior; the port would be inventing structure.

### Why a `RefusalMemento`, not a partial-LossRecord with a marker

The existing `RefusalMemento` and `MigrateReceiptEnvelope.refusal_mementos: Vec<RefusalMemento>` field are the established audit channel for "port halted." Reusing them keeps the trichotomy's three legs at the same architectural altitude:

- Exact → port proceeds with no LossRecord
- Loudly-bounded-lossy → port proceeds, `MigrateReceiptEnvelope.loss_records` non-empty
- Refuse → port halts, `MigrateReceiptEnvelope.refusal_mementos` non-empty

Substrate maintainers reading the envelope see the trichotomy directly in the field-presence pattern. No marker-fields, no overloaded semantics. Field-presence IS the trichotomy verdict.

## 4. Why all three are one decision

The unifying observation: **the openness of the schema, the shape of the composition, and the form of the refusal are constrained by each other.**

- Open-keyed-map at the schema layer forces the substrate to compute on the **intersection** of key sets. Anything else is the substrate inventing or denying structure it doesn't own.
- Intersection-composition forces the **set-difference** to be the substrate's halt-condition: there is no architecturally honest way for an intersection-composer to produce a defined value on the set-difference.
- Hard-refuse-on-asymmetry forces the schema to **expose the keys to the substrate**: an opaque-CID schema couldn't even detect the asymmetry. The refusal pattern's existence depends on the schema's openness.

Pick any one of the three and the other two are determined. The architectural mistake T caught at the value-set layer (substrate enumerates vs. kits mint) propagates upward and downward: the schema, the composition, and the refusal are the same M+N-vs-M×K question asked three times. The answer is the same in all three: **substrate provides the structural machinery, kits mint the content, the trichotomy lives in field-presence on the existing envelope.**

## 5. Tradeoffs honestly stated

There IS a real tradeoff: this design will produce more refusals at the port boundary than a kit-comparator design would. Two kits with semantically equivalent platform models that mint differently named dimensions (`"OverflowMode"` vs `"IntegerOverflowMode"`) will refuse to port despite being morally compatible. The remediation is **kit-side dimension-name alignment** (or a future `concept:dimension-name` hub at the abstraction tier, analogous to `concept:*` for ops). This cost is intentional. Under Supra omnia, rectum, a refusal that names the exact dimension-key asymmetry is correct; a silent merge under a kit's permissive comparator is not.

The other tradeoff: kits must mint `DimensionValueMemento`s with real `compare_to` formulas, not opaque tokens. This is more work at kit-registration time than the substrate-enumerated alternative. It is also where the substrate's M+N property comes from: the work is per-kit (N), not per-(kit-pair) (M×N). Pay once at mint, federate forever.

## 6. Acceptance criteria

When this ruling lands:

1. `PlatformSemanticTag` and `DimensionValueMemento` types exist in `sugar-ir-types/src/lib.rs` with the exact field order and JCS key-order specified above.
2. `IrFormula::DivergenceBetween { source, target }` variant exists.
3. `compose_loss(&PlatformSemanticTag, &PlatformSemanticTag) -> Result<LossRecord, RefusalMemento>` exists in `libsugar::core` and is called by `execute_path` at every cross-platform port site per op in the algebra.
4. `MigrateReceiptEnvelope.refusal_mementos` carries `kind: "refusal"` with `reason: "uncharacterizable_platform_divergence"` whenever dimension-key sets differ.
5. Tests cover: (a) byte-identical tags yield empty `LossRecord` (exact leg); (b) tags differing in one dimension's CID yield single-entry `LossRecord` with that dimension's name as key (lossy leg); (c) tags with asymmetric dimension key sets yield `RefusalMemento` with both `source_only_dimensions` and `target_only_dimensions` populated (refuse leg); (d) two kits minting same-string-name dimension values with different `compare_to` formulas correctly report divergence under CID-inequality despite string-equal `value_name`.

## 7. Cross-references

- `docs/plans/2026-05-16-platform-semantics-via-loss-records.md`: the locked architectural ruling this schema implements
- `docs/plans/2026-05-16-canonical-term-shape-form.md`: the γ ruling whose substrate-provides-schema-kits-mint-values principle this propagates
- `docs/plans/2026-05-16-operand-binding-sidecar-schema.md`: the β ruling whose flat-keyed-map sidecar pattern this preserves
- `implementations/rust/sugar-ir-types/src/lib.rs:508`: existing `LossRecord = BTreeMap<String, IrFormula>` that this composes against
- `implementations/rust/sugar-ir-types/src/lib.rs:4744`: existing `RefusalMemento` reused for the third leg
- `implementations/rust/libsugar/src/core/types.rs:808`: existing `ConformanceDeclaration::Carrier` to be extended with `PlatformSemanticsDeclaration` carrying `HashMap<Cid, Cid>` from `op_cid` to `PlatformSemanticTag` CID
- `project_sugar_first_principle` (Supra omnia, rectum) and the trichotomy
