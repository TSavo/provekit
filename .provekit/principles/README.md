# ProvekIt principle library

This directory holds the **starter principle library** — universally-true
axioms that ProvekIt's runtime matcher fires against every commit.

The runtime path is the DSL evaluator (`src/dsl/evaluator.ts`). Only
`*.dsl` files in this directory load. JSON files are spec-only and do
not fire.

## Curation rules

1. **Universally-true axioms only.** Every entry MUST be a tautology in
   any TypeScript / JavaScript codebase. If there's a legitimate idiom
   that triggers the principle, the principle is a heuristic and does
   not belong here. Heuristics are biome / eslint's surface; we don't
   compete.
2. **The validator IS the definition.** Per CDD
   (`docs/specs/2026-04-27-constraint-driven-development.md`,
   "Categorical reduction"): principles + invariants, no heuristics. A
   principle's authority is its discrimination evidence, not the prose
   wrapped around the SMT.
3. **Every active entry needs a fixture pair.** A buggy fixture that
   fires the principle and a clean fixture that does not. The pair lives
   in `scripts/validate-library.ts` and is run via `npx tsx
   scripts/validate-library.ts`. New principles are added to the script
   before they're added to the library.
4. **Growth is rare, by federation-promotion.** Principles graduate via
   the harvest pipeline (cross-corpus validation, `confidenceTier`
   progression in the `principles_library` DB table). Hand-written
   additions are exceptional.

## Starter axiom set (task #129)

| # | axiom | shipped? |
|---|---|---|
| 1 | division-by-zero | yes (`division-by-zero.{json,dsl}`) |
| 2 | modulo-by-zero | yes (`modulo-by-zero.{json,dsl}`) |
| 3 | NaN equality / inequality | NOT YET — DSL-pending. The arithmetic capability lacks a literal-NaN comparison column. Tracked as capability gap. |
| 4 | null/undefined dereference on possibly-null reference | partial: `null-assertion.{json,dsl}` covers the TypeScript `!` operator surface. Generalised member-access-on-possibly-null is DSL-pending. |
| 5 | unhandled promise rejection | partial: `unguarded-await.{json,dsl}` over-matches (capability gap: `yields` lacks `is_inside_handler`). |
| 6 | array index out of bounds on read | NOT YET — DSL-pending. Needs a numeric-bound capability the SAST does not yet emit. |
| 7 | use-after-close on a resource | NOT YET — DSL-pending. Requires a value-state lifecycle capability. |

The 4 axioms not shipped in this curation pass remain on the roadmap.
They get added when the underlying SAST capability is in place AND a
fixture pair exists. Until then, NOT shipping is the correct answer:
shipping a principle without a discriminating DSL would violate rule 1.

## Validation evidence (task #129, 2026-04-27)

Output of `npx tsx scripts/validate-library.ts`:

```
name                       pos  neg  status                detail
-----------------------------------------------------------------
addition-overflow            0    0  DIFF-CONTEXT-ONLY
division-by-zero             1    0  PASS
empty-collection-loop        1    0  PASS
falsy-default                0    0  DIFF-CONTEXT-ONLY
find-undefined-result        1    0  PASS
loop-accumulator-overflow    1    0  PASS
match-null-result            1    0  PASS
modulo-by-zero               1    0  PASS
null-assertion               1    0  PASS
or-chain-extended-by-fix     0    0  DIFF-CONTEXT-ONLY
reduce-no-initial            1    0  PASS
split-empty-string           1    0  PASS
subtraction-underflow        0    0  DIFF-CONTEXT-ONLY
throw-uncaught               1    0  PASS
unguarded-await              1    1  KNOWN-CAPABILITY-GAP
variable-staleness           1    0  PASS

summary: 11 pass, 4 diff-context-only, 1 known-capability-gap, 0 skipped, 0 failed
```

Status legend:
- **PASS** — fires on the buggy fixture, does NOT fire on the clean
  fixture. Strict adversarial validation.
- **DIFF-CONTEXT-ONLY** — DSL gates on `is_in_dirty_set` /
  `was_replaced_by_addition`. These principles only fire during corpus
  mining where pre/post diff context is active. Static fixture pairs
  cannot exercise them by design; their adversarial story is
  corpus-precision measurement (`scripts/run-principles-corpus.ts`),
  not the fixture pair.
- **KNOWN-CAPABILITY-GAP** — over-matches owing to a documented gap in
  the SAST extractor. Acknowledged in the DSL file's own header.
  Not strictly adversarially valid today; kept because the locus is
  still useful signal.
- **FAIL-NO-FIRE / FAIL-FALSE-POSITIVE / ERROR** — would force a
  retirement to `disabled/`. Currently zero entries.

## Retired entries

See `disabled/RETIRED.md`. The 9 JSON-only principles moved there in
task #129 were heuristics, DSL-pending stubs, or capability-gap stuck
entries that did not match the curation rules above. Each has a
`<name>.retired.json` sidecar capturing reason + timestamp.

## Heuristic tier

The original task brief asked to "drop the heuristic tier code path."
Audit found **no heuristic tier exists in the codebase**. The closest
mechanism is `principlesLibrary.confidenceTier` in
`src/db/schema/principlesLibrary.ts` — values
`"advisory" | "warning" | "blocking"` — which is a trust-progression
ladder for harvest-mined principles, not a heuristic-vs-axiom
distinction. It was left intact.

The prose word "heuristic" appears 6 times in `src/`, all as
explanatory comments in unrelated code (e.g.
`invariantKind.ts` says it prefers an LLM-emitted kind over
"keyword/regex heuristics"). None are tier discriminators.

## Re-running validation

```sh
cd /path/to/provekit
npx tsx scripts/validate-library.ts
```

Exit code is 0 only if no entry failed strict adversarial validation
(FAIL-NO-FIRE / FAIL-FALSE-POSITIVE / ERROR all yield exit 1).
DIFF-CONTEXT-ONLY and KNOWN-CAPABILITY-GAP are accepted statuses.
