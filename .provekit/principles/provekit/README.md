# ProvekIt-specific principle partition

Axioms about ProvekIt's own shape. These principles fire only when the
target codebase IS ProvekIt — they encode the 15 product constraints
from `docs/specs/2026-04-27-constraint-driven-development.md` § "Product
constraints" as enforceable axioms.

## One mechanism: tree walk + bindings + Z3

Tautologies (universal/typescript/cpp partitions) and observations
(`.provekit/observations/`) and these ProvekIt-specific axioms are all
verified the same way: walk the substrate's tree (AST + import graph +
corpus index), examine the bindings each candidate site introduces,
ask Z3 whether any path satisfies the violation predicate.

Divide-by-zero is "for any `/` node, no path lets the denominator's
binding equal zero." Constraint #1 ("no LLM in the verification path")
is "for `provekit verify`'s entry, no path of imports reaches a symbol
under `src/llm/`." Same structural shape, different bindings.

This partition does NOT introduce a separate corpus evaluator or
JSON-predicate runtime. Every axiom here is expressed in the same DSL
the language partitions use, possibly extended with new relations
(import-graph reachability, corpus-node lookup) when an axiom needs them.

## Files

(none yet — first axiom is #1 "no LLM in the verification path",
landing as a DSL principle once the import-graph relation is added)
