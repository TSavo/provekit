// A8: DSL translation of addition-overflow.json
// Match: any arithmetic addition node.
// Over-approximation: the original JSON's guardPatterns ("< MAX", "Number.MAX_SAFE_INTEGER")
// cannot be expressed in the current DSL — no capability tracks numeric comparison values
// or identifier names like "MAX_SAFE_INTEGER". Matching arithmetic.op == "+" is correct
// as a signal; guard suppression needs a value_comparison capability (capability-gap).
//
// FIXME(capability-gap): no guard suppression possible without value_comparison capability.
// See capability-gaps.md.

principle addition-overflow {
  match $add: node where arithmetic.op == "+"
  report violation {
    at $add
    captures { addition: $add }
    message "addition result may overflow safe integer range"
  }
}
