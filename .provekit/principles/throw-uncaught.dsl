// A8: DSL translation of throw-uncaught.json
// Match: throw statement that is NOT inside a catch handler.
// throws.is_inside_handler == false is CLOSE to the JSON's intent but not identical.
// The extractor sets is_inside_handler=true only for throws inside a CatchClause (re-throws).
// Throws inside a try block (not the catch) still have is_inside_handler=false —
// so this DSL over-matches: it flags throws that ARE in a try block even though
// they would be caught by that block's handler.
//
// FIXME(extractor gap): The throws extractor should set is_inside_try=true for throws
// inside a TryStatement's try block. Then this DSL would need:
//   throws.is_inside_handler == false and throws.is_inside_try == false
// OR a separate is_in_try_block column. See capability-gaps.md.

principle throw-uncaught {
  match $throw: node where throws.is_inside_handler == false
  report violation {
    at $throw
    captures { throw: $throw }
    message "throw statement outside try/catch creates caller obligation to handle exception"
  }
}
