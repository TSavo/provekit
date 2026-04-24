// A8: DSL translation of match-null-result.json
// Match: any call to String.match() method.
// Over-matches: no requiresParamRef equivalent without data_flow_reaches.
// Guard suppression via null check: non-functional pending semantic variable tracking.
//
// FIXME(capability-gap): no requiresParamRef equivalent — over-matches non-param cases.
// Guard suppression non-functional. See capability-gaps.md.

principle match-null-result {
  match $call: node where calls.callee_name == "match"
  report violation {
    at $call
    captures { call: $call }
    message "String.match() result used without null check"
  }
}
