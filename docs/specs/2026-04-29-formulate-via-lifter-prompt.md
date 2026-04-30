# Formulate-via-lifter Refactor — Teaching Prompt

> Dispatch this prompt to the `implementer-against-spec` agent.
> Single commit. Conventional commit format. Co-Authored-By: Claude Sonnet 4.6.

## Stakes

The bug-fix workflow's `formulate` stage is the moment the framework
synthesizes invariants from intent. Today it produces invariants in
the LEGACY shape — Z3-targeted IR formulas via the old IR-library
builder API, with an `InvariantClaim` shape that doesn't go through
the new lifter and doesn't compose against kit catalog bridges.

The architecture-correct shape (per `docs/specs/2026-04-29-ts-ir-language.md`
and `docs/specs/2026-04-29-correctness-is-a-hash.md`):

1. The LLM produces TS-IR-language SURFACE text — the same TypeScript
   subset humans would write in `.invariant.ts` files.
2. The lifter (`src/ir/lift/`, shipped this session) projects that
   surface to `IrFormula`.
3. The canonicalizer produces a `propertyHash`.
4. The memento composes against kit catalog CIDs (e.g., the TS-kit's
   parseInt bridge) when the invariant uses kit symbols.
5. The output is a `ClaimEnvelope` with the new shape, signed.

This refactor closes the gap. After it lands, every formulate output
flows through the lifter (the same path human-authored `.invariant.ts`
files take), composes against the global proof DAG, and is auditable
the same way as any other published invariant.

## Read first

In order. One-line rationale per file:

1. **`docs/specs/2026-04-29-ts-ir-language.md`** — Sections 2 (the
   two-LLM-call architecture), 9 (the lifter dispatch table), 13
   (verification cadence at git commit), 15 (three-step unit of work),
   Appendix C (the one-size-fits-all LLM template). Especially
   Appendix C — that's the prompt template the new formulate uses.

2. **`docs/specs/2026-04-29-correctness-is-a-hash.md`** — Sections
   "What ProvekIt is" (scope discipline), "Adding propositions is
   free" (the proofHash composition), "More immutable than Bitcoin"
   (immutability properties).

3. **`src/ir/lift/index.ts`** — `liftProject(program)` API. The new
   formulate uses this to lift the LLM's surface text into IrFormula.

4. **`src/canonicalizer/canonicalize.ts`** — `propertyHashFromFormula`.
   Used to produce the propertyHash from the lifted IR.

5. **`src/claimEnvelope/mint.ts`** — `mintMemento`, `mintBridge`. Use
   these instead of hand-rolling the envelope.

6. **`src/workflow/producers/formulate.ts`** (120 lines) — the current
   producer wrapper. THIS is what gets refactored.

7. **`src/fix/stages/formulateInvariant.ts`** (1650 lines) — the
   underlying implementation. Read enough to understand the LLM
   prompting + parsing logic. You don't need to rewrite this file
   wholesale; the new behavior layers on top.

8. **`src/workflow/producers/formulate.test.ts`** — existing tests.
   Update to match the new shape; some test expectations change.

9. **`src/workflows/bug-fix.test.ts`** — workflow-level test that
   exercises formulate's output downstream. Adjust expectations.

10. **`src/integration/bug-fix-workflow.smoke.test.ts`** — the e2e
    smoke. The mock for `formulateInvariant` may need updating.

## What you are building

**End state:** the formulate stage's output memento is a `ClaimEnvelope`
whose evidence variant carries the LLM-produced surface text + the
lifted `IrFormula` + the propertyHash, signed by the formulate
producer. Downstream consumers (do-the-work, recognize, bundle)
receive this new shape.

**Two implementation paths — pick one in your judgment:**

**Path A: New producer, swap in manifest** (cleaner migration)
- Add `src/workflow/producers/formulateViaLifter.ts` as a NEW Stage
- `formulateViaLifter` capability becomes the default
- The legacy `formulate` capability is retained as a back-compat alias
- The manifest's `formulate` reference points at the new producer
- Downstream stages adapt to the new shape

**Path B: In-place refactor** (smaller blast radius)
- Modify `src/workflow/producers/formulate.ts` to use the new flow
- The capability name stays the same; the SHAPE of the output changes
- Downstream stages adapt to the new shape

**Recommendation: Path A.** Easier to test in isolation; legacy code
stays compileable as fallback; smoke can switch over once new path is
proven. The agent's call.

## The new formulate's contract

```typescript
export interface FormulateStageInput {
  intent: IntentSignal;        // from intake stage
  investigateReport: InvestigateReport;  // from investigate stage
  classifyOutput?: IntentClassification;  // optional; from classify
  // NEW: tests are a first-class intent source per spec §15
  tests?: { source: string; testNames: string[] }[];
  // NEW: kit catalog CIDs in scope (so the lifter knows what's available)
  kitCatalogCids?: string[];
}

export interface FormulateStageOutput {
  // Preserved from legacy: the binding scope + bindings + name + hint
  bindings: Bindings;
  scope: BindingScope;
  hint?: CompilationHint;
  name: string;

  // NEW: the LLM-produced surface text (TS-IR-language, the .invariant.ts content)
  surfaceText: string;

  // NEW: the lifted IrFormula (from the new lifter)
  formula: IrFormula;

  // NEW: the propertyHash (from the canonicalizer)
  propertyHash: string;

  // NEW: kit catalog CIDs this formula composes against
  inputCidsToCompose: string[];

  // For backward compat with downstream stages that still expect
  // InvariantClaim-shaped data: the formula doubles as the InvariantClaim.formula.
  // Keep the InvariantClaim shape exposed for do-the-work + recognize.
  invariantClaim: InvariantClaim;
}
```

The `inputCidsToCompose` is what the runner uses to populate the
formulate memento's `inputCids` field — the kit catalog CIDs that the
formulate's output composes against.

## The LLM prompt template

Use the one-size-fits-all template from `2026-04-29-ts-ir-language.md`
Appendix C. Substitute:

- `{{diff}}` — leave as `"(no diff yet; prospective change)"` for the
  bug-fix workflow's prospective use case, OR pass the diff if available
- `{{tests}}` — the `tests` input, formatted as test code blocks
- `{{intent_text}}` — the IntentSignal's text + classifyOutput's
  classification
- `{{file_paths_for_invariant_files}}` — derived from
  investigateReport's locus

The prompt instructs the LLM to output `.invariant.ts` source. The
output is a string of TypeScript code containing one or more `property(...)`
calls. Capture this string as `surfaceText`.

## Lifting the surface text

```typescript
import * as ts from "typescript";
import { liftProject } from "../../ir/lift/index.js";

function liftSurfaceText(surfaceText: string, virtualFilePath: string): {
  formula: IrFormula;
  diagnostics: ts.Diagnostic[];
} {
  // Construct a minimal in-memory ts.Program from the surface text.
  // Use ts.createSourceFile + a mock host that resolves the file.
  // The lifter walks the program's files looking for .invariant.ts;
  // ensure the virtualFilePath ends with .invariant.ts.
  
  // Run liftProject; extract the (single) LiftedProperty's formula.
  // If multiple properties were emitted, pick the first or compose.
  // If diagnostics is non-empty, throw or return error.
}
```

The lifter expects a `ts.Program`. For an in-memory string, build a
custom CompilerHost that returns the surface text for the virtual
filename and falls through to the default host for everything else.
See `src/ir/lift/lift.test.ts` for the pattern.

## Quiet parts

1. **The legacy formulateInvariant.ts is 1650 lines.** Don't try to
   rewrite it. Keep it as the LLM-call layer; ADD the lift+canonicalize
   step on top. The new producer's `run()` calls the legacy LLM logic
   to get the surface text, then lifts it.

2. **kitCatalogCids resolution is out of scope.** For v1, accept this
   as an empty list and let the runner resolve later. The new
   formulate's output CAN have an empty `inputCidsToCompose` array;
   downstream composition will handle this.

3. **Backward-compat InvariantClaim is critical.** do-the-work expects
   `InvariantClaim` with bindings + scope + formula + name. The new
   output type extends, doesn't replace; the legacy shape stays
   exposed under `invariantClaim`.

4. **The propertyHash is the new identity.** The legacy formulate
   computed a hash internally for its own caching; the new propertyHash
   is the canonical hash from the canonicalizer. Use that.

5. **The lifter's input expects strict mode tsconfig.** When you build
   the in-memory program, set `strict: true`. The lifter's anchoring
   check rejects properties not in `*.invariant.ts` files; the virtual
   path must end in `.invariant.ts`.

6. **Kit catalog bridges (composition target) for v1: no-op.** Until
   real kit catalogs are loaded by the project, the formulate's
   inputCids stays minimal (just the upstream intake/investigate
   memento CIDs). The kit catalog composition is a follow-up task.

7. **Mock the LLM in tests.** The existing tests use `StubLLMProvider`
   from `src/fix/types.ts`. Same pattern.

## Tasks

1. Pick Path A or B; commit to the choice.
2. Add the new producer file (Path A) or modify the existing one
   (Path B).
3. Wire `liftSurfaceText` helper that constructs an in-memory
   `ts.Program` and lifts a single property.
4. Update `FormulateStageInput` and `FormulateStageOutput` interfaces.
5. Update the LLM prompt to the one-size-fits-all template.
6. Update tests in `formulate.test.ts` (and parallel tests in
   `formulateInvariant.test.ts` if they touch shape).
7. Update workflow tests in `bug-fix.test.ts` to expect the new
   output shape.
8. Update the smoke test's stub for `formulateInvariant` if needed.
9. Run the full test suite. Adjacent suites (`src/workflow`,
   `src/workflows`, `src/integration`, `src/ir/lift`,
   `src/canonicalizer`) MUST stay green.

## Verify

```sh
cd /Users/tsavo/provekit
npx vitest run src/workflow/producers/formulate.test.ts
npx vitest run src/workflows/bug-fix.test.ts
npx vitest run src/integration/bug-fix-workflow.smoke.test.ts
npx tsc --noEmit
```

Expected: all green. Test count delta is positive (new tests for the
lift + propertyHash path).

## Cut list

Out of scope:
- Real LLM API calls (use the stub provider)
- Kit catalog composition (inputCidsToCompose stays minimal for v1)
- Refactoring downstream stages (do-the-work, recognize) — the
  backward-compat InvariantClaim keeps them functional
- The legacy formulateInvariant.ts internal logic — keep as is
- New evidence variant types — reuse legacy-witness for now

## Project conventions to apply

- pnpm, not npm
- Single conventional commit
- Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
- No emojis
- Comments only when WHY is non-obvious

## Commit

```
feat(formulate): synthesize invariants in TS-IR-language surface

Refactor the formulate stage to produce invariants in the architecture-
correct shape per docs/specs/2026-04-29-ts-ir-language.md §2 (two-LLM-
call architecture) and §15 (three-step unit of work).

The new formulate:
1. Takes (intent, investigateReport, tests, kitCatalogCids) as input
2. Invokes the LLM with the one-size-fits-all template (Appendix C)
3. Captures the LLM's TS-IR-language SURFACE text
4. Lifts the surface via the new lifter (src/ir/lift/)
5. Canonicalizes via propertyHashFromFormula
6. Outputs FormulateStageOutput with surfaceText + formula +
   propertyHash + invariantClaim (backward compat)

Path: <A or B>
Files modified: <list>
Test count delta: +N (new tests for lift+canonicalize path)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

Report SHA, test count delta, path chosen (A or B), surfaced spec gaps.
