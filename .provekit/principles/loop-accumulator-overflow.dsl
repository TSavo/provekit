// 2026-04-26: hard-bug 2 closure. Augmented-assignment (+=) inside a loop
// body — unbounded iteration count can overflow the accumulator past
// Number.MAX_SAFE_INTEGER. The original JSON spec described the shape but
// no DSL existed (capability gap: AST containment relation).
//
// Substrate addition (this commit): `encloses($outer, $inner)` relation,
// implemented via source-range nesting (no recursive closure required —
// ts-morph guarantees properly nested ranges).
//
// Match shape: an assigns row with assign_kind = "+=" whose source range
// is enclosed by some iterates row's body_node range.
//
// Per the principle JSON, *= is also a candidate but the DSL only supports
// one match-clause kind per principle (atom-pred is a single cap.col cmp).
// First version covers += in for-loops; future principle files cover *= and
// while/for-of variants.

predicate is_for_loop($x: node) {
  match $loop: node where iterates.loop_kind == "for"
}

principle loop-accumulator-overflow {
  match $assn: node where assigns.assign_kind == "+="
  require $loop: is_for_loop($assn)
    where encloses($loop.iterates.body_node, $assn)
  report violation {
    at $assn
    captures { accumulator: $assn }
    message "+= inside loop body — unbounded iteration count can overflow Number.MAX_SAFE_INTEGER"
  }
}
