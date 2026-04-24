// A8: DSL translation of division-by-zero.json
// Match: any arithmetic division node.
// Guard suppression: requires narrows row whose target_node equals the division's
// rhs_node with narrowing_kind == "literal_eq". This only fires when the guard check
// is on the same syntactic occurrence as rhs_node — intra-function tracking would
// need data_flow_reaches (capability-gap). Over-matches guarded sites at present.
//
// FIXME(capability-gap): guard suppression currently non-functional. The same_value
// relation (A8b) is registered and the parser now accepts any relation name (not just
// "before" | "dominates"). However the requireClause grammar only allows whole-node
// variable args: "pred($arg) RELATION $targetVar". The needed form puts same_value
// inside a predicate where-clause atom — not yet supported. A grammar extension for
// relation atoms in where predicates is required. See capability-gaps.md.

predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) before $div
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
