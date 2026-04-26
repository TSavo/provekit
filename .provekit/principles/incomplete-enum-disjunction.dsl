// 2026-04-26: mined from BugsJS corrective sample (eslint-41, eslint-211,
// eslint-266, eslint-43). Pattern:
//   parent.type === "Foo" || parent.type === "Bar"
// where the chain is incomplete and the fix adds a third clause for a sibling
// AST type (e.g. ArrowFunctionExpression alongside FunctionExpression).
//
// Substrate signal: at least two DISTINCT literal_eq narrowings exist whose
// target_node columns share data flow (same declared variable accessed via
// the same property). The new DSL `!=` operator (added 2026-04-26) lets us
// express the distinct-from-self constraint that prevents trivial self-match.
//
// Severity = info: this is advisory — most paired literal_eq narrowings on
// the same variable are intentional exhaustive checks. The principle flags
// for review, not as a hard violation.

predicate other_literal_eq($g: node) {
  match $other: node where narrows.narrowing_kind == "literal_eq" and narrows.node_id != $g
}

principle incomplete-enum-disjunction {
  match $g: node where narrows.narrowing_kind == "literal_eq"
  require $other: other_literal_eq($g)
    where same_value($other.narrows.target_node, $g.narrows.target_node)
  report info {
    at $g
    captures { narrowing: $g }
    message "literal_eq narrowing whose target shares data flow with a distinct literal_eq — review enum completeness"
  }
}
