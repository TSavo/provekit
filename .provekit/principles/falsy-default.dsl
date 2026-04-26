// Tightened (2026-04-26): only fire when the LHS of `||` derives from a function
// parameter via data flow. Implements the original spec's `requiresParamRef: true`
// constraint, now expressible thanks to:
//   - DSL parser supporting positive `require $witness: pred(...) where rel(...)`
//   - new `flows_from_param` relation (joins data_flow_transitive with node_binding
//     where binding_kind = 'param')
//
// The witness predicate `any_param_decl` produces any param-binding decl in the
// file as the existential witness — the binding is required by the DSL grammar
// but the semantic constraint is in the where-relation. flows_from_param itself
// checks whether any param-bound source reaches operand_node transitively.
//
// Result: the principle no longer fires on `(literal || literal)` or
// `(member_access_with_no_param_origin || default)` — it requires the LHS to be
// user-derivable, matching the bug class definition.

predicate any_param_decl($x: node) {
  match $b: node where binding.binding_kind == "param"
}

principle falsy-default {
  match $node: node where truthiness.coercion_kind == "falsy_default"
  require $witness: any_param_decl($node)
    where flows_from_param($node.truthiness.operand_node)
  report violation {
    at $node
    captures { node: $node }
    message "|| used as default may silently discard valid falsy values (0, '', false)"
  }
}
