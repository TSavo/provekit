// A8: DSL translation of multiplication-overflow.json
// Match: any arithmetic multiplication node.
// Over-approximation: no guard suppression possible without value_comparison capability.
//
// FIXME(capability-gap): no guard suppression possible without value_comparison capability.
// See capability-gaps.md.

principle multiplication-overflow {
  match $mul: node where arithmetic.op == "*"
  report violation {
    at $mul
    captures { multiplication: $mul }
    message "multiplication result may exceed MAX_SAFE_INTEGER"
  }
}
