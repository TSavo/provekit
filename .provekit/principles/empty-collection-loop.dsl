// A8: DSL translation of empty-collection-loop.json (partial — for_of only)
// Match: for-of loop (iterates.loop_kind == "for_of").
// The original JSON's nodeType was "for_in_statement" which covers both for-of and
// for-in in tree-sitter (the JSON name is misleading — tree-sitter uses for_in_statement
// for both). The iterates capability uses "for_of" and "for_in" as separate enum values.
//
// DSL limitation: no OR operator — cannot match both "for_of" and "for_in" in one principle.
// Only for_of is translated here; for_in is a capability-gap (needs OR predicate).
//
// FIXME(capability-gap): DSL lacks OR in where clause; cannot match loop_kind == "for_of"
// OR loop_kind == "for_in" in a single principle. See capability-gaps.md.
//
// Over-approximation: the original principle only flags cases where the collection could
// be empty. That requires value-range analysis (capability-gap).

principle empty-collection-loop {
  match $loop: node where iterates.loop_kind == "for_of"
  report violation {
    at $loop
    captures { loop: $loop }
    message "for-of loop over collection that may be empty; loop body never executes"
  }
}
