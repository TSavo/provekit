// A8: DSL translation of subtraction-underflow.json
// Match: any arithmetic subtraction node.
// Over-approximation: no guard suppression possible without value_comparison capability.
//
// FIXME(capability-gap): no guard suppression possible without value_comparison capability.
// See capability-gaps.md.

principle subtraction-underflow {
  match $sub: node where arithmetic.op == "-"
  report violation {
    at $sub
    captures { subtraction: $sub }
    message "subtraction result may underflow below zero or minimum safe integer"
  }
}
