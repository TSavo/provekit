// A8: DSL translation of find-undefined-result.json
// Match: any call whose callee_name ends in ".find" or equals "find".
//
// EXTRACTOR NOTE: The calls extractor stores callee_name as the full PropertyAccessExpression
// text (e.g., "arr.find" not just "find"). A future method_name capability that extracts
// just the method name would be cleaner. For now, DSL users must match the full callee text
// or use a method_name relation (capability-gap).
//
// The original JSON's requiresParamRef (only flag when arg derives from a parameter)
// is not expressible in the DSL without a data_flow_reaches relation (capability-gap).
// Guard suppression via undefined check would require a narrows predicate; non-functional
// pending semantic variable tracking.
//
// FIXME(capability-gap): no requiresParamRef equivalent — over-matches non-param cases.
// Guard suppression non-functional. See capability-gaps.md.
// FIXME(extractor gap): callee_name is full expr text, not just method name.

principle find-undefined-result {
  match $call: node where calls.callee_name == "find"
  report violation {
    at $call
    captures { call: $call }
    message "Array.find() result used without undefined check"
  }
}
