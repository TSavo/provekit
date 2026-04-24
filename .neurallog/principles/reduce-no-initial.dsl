// A8: DSL translation of reduce-no-initial.json
// Match: call to Array.reduce() with exactly 1 argument (no initial value).
// calls.arg_count == 1 is the direct encoding of "no initial value argument".
// This is a COMPLETE translation — no guard suppression needed because the check
// is structural (arg count), not semantic.
//
// NOTE: arg_count == 1 means only the callback is passed; arg_count >= 2 means
// the initial value is also passed (guarded case). This is accurate.

principle reduce-no-initial {
  match $call: node where calls.callee_name == "reduce" and calls.arg_count == 1
  report violation {
    at $call
    captures { call: $call }
    message "Array.reduce() called without initial value; throws TypeError on empty array"
  }
}
