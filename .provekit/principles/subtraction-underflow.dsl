// Tightened (2026-04-26): suppress when the LHS variable has a prior lower-bound check.
// The bug shape is "subtraction whose LHS could underflow below zero/min". A guard like
// `if (x > 0) ...` or `if (x >= MIN) ...` asserts the lower bound. The narrows extractor
// emits literal_gt and literal_gte for those comparisons.
//
// We use literal_gt (matches `x > N` and `N < x` mirrored) on the LHS variable to
// suppress. literal_gte would also be a valid guard but gives narrower coverage — pick
// literal_gt as the dominant idiom; revisit if false-positive count remains high.
//
// One-sided check (LHS only) — same DSL limit as addition-overflow.
//
// Tightened (2026-04-27, #115 step 2.5): added is_in_dirty_set guard so the
// principle only fires on subtractions actually changed by the fix, not stable
// arithmetic in the dirty-zone neighborhood (e.g. `commentGroup.length - 1`
// at the locus where the actual bug is somewhere else). Subtraction always
// produces a numeric result so no result_sort filter needed.

predicate has_lower_bound_gt($var: node) {
  match $g: node where narrows.narrowing_kind == "literal_gt"
}

predicate any_sub($x: node) {
  match $a: node where arithmetic.op == "-"
}

principle subtraction-underflow {
  match $sub: node where arithmetic.op == "-"
  require $w: any_sub($sub)
    where is_in_dirty_set($sub)
  require no $guard: has_lower_bound_gt($sub)
    where same_value($guard.narrows.target_node, $sub.arithmetic.lhs_node)
  report violation {
    at $sub
    captures { subtraction: $sub }
    message "subtraction result may underflow below zero or minimum safe integer"
  }
}
