# A8: Capability Gaps — Seed Principles DSL Migration

**Date:** 2026-04-23
**Task:** A8 — Migrate 23 seed principles to DSL
**Result:** 14 migrated, 9 capability-gap

---

## Summary

| Bucket | Count | Principles |
|--------|-------|-----------|
| Migrated (match-only, no guard suppression) | 8 | addition-overflow, subtraction-underflow, multiplication-overflow, find-undefined-result, match-null-result, split-empty-string, falsy-default, empty-collection-loop |
| Migrated (guard-aware / structurally complete) | 3 | reduce-no-initial, throw-uncaught, unguarded-await (partial — over-matches) |
| Migrated (guard predicate present, non-functional) | 3 | division-by-zero, modulo-by-zero, null-assertion |
| Capability-gap (not migrated) | 9 | shell-injection, empty-catch, guard-narrowing, loop-accumulator-overflow, param-mutation, switch-no-default, ternary-branch-collapse, variable-staleness, while-loop-termination |

---

## Cross-cutting gap: guard suppression via narrows

All principles that have `guardPatterns` in their JSON (division-by-zero, modulo-by-zero, null-assertion, find-undefined-result, match-null-result) need a `require no $guard: <predicate>(...) before $div` clause that fires when a semantic guard exists. The existing `narrows` extractor tracks the *syntactic occurrence* of the narrowed node, not the *semantic variable*. So when `b !== 0` narrows variable `b` at one occurrence and `a/b` uses `b` at a different occurrence, the `narrows.target_node` for the guard check doesn't match `arithmetic.rhs_node` for the division — they are different node IDs for the same logical variable.

**Proposed fix (substrate extension, tracked for C6):**
- Add a `data_flow_same_value(nodeA, nodeB)` relation: true when both nodes reference the same declaration with no intervening writes
- Or: extend the narrows extractor to emit rows for ALL uses of the narrowed variable, not just the check site

---

## shell-injection (not migrated)

**Why:** Principle matches `execSync`, `exec`, or `spawn` calls whose first argument is a tainted string (template literal or concatenation with a variable). Three issues:
1. The JSON has three separate `method` values — the DSL has no OR in where clauses, requiring three separate DSL principles.
2. Detecting "first argument contains interpolation" requires a `string_composition` capability.
3. Detecting "argument derives from parameter" requires `data_flow_reaches`.

**Would need:** `string_composition.has_interpolation` capability + `data_flow_reaches` relation.

**DSL sketch (when capabilities land):**
```dsl
principle shell-injection-exec {
  match
    $call: node where calls.callee_name == "execSync"
    $arg: node where string_composition.has_interpolation == true
  require data_flow_reaches($arg, $call)
  report violation {
    at $call
    captures { call: $call, tainted_arg: $arg }
    message "shell exec called with interpolated string; shell injection possible"
  }
}
```

**Proposed substrate extension (C6 → D1 path):**
- New capability: `string_composition` — `{node_id, kind: 'template' | 'concat' | 'literal', has_interpolation: bool}`
- New relation: `data_flow_reaches(a, b)` — true when value of `a` can flow to `b` via data_flow_transitive

---

## empty-catch (not migrated)

**Why:** The principle flags `try_statement` nodes whose `catch` handler has an empty body. Detecting "empty handler body" requires querying the number of child statements in the catch block — no current capability tracks this structural property.

**Would need:** A `try_catch_block` capability with `handler_stmt_count: Int` column, or a relation `empty_catch_body(tryNode)`.

**DSL sketch (when capability lands):**
```dsl
principle empty-catch {
  match $try: node where try_catch_block.handler_stmt_count == 0
  report violation {
    at $try
    captures { try: $try }
    message "catch block is empty; exceptions silently swallowed"
  }
}
```

**Proposed substrate extension:**
- New capability: `try_catch_block` — `{node_id, handler_stmt_count: Int, has_finally: Bool}`
- Extractor: check `catch_clause.body.namedChildren.length`

---

## guard-narrowing (not migrated)

**Why:** The principle flags `if_statement` nodes where the consequence contains a return/throw (early-exit guard) and no else branch. After the if, code implicitly assumes the guard's negation. Detecting "consequence always exits" requires a control-flow property — no current capability tracks this.

**Would need:** `always_exits` relation or `block_always_exits` capability column on `decides`.

**DSL sketch (when capability lands):**
```dsl
principle guard-narrowing {
  match $if: node where decides.decision_kind == "if" and decides.alternate_node == null
  require always_exits($if.decides.consequent_node)
  report warning {
    at $if
    captures { guard: $if }
    message "early-return guard; code after if assumes negation but does not re-check"
  }
}
```

**Proposed substrate extension:**
- New relation: `always_exits(blockNode)` — true when every control-flow path through the block ends in a return/throw/process.exit
- Alternatively: extend `decides` with `consequent_always_exits: Bool` column

---

## loop-accumulator-overflow (not migrated)

**Why:** The principle flags loops with augmented assignment (`+=`, `*=`) in the body. Detecting "augmented assignment inside loop body" requires containment: an `assigns` row inside the `iterates` row's body subtree. The current DSL has no `encloses` or `contains` relation.

**Would need:** `encloses($outer, $inner)` relation — true when `$outer` is an ancestor of `$inner` in the AST.

**DSL sketch (when relation lands):**
```dsl
principle loop-accumulator-overflow {
  match
    $loop: node where iterates.loop_kind == "for"
    $acc: node where assigns.assign_kind == "+="
  require encloses($loop, $acc)
  report violation {
    at $acc
    captures { loop: $loop, accumulator: $acc }
    message "augmented assignment inside loop; unbounded iteration may overflow accumulator"
  }
}
```

**Proposed substrate extension:**
- New relation: `encloses($outer, $inner)` — `EXISTS (SELECT 1 FROM dominance WHERE dominator = $outer.id AND dominated = $inner.id)` using the materialized closure

---

## param-mutation (not migrated)

**Why:** The principle flags `assignment_expression` where the LHS is a member expression on a parameter-derived object (`param.foo = ...`). Detecting "LHS is a member expression on a parameter" requires binding: knowing which variables are function parameters and whether the object of the member expression is bound to one of them.

**Would need:** `binding.binding_kind == "parameter"` lookup + data_flow to connect the assignment LHS's receiver to a parameter binding.

**DSL sketch (when relation lands):**
```dsl
predicate is_param($var: node) {
  match $b: node where binding.binding_kind == "parameter" and binding.node_id == $var
}

principle param-mutation {
  match $assign: node where assigns.assign_kind == "="
  require is_param($assign.assigns.target_node)
  report warning {
    at $assign
    captures { assign: $assign }
    message "property assignment on parameter object; mutation visible to caller"
  }
}
```

**Gap:** The `assigns.target_node` for `param.foo = x` points to the *member expression* node, not the *identifier* `param`. Connecting member expression receiver to a parameter declaration still needs `data_flow_reaches`.

---

## switch-no-default (not migrated)

**Why:** The principle flags `switch_statement` nodes that have NO `switch_default` child. Detecting *absence* of a child type is a structural query — the current capabilities don't expose switch-case structure. The `decides` capability has `decision_kind == "switch_case"` but no `has_default` boolean.

**Would need:** Extend `decides` with `has_default: Bool` for switch_case kind, or a new `switch_block` capability.

**DSL sketch (when capability lands):**
```dsl
principle switch-no-default {
  match $sw: node where decides.decision_kind == "switch_case" and decides.has_default == false
  report violation {
    at $sw
    captures { switch: $sw }
    message "switch statement missing default case; unmatched values fall through silently"
  }
}
```

**Proposed substrate extension:**
- Extend `node_decides` with `has_default Bool` column, populated for `switch_case` kind nodes

---

## ternary-branch-collapse (not migrated)

**Why:** The principle flags ternary expressions where both branches produce an empty or identity value (e.g., `""`, `0`). Detecting "branch value is empty or identity" requires literal-value analysis — knowing the return value of each branch. No current capability tracks the literal value produced by a node.

**Would need:** A `literal_value` capability: `{node_id, kind: 'string' | 'number' | 'bool', value: Text}`.

**DSL sketch (when capability lands):**
```dsl
predicate is_empty_string($var: node) {
  match $lit: node where literal_value.kind == "string" and literal_value.value == ""
                      and literal_value.node_id == $var
}

principle ternary-branch-collapse {
  match $tern: node where decides.decision_kind == "ternary"
  require is_empty_string($tern.decides.consequent_node)
  require is_empty_string($tern.decides.alternate_node)
  report warning {
    at $tern
    captures { ternary: $tern }
    message "both branches produce empty string; ternary collapses to semantic no-op"
  }
}
```

**Note:** The JSON's confidence is "low" — this principle is the most over-specified of the 23 and may need refinement before the DSL version is enabled in production.

---

## variable-staleness (not migrated)

**Why:** The principle flags `if_statement` where the body modifies a variable that is read after the if (on the fall-through path). Detecting "variable modified inside if-block and read after" requires intra-block data-flow: knowing which variables are assigned inside a consequence block and whether those same variables are read later in the enclosing scope. No current capability or relation provides this.

**Would need:** `data_flow_reaches` relation + `assigns` + `binding` working together to track if a write inside a block can affect a read outside it.

**Gap:** This is a full liveness/def-use analysis — non-trivial to express as a single DSL principle even with extended capabilities. May need a dedicated oracle.

---

## while-loop-termination (not migrated)

**Why:** The principle flags `while_statement` loops where the body does NOT modify any variable referenced in the loop condition. This is fundamentally a "body-does-not-modify-condition-vars" analysis, which requires:
1. Extracting which variables appear in `condition_node`
2. Checking that no `assigns` row in the body's subtree targets any of those variables

This requires an `encloses` relation (for containment check) AND a `same_variable` semantic join (to connect the identifier in the condition to the assignment target in the body).

**Would need:** `encloses` relation + `data_flow_same_value` relation (or variable identity tracking).

---

## Known over-approximations in migrated principles

All migrated principles (except `reduce-no-initial` and `throw-uncaught`) over-match relative to the original JSON because:

1. **No `requiresParamRef`:** The DSL has no way to express "only flag nodes where at least one operand derives from a function parameter." This was a key filter in the JSON's `requiresParamRef: true` field. Needs `data_flow_reaches` from any parameter binding.

2. **Guard suppression non-functional:** The `require no ... before` clauses exist in the DSL files for division-by-zero, modulo-by-zero, and null-assertion, but the narrows extractor tracks syntactic node IDs, not semantic variable identity. So `narrows.target_node` for a guard check and `arithmetic.rhs_node` for the operation being guarded will almost never match (different AST nodes for the same variable occurrence). Tracked in the evaluator test as a `.skip` test.

3. **`||` operator enum gap:** The `arithmetic.op` kindEnum does not include `||`. Falsy-default is translated via `truthiness.coercion_kind == "falsy_default"` — this is correct IF the truthiness extractor populates this value for `||` expressions used as defaults. Needs empirical verification against real fixtures.

4. **`empty-collection-loop` partial:** Only translates `for_of`; `for_in` requires OR in where clauses (capability-gap: DSL lacks OR).

---

## Implications for C6 capability-proposal prompt design

The gap patterns cluster into five themes:

1. **Containment / encloses:** needed by loop-accumulator-overflow, while-loop-termination, param-mutation. One `encloses(outer, inner)` relation (using existing dominance closure) unblocks all three.

2. **Data-flow variable identity:** needed by guard suppression, variable-staleness, shell-injection, param-mutation. A `data_flow_same_value(a, b)` relation or extending narrows to track all uses (not just check sites) would unblock the largest cluster.

3. **Structural absence:** needed by switch-no-default (no default child), empty-catch (empty handler body). Simple column additions to existing tables (`has_default`, `handler_stmt_count`).

4. **Literal value tracking:** needed by ternary-branch-collapse. A `literal_value` capability with `node_id + kind + value` rows.

5. **String composition / taint:** needed by shell-injection. A `string_composition` capability tracking template/concat structure + `data_flow_reaches` for taint propagation.

Priority recommendation for C6: implement **containment** and **structural absence** first — lowest complexity, unblock 5 principles. Data-flow variable identity second — high value but higher complexity.
