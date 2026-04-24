// A8: DSL translation of null-assertion.json
// Match: any non-null assertion node (TypeScript ! operator).
// The non_null_assertion capability marks nodes that ARE the ! operator.
// The tautological where clause (operand_node == $assert.non_null_assertion.operand_node)
// simply ensures the JOIN fires for any row in the non_null_assertion table.
//
// Guard suppression: requires a narrows row with null_check kind on the operand
// before the assertion. Non-functional pending data-flow semantic tracking.
//
// FIXME(capability-gap): guard suppression currently non-functional (narrows tracks
// syntactic occurrence, not semantic variable). See capability-gaps.md.

predicate null_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "null_check"
}

principle null-assertion {
  match $assert: node where non_null_assertion.operand_node == $assert.non_null_assertion.operand_node
  require no $guard: null_guard($assert.non_null_assertion.operand_node) before $assert
  report violation {
    at $assert
    captures { assertion: $assert }
    message "non-null assertion used without preceding null/undefined check"
  }
}
