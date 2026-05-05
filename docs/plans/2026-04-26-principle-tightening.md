# Principle Library Tightening

**Goal:** add `require no` discrimination to the existing 6 broad principles so recognition fires only when the bug shape is actually present, not just when the syntactic shape is.

This is the precondition that makes Phase 5 (continuous customer-fix-loop harvest) trustworthy. Today's library produces noisy provenance (161 falsy-default hits across the BugsJS corpus, many of which are not actually falsy-default bugs). With Phase 5 wired, that noise compounds into the customer's local library every time they close a bug.

This is purely a library-quality workstream. No harvest infrastructure changes; no new capabilities; pure DSL refinement against the existing capability set.

## Scope: 6 principles, in priority order by current hit count

### 1. `falsy-default`: 161 hits (top of the noise list)

Current DSL:
```dsl
principle falsy-default {
  match $node: node where truthiness.coercion_kind == "falsy_default"
  report violation { at $node ... }
}
```

The `truthiness` capability already discriminates `||`-as-default from `||`-as-conditional. But it fires whether or not the left operand could legitimately be 0 / "" / false at runtime. The bug only exists when the left side carries a value where falsy is meaningful (e.g. a port, a count, a flag).

Tightening direction:
- Add `require $param_ref: node where data_flow.from_node == $node.truthiness.lhs` constraining the LHS to flow from a parameter or external input. When the LHS is a literal expression with no externally-derived flow, it's not a falsy-default bug.

This matches the original JSON's `requiresParamRef: true` field that the DSL translation dropped due to a capability gap (the FIXME comment in the file).

### 2. `addition-overflow`: 88 hits

Current DSL: matches any `+`. Tightening:
- `require no $guard: node where narrows.narrowing_kind == "literal_lt"` near the addition site (any prior comparison against a literal, such as `< MAX`, `<= 1000`, etc.)
- Or constrain to additions whose RHS flows from a parameter (data_flow check, same shape as falsy-default)

Realistic outcome: from 88 hits down to maybe 15-25.

### 3. `subtraction-underflow`: 51 hits

Same shape as addition-overflow. Match `-`, require no guard against zero or against the LHS bound. Tightening fix mirrors #2.

### 4. `multiplication-overflow`: 11 hits

Same family as #2-3. Probably the same `require no narrows` shape.

### 5. `throw-uncaught`: 11 hits

Current DSL: `throws.is_inside_handler == false`. The FIXME comment notes an extractor gap: throws inside the try-block (not the catch-block) report `is_inside_handler == false`, so the principle over-matches throws that ARE going to be caught by the surrounding try.

This needs an extractor change, not a DSL change: add `throws.is_inside_try` column. Then tighten DSL with `and throws.is_inside_try == false`. **Capability work, not principle work.** Track separately.

### 6. `empty-collection-loop`: 3 hits

Already tight (constrains to `loop_kind == "for_of"` only). Low-priority.

## Validation strategy

For each tightened principle, re-run `scripts/harvest-recognize-report.ts` and compare hit counts before/after. The expected pattern:
- Before tightening: high hit count, mix of real and shape-only matches
- After tightening: lower hit count, but the matches that remain are honest

Also: the candidates that LOSE coverage from the tightened principle become eligible for Phase 2-B discovery; they need new principles distilled. The hit-count drop becomes Phase 2-B's growth budget.

If tightening a principle drops it from 161 hits to 30, that's 131 candidates that should produce new principles via discovery, each one a new bug class for the library.

## Acceptance

A principle is "tightened" when:
- Its DSL adds at least one `require [no]` clause OR a data-flow constraint
- Recognition hit count drops by ≥30% vs the broad version on the BugsJS corpus
- Manual inspection of 5 random remaining matches confirms they all look like genuine bug-shape matches (not just syntactic look-alikes)

## Out of scope (for this task)

- New capability tables. The existing capabilities (`arithmetic`, `truthiness`, `narrows`, `data_flow`, etc.) should be enough for ≥4 of the 6 principles. `throw-uncaught` and any extractor-gap entries are tracked as separate items.
- Principle library expansion via Phase 2-B. That's #97 calibration work.
- Phase 5 continuous wiring. That depends on this task closing.

## Estimated effort

~half a day per principle of focused work: read the broad principle, write the tightened version, run recognize-report, compare, manually inspect, iterate. 4 principles in scope (1, 2, 3, 4) → ~2 days total.

The `same_value` cross-relation that worked for `division-by-zero` (the model for tightness) is the pattern. Each tightening will look structurally similar.
