// 2026-04-27: hard-bug 3 closure. Variable Staleness on Fall-Through.
//
//   let x = 0;
//   if (cond) { x = 1; }
//   use(x);
//
// When cond is false, use(x) sees the stale default. Fix is an else-branch
// with the same assignment, or replacing the if with a conditional
// expression.
//
// Substrate machinery (this commit's full version):
//   stale_assignment($if, $assn): true iff:
//     (1) $assn is structurally inside $if's `decides.consequent_node`
//         (source-range nesting), AND
//     (2) the variable that $assn writes (assigns.target_node) has at
//         least one OTHER use-site (sharing the same data_flow.from_node
//         declaration) whose source range is NOT enclosed by $if. The
//         "other use outside the if" is the staleness condition.
//
// Severity = violation (the data-flow check restricts firings to cases
// with a real fall-through reach, not just any if-with-assignment).

// 2026-04-27 #115 step 2 round-2 tightening: 6 of 7 round-2 disagreements
// are if-statements WITH an else branch (so fall-through never happens) or
// patterns that aren't fall-through staleness at all (null guards,
// missing-case widening). Requiring `decides.alternate_node == null` makes
// the principle only fire on bare `if (cond) { x = ... }` shapes where
// fall-through is actually reachable. That's the bug-class definition.

predicate any_assignment($x: node) {
  match $a: node where assigns.assign_kind == "="
}

principle variable-staleness {
  match $if: node where decides.decision_kind == "if" and decides.alternate_node == null
  require $assn: any_assignment($if)
    where stale_assignment($if, $assn)
  report violation {
    at $if
    captures { ifBlock: $if }
    message "assignment inside if-block writes a variable that is also used outside the if; fall-through path sees the unmodified value"
  }
}
