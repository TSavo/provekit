// A8: DSL translation of division-by-zero.json
// Match: any arithmetic division node.
// Guard suppression: requires a narrows row with narrowing_kind == "literal_eq" where
// the narrows.target_node shares the same data_flow.from_node as the division's rhs_node.
//
// Uses the NEW explicit relation-call syntax:
//   where same_value($guard.narrows.target_node, $div.arithmetic.rhs_node)
//
// LHS: the narrows row's target_node column — the node that the guard is checking.
// RHS: the arithmetic row's rhs_node column — the denominator of the division.
// same_value: holds iff both nodes share the same data_flow.from_node (same declared variable).
//
// The predicate zero_guard finds any narrows row with narrowing_kind == "literal_eq".
// The `where same_value(...)` clause then restricts to those where the guard checks
// the same variable as the denominator.

predicate zero_guard($div: node) {
  match $g: node where narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div)
    where same_value($guard.narrows.target_node, $div.arithmetic.rhs_node)
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
