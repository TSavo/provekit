// Tightened (2026-04-26): suppress when the LHS variable has a prior literal_lt guard.
// The narrows extractor now emits literal_lt for `x < N` (and literal_gt/lte/gte). We
// use that to detect explicit upper-bound checks. If `lhs < some_literal` exists
// anywhere with `lhs` matching the addition's lhs_node via same_value, the principle
// is suppressed: the programmer asserted a bound.
//
// Tightened (2026-04-27, #115 step 2.5):
//   - `arithmetic.result_sort != "String"`: extractor now infers result kind from
//     operand types (post-order). Numeric `+` keeps "Numeric"; string concat is
//     "String" via contagion (any string operand → string result). 5 manual-30
//     gate disagrees were string-concat misfires (regex assembly, fixer text
//     assembly, etc.); this filter eliminates them.
//   - `is_in_dirty_set($add)`: a 1-arg substrate relation that requires the
//     matched node to actually live in the diff (change_kind != 'unchanged' for
//     this exact pre coordinates). Without this, principles match on stable code
//     near the actual fix and report violations the developer didn't introduce.
//     Dormant when no active diff context: static-only runs unaffected.

predicate has_upper_bound_lt($var: node) {
  match $g: node where narrows.narrowing_kind == "literal_lt"
}

predicate any_arith($x: node) {
  match $a: node where arithmetic.op == "+"
}

principle addition-overflow {
  match $add: node where arithmetic.op == "+" and arithmetic.result_sort != "String"
  require $w: any_arith($add)
    where is_in_dirty_set($add)
  require no $guard: has_upper_bound_lt($add)
    where same_value($guard.narrows.target_node, $add.arithmetic.lhs_node)
  report violation {
    at $add
    captures { addition: $add }
    message "addition result may overflow safe integer range"
  }
}
