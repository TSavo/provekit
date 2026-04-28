# ProvekIt principle library

This directory holds the **starter principle library** — universally-true
axioms that ProvekIt's runtime matcher fires against every commit.

The runtime path is the DSL evaluator (`src/dsl/evaluator.ts`). Only
`*.dsl` files in this directory load. JSON files are spec-only and do
not fire.

## Partition layout (task #134)

The library is partitioned by language. Each principle lives under
exactly one partition directory:

```
.provekit/principles/
  README.md                  ← this file
  universal/                 ← always loaded, applies to any language
    division-by-zero.{dsl,json}
    modulo-by-zero.{dsl,json}
    addition-overflow.{dsl,json}
    subtraction-underflow.{dsl,json}
    loop-accumulator-overflow.{dsl,json}
    throw-uncaught.{dsl,json}
  typescript/                ← TS/JS-specific axioms
    null-assertion.{dsl,json}
    unguarded-await.{dsl,json}
    falsy-default.{dsl,json}
    find-undefined-result.{dsl,json}
    match-null-result.{dsl,json}
    reduce-no-initial.{dsl,json}
    split-empty-string.{dsl,json}
    empty-collection-loop.{dsl,json}
    variable-staleness.{dsl,json}
    or-chain-extended-by-fix.dsl   (DSL-only stub)
  cpp/                       ← C/C++ placeholder (TBD; see partition README)
  rust/                      ← Rust placeholder
  java/                      ← Java placeholder
  python/                    ← Python placeholder
  go/                        ← Go placeholder
  disabled/                  ← retired/quarantined entries (NEVER loaded)
```

### Partition-size principle

Per the CDD spec
(`docs/specs/2026-04-27-constraint-driven-development.md`,
"Language partitioning: principles per language"):

- universal axioms apply everywhere
- per-language axioms apply only inside their language's universe
- the partition's size is **inversely proportional to the language's
  compile-time safety story**: C/C++ has the largest, Rust the smallest

### Load-time selection

`enumeratePrincipleFiles(principlesDir, opts)` (`src/principleEnumeration.ts`)
is the single read-side entry point. It walks `universal/` plus the
language partitions detected by `detectLanguages(projectRoot)`
(`src/fix/intake/languageDetect.ts`):

| project signal                  | partitions loaded                 |
|---------------------------------|-----------------------------------|
| `package.json` + `tsconfig.json` | universal + typescript           |
| `Cargo.toml`                    | universal + rust                  |
| `go.mod`                        | universal + go                    |
| `pyproject.toml` / `requirements.txt` | universal + python          |
| `pom.xml` / `build.gradle`      | universal + java                  |
| `.cpp`/`.cc`/`.cxx` files       | universal + cpp                   |
| `.c`-only files                 | universal + c                     |
| polyglot (e.g. TS + Rust)       | universal + typescript + rust     |

The B3 recognize stage and the harvest recognize stage opt into
`loadAllPartitions: true` because they evaluate every applicable
principle regardless of project language.

### Write-time policy

When a principle is minted at runtime (harvest promotion,
`PrincipleStore.add`), it writes into the partition matching the
language tag captured at derivation time. Without a tag, it defaults
to `universal/` — the conservative choice (a candidate that turns out
to be language-specific can be moved later, but a candidate filed
under a specific language that turns out to be universal would be
invisible to projects in other languages).

### Today's reality

The SAST extractor is TypeScript-only. Every shipped DSL fires against
the TS SAST. So `universal/` here means "would be ported to other-
language SAST when it exists," not "fires on any language today." The
cpp/rust/java/python/go placeholder dirs ship empty (with a README
each describing the intended axiom set) so the structural slot is
ready when those SAST extractors land.

## Curation rules

1. **Universally-true axioms only.** Every entry MUST be a tautology
   in any TypeScript / JavaScript codebase (for `typescript/` entries)
   or in any codebase regardless of language (for `universal/`
   entries). If there's a legitimate idiom that triggers the
   principle, the principle is a heuristic and does not belong here.
   Heuristics are biome / eslint's surface; we don't compete.
2. **The validator IS the definition.** Per CDD
   (`docs/specs/2026-04-27-constraint-driven-development.md`,
   "Categorical reduction"): principles + invariants, no heuristics. A
   principle's authority is its discrimination evidence, not the prose
   wrapped around the SMT.
3. **Every active entry needs a fixture pair.** A buggy fixture that
   fires the principle and a clean fixture that does not. The pair
   lives in `scripts/validate-library.ts` and is run via `npx tsx
   scripts/validate-library.ts`. New principles are added to the
   script before they're added to the library.
4. **Growth is rare, by federation-promotion.** Principles graduate
   via the harvest pipeline (cross-corpus validation, `confidenceTier`
   progression in the `principles_library` DB table). Hand-written
   additions are exceptional.
5. **The library remains small forever.** Curation rules apply per
   partition. A partition that grows past ~20 entries is a signal that
   either the partition is too coarse (split) or some entries are
   heuristics (retire).

## Starter axiom set (task #129)

| # | axiom | partition | shipped? |
|---|---|---|---|
| 1 | division-by-zero | universal | yes |
| 2 | modulo-by-zero | universal | yes |
| 3 | NaN equality / inequality | universal | NOT YET (capability gap) |
| 4 | null/undefined dereference | typescript | partial: `null-assertion` covers TS `!`. General DSL pending. |
| 5 | unhandled promise rejection | typescript | partial: `unguarded-await` over-matches (capability gap) |
| 6 | array index out of bounds on read | universal | NOT YET (capability gap) |
| 7 | use-after-close on a resource | universal | NOT YET (capability gap) |

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

`disabled/` stays flat (NOT partitioned) — these are retired entries
that no longer load. Re-partitioning retired heuristics would imply
they're candidates for revival, which contradicts their status.

## Heuristic tier

The original task brief asked to "drop the heuristic tier code path."
Audit found **no heuristic tier exists in the codebase**. The closest
mechanism is `principlesLibrary.confidenceTier` in
`src/db/schema/principlesLibrary.ts` — values
`"advisory" | "warning" | "blocking"` — which is a trust-progression
ladder for harvest-mined principles, not a heuristic-vs-axiom
distinction. It was left intact.

## Re-running validation

```sh
cd /path/to/provekit
npx tsx scripts/validate-library.ts
```

Exit code is 0 only if no entry failed strict adversarial validation
(FAIL-NO-FIRE / FAIL-FALSE-POSITIVE / ERROR all yield exit 1).
DIFF-CONTEXT-ONLY and KNOWN-CAPABILITY-GAP are accepted statuses. The
script walks every partition (universal/, typescript/, ...) so a new
language's axiom set comes online automatically once shipped.
