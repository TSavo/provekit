// A8: DSL translation of falsy-default.json
// Match: truthiness node with coercion_kind == "falsy_default".
// The truthiness capability's "falsy_default" kind covers || used as a default
// (where falsy values 0, "", false are silently replaced).
//
// NOTE: The truthiness extractor must populate rows with coercion_kind == "falsy_default"
// for binary || expressions. Verify against the truthiness extractor if this produces
// no matches. The JSON's original pattern was binary_expression with operator "||".
//
// Over-approximation: no requiresParamRef equivalent — flags all || defaults, not just
// those where the left operand derives from a parameter.
//
// FIXME(capability-gap): no requiresParamRef equivalent. See capability-gaps.md.

principle falsy-default {
  match $node: node where truthiness.coercion_kind == "falsy_default"
  report violation {
    at $node
    captures { node: $node }
    message "|| used as default may silently discard valid falsy values (0, '', false)"
  }
}
