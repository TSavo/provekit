// 2026-04-27: hard-bug 3 closure (v1, conservative shape).
//
// Variable Staleness on Fall-Through. Pattern:
//   let x = 0;
//   if (cond) { x = 1; }
//   use(x);
// When cond is false, use(x) sees the stale default. The classical fix is
// either an else-branch with the same assignment OR replacing the if with
// a conditional expression.
//
// V1 substrate signal: an if-statement with NO else-branch
// (decides.alternate_node IS NULL) that ENCLOSES an assignment. Severity
// = info because legitimate `if (cond) { x = ... }` patterns exist (e.g.,
// when x is used only inside the same block, or when the missing else is
// intentional).
//
// V2 (deferred): would require adding a relation that links the assignment's
// target variable to use-sites OUTSIDE the if-statement. The substrate has
// data_flow_transitive but it tracks variable-declarations to uses, not
// assignment-to-uses. A new same-variable-use relation (analogous to
// same_value but parameterized by scope) would tighten this.

predicate any_assignment($x: node) {
  match $a: node where assigns.assign_kind == "="
}

principle variable-staleness {
  match $if: node where decides.decision_kind == "if" and decides.alternate_node == null
  require $assn: any_assignment($if)
    where encloses($if, $assn)
  report info {
    at $if
    captures { ifBlock: $if }
    message "if-statement with no else branch contains an assignment; fall-through path may see the unmodified value"
  }
}
