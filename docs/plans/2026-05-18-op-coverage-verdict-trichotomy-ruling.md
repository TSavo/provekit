# OpCoverageVerdict: Four-State Trichotomy Ruling

Date: 2026-05-18
Status: Active. Implemented in libsugar/src/core/types.rs via PR #1204.

## Ruling

`PlatformSemanticsDeclaration::compare_op_with(op_cid, other)` returns a four-state `OpCoverageVerdict`, not a two-or-three-state result:

```rust
pub enum OpCoverageVerdict {
    NoOpinion,
    Uncharacterizable { absent_on: Side },
    Same,
    Divergent(DivergenceCharacterization),
}

pub enum Side { Source, Target }
```

Each variant has a load-bearing semantic distinct from the others:

- **NoOpinion** = neither kit declares a tag for this op-CID. The substrate has no claim about this op-pair. Caller should continue without action. This is NOT "the operations agree"; it's "the substrate has nothing to say."
- **Uncharacterizable { absent_on }** = exactly one kit declares a tag. The substrate CAN see a divergence (presence vs absence) but CANNOT characterize it (the absent kit gave us no value memento to compare against). Caller must route through the refuse leg. This is NOT "the operations might agree"; it's "the substrate refuses to claim because half the comparison is missing."
- **Same** = both kits declare tags and dimension values are identical. No divergence to characterize. Caller takes no action.
- **Divergent(c)** = both kits declare tags and values differ. Caller mints a `ChangedCallsite` with `c.dimension_name`, `c.source_compare_to`, `c.target_compare_to`, feeds it to `propagate_effects`, and emits a `LossRecordMemento` with `IrFormula::DivergenceBetween` between the two formulas.

## Why this shape

The prior return type `Result<Option<DivergenceCharacterization>, PlatformSemanticComparisonError>` conflated the two absent-cases:
- Source op-absent and target op-absent both became `Err(...)`.
- "Both absent" (substrate has no opinion) and "exactly one absent" (substrate refuses) were treated identically by the type system.

Under "Supra omnia, rectum" + the trichotomy (exact / loudly-bounded-lossy / refuse), those two absent-cases have DIFFERENT correct outcomes. NoOpinion is continue; Uncharacterizable is refuse. Conflating them at the type layer made the trichotomy unenforceable at the type system level.

The four-state enum makes the four legs of the substrate's reasoning explicit and exhaustive. Every caller pattern-matches all four, no caller can "forget" to handle the refuse leg, and the type system enforces the trichotomy.

## Discipline

- Callers MUST exhaustively match all four variants. No `_ => ...` catchall for the absent cases.
- Uncharacterizable MUST route through `CompositionRefusalMemento` or `RefusalMemento`. Silent fall-through to widen or no-op is a Supra-omnia-rectum violation.
- NoOpinion MUST continue without action. Synthesizing a verdict from no-opinion is the substrate claiming what it cannot claim.
- Tests asserting on the comparison primitive MUST cover all four variants with three tests each (positive, discrimination, structural) per [[discrimination-tests-per-variant]].

## Future work

Any new substrate consumer of `compare_op_with` must follow the four-leg pattern. New verdict variants (if added) require ruling amendment, not silent extension.

## Cross-references

- PR #1155: substrate primitives
- PR #1201: production-wiring + heuristic removal
- PR #1204: refuse-leg P0 fix (consumes uncharacterizable_callsites)
- [[2026-05-18-refuse-leg-short-circuit-ruling]]
- [[2026-05-18-platform-semantics-binding-kit-compose-ruling]]
- Existing ruling: `docs/plans/2026-05-16-platform-semantics-via-loss-records.md`
