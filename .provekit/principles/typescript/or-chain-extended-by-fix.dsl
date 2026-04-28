// 2026-04-27: hard-bug 1 closure. Diff-aware principle (first of class).
//
// Targets the failed enum-disjunction cluster (39 candidates, baseline
// 1-2/10 precision when mined LLM-only without diff signal). The shape:
//
//   // pre
//   if (parent.type === "Foo" || parent.type === "Bar") { ... }
//
//   // post (the production fix)
//   if (parent.type === "Foo" || parent.type === "Bar" || parent.type === "Baz") { ... }
//
// Without the diff signal, an LLM-mined principle either fires on every
// OR-chain (precision dies on `a || b` defaults, condition checks,
// pretty much any short-circuit) or doesn't fire at all (no syntactic
// way to know "this chain is missing a clause"). The diff is the
// signal: the post-fix outer BinaryExpression is `added`, the pre-fix
// inner BinaryExpression survives unchanged inside it.
//
// `was_replaced_by_addition($or)` (src/dsl/relations.ts) packages this:
//   - $or pairs `unchanged` with a post node (its fingerprint preserved)
//   - some `added` post node strictly encloses that paired post node
//
// Because the relation requires a non-empty `diff_context_active` row,
// the principle is dormant outside diff-bearing contexts (mining,
// future lint --base). Static-only runs see no false positives from it.
//
// Witness predicate: `any_or_node` exists purely to satisfy the
// require-clause grammar; it produces any falsy_default node in the
// file (the same kind we're matching), guaranteeing a witness exists
// whenever the match did.

predicate any_or_node($x: node) {
  match $w: node where truthiness.coercion_kind == "falsy_default"
}

principle or-chain-extended-by-fix {
  match $or: node where truthiness.coercion_kind == "falsy_default"
  require $witness: any_or_node($or)
    where was_replaced_by_addition($or)
  report violation {
    at $or
    captures { orChain: $or }
    message "OR-chain was extended in the production fix; the matched version is missing a clause that the maintainer added"
  }
}
