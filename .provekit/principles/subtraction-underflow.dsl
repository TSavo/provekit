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

predicate has_lower_bound_gt($var: node) {
  match $g: node where narrows.narrowing_kind == "literal_gt"
}

principle subtraction-underflow {
  match $sub: node where arithmetic.op == "-"
  require no $guard: has_lower_bound_gt($sub)
    where same_value($guard.narrows.target_node, $sub.arithmetic.lhs_node)
  report violation {
    at $sub
    captures { subtraction: $sub }
    message "subtraction result may underflow below zero or minimum safe integer"
  }
}
