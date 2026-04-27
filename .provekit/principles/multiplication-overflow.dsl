// Tightened (2026-04-27, #115 step 2.5): added is_in_dirty_set so the principle
// only fires on multiplications the fix actually touched. The original FIXME on
// value_comparison capability stands — without that we can't suppress on bound
// guards — but the dirty-set filter alone removes the most acute false-positive
// class (matching stable `* indentSize` in unrelated code at the locus).
//
// FIXME(capability-gap): no guard suppression possible without value_comparison
// capability. See capability-gaps.md.

predicate any_mul($x: node) {
  match $a: node where arithmetic.op == "*"
}

principle multiplication-overflow {
  match $mul: node where arithmetic.op == "*"
  require $w: any_mul($mul)
    where is_in_dirty_set($mul)
  report violation {
    at $mul
    captures { multiplication: $mul }
    message "multiplication result may exceed MAX_SAFE_INTEGER"
  }
}
