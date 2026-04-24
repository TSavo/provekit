// A8: DSL translation of modulo-by-zero.json
// Match: any arithmetic modulo node.
// Guard suppression: same pattern as division-by-zero — requires a narrows row
// on the rhs_node with narrowing_kind == "literal_eq" (e.g. x !== 0 check).
//
// FIXME(capability-gap): guard suppression currently non-functional (narrows tracks
// syntactic occurrence, not semantic variable). See capability-gaps.md.

predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle modulo-by-zero {
  match $mod: node where arithmetic.op == "%"
  require no $guard: zero_guard($mod.arithmetic.rhs_node) before $mod
  report violation {
    at $mod
    captures { modulo: $mod }
    message "modulo divisor may be zero"
  }
}
