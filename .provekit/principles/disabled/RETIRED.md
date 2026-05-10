# Disabled / retired principles

This directory holds principles that are **not loaded by the runtime
matcher**. They remain on disk for audit history.

The active runtime path is the DSL evaluator (`src/dsl/evaluator.ts`),
which only iterates `*.dsl` files in `.provekit/principles/`. JSON-only
principles never fire even when present in the active dir, but they are
still loaded by the legacy `PrincipleStore` for hashing and provenance
purposes: moving them here removes them from that path too.

## Retirement reasons (task #129, 2026-04-27)

The original library (24 JSON, 16 DSL) was assembled before the spec
crystallized "the validator IS the definition" (`docs/specs/2026-04-27-constraint-driven-development.md`).
Several entries were heuristics or DSL-pending JSON spec stubs that
wouldn't pass strict adversarial validation today. Per the categorical
reduction in CDD: principles + invariants only, no heuristics. Heuristics
are biome / eslint's surface, not ours.

| principle | reason |
|---|---|
| `constructor-io-unguarded` | Heuristic. Description ("unconditional expression statements with no try/catch") is shape-matching, not a universal axiom. |
| `empty-catch` | Heuristic. Empty catch is sometimes intentional (best-effort cleanup). Not universally a defect; biome / eslint cover this surface. |
| `guard-narrowing` | DSL-pending stub. No matching mechanism shipped. |
| `multiplication-overflow` | DSL was already disabled in this dir (capability-gap on value comparisons); JSON moved alongside to finish the move. |
| `param-mutation` | Heuristic. Mutating parameters is an idiom in some codebases; not universally a defect. |
| `shell-injection` | Domain-specific (not a universal arithmetic/null/lifetime axiom). DSL-pending; revisit when capability for taint tracking lands. |
| `switch-no-default` | Heuristic. Many switches are intentionally exhaustive over a closed enum and need no default. |
| `ternary-branch-collapse` | Heuristic. Same-branch ternary often arises legitimately during refactor mid-flight. |
| `while-loop-termination` | DSL-pending. Termination is undecidable in general; principle would over-match. |

For per-entry timestamped retirement records see the corresponding
`<name>.retired.json` sidecars.
