// A8: DSL translation of split-empty-string.json
// Match: any call to String.split() method.
// Over-matches: no requiresParamRef equivalent without data_flow_reaches.
// The original principle detects calls where the receiver could be an empty string;
// that requires value-range analysis (capability-gap).
//
// FIXME(capability-gap): no requiresParamRef equivalent — over-matches non-param cases.
// Empty-string receiver detection needs value_range capability. See capability-gaps.md.

principle split-empty-string {
  match $call: node where calls.callee_name == "split"
  report violation {
    at $call
    captures { call: $call }
    message "String.split() on value that may be empty yields [''] not []"
  }
}
