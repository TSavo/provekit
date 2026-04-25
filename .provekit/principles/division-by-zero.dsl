// A8: DSL translation of division-by-zero.json
// Match: any arithmetic division node.
// Guard suppression: requires a narrows row with narrowing_kind == "literal_eq" whose
// target_node shares the same data_flow declaration as the division's rhs_node.
// Uses same_value relation to check that the guard checks the same variable as the
// division denominator (not just positional/syntactic equality).
//
// NOTE(capability-gap): same_value checks data_flow.from_node equality between the
// guard's target_node and rhs_node. The relation LHS is the first inlined predicate
// clause's node alias (the narrows row itself), not narrows.target_node. End-to-end
// suppression of guarded sites still requires relation atoms inside match where-clauses
// or LHS targeting of capability columns (not just whole-node aliases). The skipped
// equivalence test in sameValueRelation.test.ts documents the remaining gap.

predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) same_value $div.arithmetic.rhs_node
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
