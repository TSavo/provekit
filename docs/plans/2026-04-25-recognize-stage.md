# B3 Recognize: Move Bug Recognition Upstream into a Mechanical Stage

**Status:** Spec. Ready to land after Leak 6 win 2 (#95) settles its current scope.

## The Architectural Claim

The principle library IS the bug-recognition mechanism. Every principle is a compiled SAST query. Recognition is a database query that runs in microseconds, not an LLM judgment. The whole substrate (SAST, DSL, capability tables, relation registry) exists to make this true.

The current pipeline doesn't honor this. It puts the LLM in the recognition path:

```
intake (LLM) → locate → classify (LLM) → C1 (LLM) → C3 (LLM) → C5 (LLM) → C6 (LLM) → D1 → D2
```

Leak 6 win 2 (in flight) makes one of those LLM calls (C1) skippable when a principle is in the library. That's correct but too narrow. The recognition decision belongs upstream, before C1, and once it fires it should mechanize the entire downstream.

## The Reframe

A new stage between Locate and Classify:

```
intake → locate → B3:RECOGNIZE → ┬─ HIT  → C1m → C3m → C5m → C6m → D1 → D2  (zero LLM, seconds)
                                  └─ MISS → C1  → C3  → C5  → C6  → D1 → D2  (LLM-driven, minutes)
                                                                       ↓
                                                                 harvest → library
```

B3's algorithm:
1. For each principle P in the library, compile P to a SAST query.
2. Run the query against the locus's file.
3. If any match's root node intersects with the locus's primary node: **recognized**. Record `{ principleId, bug_class_id }` and propagate downstream.
4. If multiple principles match: pick the highest-confidence one (or run all in parallel and pick the one whose stored fix template best matches the locus's syntactic shape).
5. If no match: novel bug. Continue to LLM-driven path.

B3 has no LLM call. Wall time: O(library size × file size). At 200 principles and a 1000-LOC file, expect single-digit milliseconds.

## Library Data Shape Expansion

Today a principle stores: DSL source, sometimes an SMT template, optional teaching example.

To support full mechanical short-circuit, each principle must also store:

```typescript
interface LibraryPrinciple {
  id: string;
  bugClassId: string;
  dslSource: string;              // existing — the SAST query
  smtTemplate: string;            // existing — for oracle #1
  bindings: Binding[];            // existing — bind locus identifiers to SMT constants
  fixTemplate: FixTemplate;       // NEW — parameterized AST transformation
  testTemplate: TestTemplate;     // NEW — parameterized regression test
  provenance: BugProvenance[];    // NEW — which BugsJS bugs / customer fixes contributed
  alternateShapes?: ShapeRef[];   // existing (Leak 3 layer 1) — alternate syntactic forms
}

interface FixTemplate {
  // The canonical AST transformation that fixes any matching site.
  // Uses placeholders bound from the principle's match. E.g. for division-by-zero:
  //   "before: ${arithmetic_op.left} / ${arithmetic_op.right}"
  //   "after:  if (${arithmetic_op.right} === 0) throw new Error('Division by zero'); ${arithmetic_op.left} / ${arithmetic_op.right}"
  pattern: string;       // structured-edit pattern with placeholders
  imports?: string[];    // any new imports the fix introduces
  rationale: string;
}

interface TestTemplate {
  // Parameterized vitest test source. Placeholders bound from the principle's match.
  source: string;
  importsFrom: string;  // which module under test
}
```

## Mechanical Pipeline (Recognized Path)

When B3 hits, each downstream stage runs in mechanical mode. No LLM calls.

### C1m: instantiate invariant template
```typescript
const invariant = instantiateSmt(principle.smtTemplate, locus.bindings);
// Run oracle #1 (Z3 SAT check) for verification
const witness = await runOracle1(invariant);
return { invariant, witness, source: "library", principleId: principle.id };
```

### C3m: apply fix template
```typescript
const patchSource = instantiateFixTemplate(principle.fixTemplate, locus);
const patch = parseTemplate(patchSource);  // produces CodePatch
applyPatchToOverlay(overlay, patch);
return { patch, source: "library" };
```

### C5m: instantiate regression test
```typescript
const testSource = instantiateTestTemplate(principle.testTemplate, locus, witness);
writeFileSync(testFilePath, testSource);
const result = runTestInOverlay(overlay, testFilePath);
// Same oracle #9 mutation verification as the LLM path
return { testFilePath, source: "library" };
```

### C6m: skip
Principle already in library. No harvest. Record provenance:
```typescript
appendProvenance(principle, { project: "<customer>", bugId: signal.id });
```

### D1, D2: unchanged
Bundle assembly, oracle #10/11/12/13/15, application logic all the same.

## Wall Time Targets

Recognized path:
- B3: 1-10ms
- C1m (Z3 SAT check): ~50ms
- C3m (apply patch + reindex): ~600ms
- C5m (run regression test): ~2s
- D1+D2: ~1s
- **Total: ~4-5 seconds**

Compare to today's recognized-but-still-LLM-driven path: ~5 minutes (4 LLM calls × ~30s + overhead).

LLM-driven (novel) path stays ~3-5 minutes after Leak 6 wins land.

The result: **once the library covers common cases, ProveKit fixes are seconds. Novel cases are minutes.** "Every PR" becomes architecturally feasible.

## Implementation Sequence

1. **Schema migration**: extend `LibraryPrinciple` with `fixTemplate` and `testTemplate` fields. Existing principles get them filled in (one-time backfill, possibly by harvesting from their existing fixtures).
2. **Stage B3**: implement `recognize()` in `src/fix/stages/recognize.ts`. Pure SAST + DSL evaluation. No LLM.
3. **Mechanical-mode C1/C3/C5**: add `mode: "library" | "llm"` to each stage. The library mode reads from `LibraryPrinciple` and instantiates templates. The LLM mode is the existing path.
4. **Orchestrator routing**: after Locate, call B3. If hit, propagate `mode: "library"` to all downstream stages. If miss, propagate `mode: "llm"`.
5. **Harvest pipeline (#97) writes full templates**: BugsJS harvest's discovery mode produces `fixTemplate` and `testTemplate` alongside the DSL principle. The `Bug-N-fix` diff IS the fix template (after parameterization). The `Bug-N-test` IS the test template.
6. **Backfill existing principles**: division-by-zero, empty-catch, etc. get their fix/test templates extracted from existing dogfood fixtures.

## Risks

### Risk 1: Fix template parameterization is non-trivial

A fix template can't be a string with `${var}` placeholders. It has to be an AST template with positional bindings to capability column values. Otherwise variable shadowing, scoping, and import management break.

**Mitigation**: use ts-morph's structural transformations rather than string templates. Each `FixTemplate` becomes a small TypeScript function that takes locus bindings and returns the modified AST. Stored as compiled code, not as a template string.

### Risk 2: Multiple principles match the same locus

When B3 finds two principles whose queries both match at the locus, ambiguity. Two options:
- Pick by confidence: each principle has a confidence score (0-1), highest wins.
- Run both fix templates speculatively, pick the one whose oracle #2 verdict is `unsat`.

The second is more robust but doubles the cost. Default to confidence; fall back to speculative if confidences are within ε.

### Risk 3: Customer fix overrides library principle

Sometimes the customer wants a specific fix that differs from the library's canonical template (e.g., they prefer `assert(b !== 0)` over `if (b === 0) throw`). The mechanical path would silently apply the wrong style.

**Mitigation**: the recognized path produces a *suggested* fix. The customer's preference (encoded in their `.provekit/preferences.json` or similar) overrides the canonical template per principle.

## Acceptance

- B3 stage implemented; orchestrator routes through it after Locate
- Each principle in the library has `fixTemplate` and `testTemplate` populated
- Recognized path completes in ≤ 10 seconds wall time on division-by-zero fixture
- Existing dogfood tests still pass (the LLM-mode path is unchanged for novel bugs)
- New test: a synthetic project with a known bug class produces a fix via the recognized path with zero LLM calls (verify by stub LLM that asserts complete() is never called)

## Self-Bootstrapping: Every Novel Fix Becomes Future Recognition

The LLM-driven path doesn't just fix a bug — it produces all four pieces of a library template by construction:

| Bundle artifact | Becomes library field |
|---|---|
| C1's invariant | `smtTemplate` (already stored) |
| C3's fix diff | `fixTemplate` (parameterize and store) |
| C5's regression test | `testTemplate` (parameterize and store) |
| C6's DSL principle | `dslSource` (already stored) |

After D2 applies the fix, **harvest captures the full set** into a new `LibraryPrinciple` and adds it to the library.

### BugsJS harvest is a strict subset

BugsJS already provides the diff and the test from production-merged tags. There is nothing to generate.

| Source | C1 (invariant) | C3 (fix) | C5 (test) | C6 (principle) |
|---|---|---|---|---|
| Customer novel bug | LLM | LLM (produces diff) | LLM (produces test) | LLM |
| **BugsJS bug** | (derived from diff, no LLM needed) | **diff already exists in `Bug-N-fix` tag** | **test already exists in `Bug-N-test` tag** | LLM (one call per bug class) |

For BugsJS bootstrap, the LLM is only needed for C6 — and even C6 drops out for cluster members once the first-of-class is harvested and recognition fires.

### Bootstrap cost model

For 452 BugsJS bugs assuming ~100 unique classes with average cluster size 4-5:
- **~100 first-of-class bugs**: one LLM call each (C6 principle derivation from diff). At ~$0.10/call = **~$10**
- **~350 cluster members**: zero LLM calls (recognition fires, existing principle matches, append provenance)
- **Total: ~$10 in compute** to bootstrap a ~150-principle library from 452 production-merged fixes

This is dramatically cheaper than the naive "452 LLM calls" plan because:
1. Recognition mode collapses cluster members to zero LLM cost
2. C3 and C5 are skipped for BugsJS entirely (we already have the diff and test)
3. Only C6's principle-derivation step needs the LLM, and only once per class

### The customer fix-loop completes the cycle

Customer fixes work the same way. After a novel-bug fix completes:
- C3's diff IS the new principle's fix template
- C5's test IS the new principle's test template
- C6's DSL IS the new principle's match query
- All four get stored in the library

Future recognition of the same class fires the recognized path. Customer use grows the library; the library accelerates customer use. The system is self-bootstrapping — every customer fix is a learning event, every BugsJS bug is a learning event, and the LLM-driven path becomes asymptotically rare as the library matures.

## Why This Is The Architecture We Were Already Pointing At

Every layer was building toward this:
- **SAST** = the fact database
- **Capability tables** = the schema of bug-relevant facts
- **Relation registry** (data_flow_reaches, dominates, etc.) = query primitives
- **DSL** = the query language
- **Principle library** = the catalog of compiled queries
- **Bundle templates** (proposed here) = the canonical fix/test for each query

Recognition was always a SAST query. The LLM was only needed to *grow* the library, never to operate it. The Recognize stage just gives that latent architecture its proper shape.
