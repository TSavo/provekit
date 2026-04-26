// Tightened (2026-04-26): suppress when the LHS variable has a prior literal_lt guard.
// The narrows extractor now emits literal_lt for `x < N` (and literal_gt/lte/gte). We
// use that to detect explicit upper-bound checks. If `lhs < some_literal` exists
// anywhere with `lhs` matching the addition's lhs_node via same_value, the principle
// is suppressed — the programmer asserted a bound.
//
// One-sided: only checks LHS. RHS-bounded cases (e.g. `if (n < MAX) a + n`) still
// fire and contribute residual noise. The DSL's single require-clause limit blocks
// the natural extension; consider extending the parser to accept multiple require
// clauses or supporting OR-semantics in same_value to cover both sides.

predicate has_upper_bound_lt($var: node) {
  match $g: node where narrows.narrowing_kind == "literal_lt"
}

principle addition-overflow {
  match $add: node where arithmetic.op == "+"
  require no $guard: has_upper_bound_lt($add)
    where same_value($guard.narrows.target_node, $add.arithmetic.lhs_node)
  report violation {
    at $add
    captures { addition: $add }
    message "addition result may overflow safe integer range"
  }
}
