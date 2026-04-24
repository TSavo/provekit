<!-- This plan was written under the product's old name (neurallog); the implemented system is ProveKit. -->

# `analyze` Integration: Mechanical Binding Derivation + Gap Detection Phase

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Phase D gap detection into `neurallog analyze` so running the CLI against a source file produces `gap_reports` rows in SQLite, visible via `explain --gaps`. No separate commands. Gap output shows up alongside proofs in the normal workflow.

**Architecture:** The existing pipeline runs mechanical templates (via `TemplateEngine`) that produce Z3-verified SMT-LIB blocks. We extend those templates to also emit per-constant source bindings — derived automatically from the AST match data they already have. Violations carry bindings through `buildContracts`. A new `GapDetectionPhase` between derivation and axiom phases reads violations, synthesizes harness inputs from the Z3 witness, calls `detectGaps`, and persists to SQLite. CEGAR-refined violations mark bindings stale and skip. The dormant `invariant_derivation.md` LLM path gets removed.

**Tech Stack:** TypeScript, vitest, tree-sitter (existing), Drizzle + better-sqlite3 (from Phase A-thin), ts-morph (from Phase D-core). No new deps.

**Spec/prior plan:** `docs/specs/2026-04-23-provekit-v2-design.md`, `docs/plans/2026-04-23-phase-ad-core.md` (Phase D-core, shipped).

---

## Context worth carrying

**What the template engine does today.** `src/templates/TemplateEngine.ts:generateProofs` walks a function's AST, matches principles' `astPatterns` against nodes, extracts a `Record<string, string>` of template variable → source variable names (e.g., `{left: "a", right: "b", numerator: "a", denominator: "b"}` for `a / b`), and substitutes those names into the principle's `smt2Template` to produce Z3-verifiable SMT-LIB. The match data is thrown away after substitution.

**What's missing for bindings.** Each template variable corresponds to a specific AST child node with a specific `startPosition.row` and `.text`. Those are the source_line and source_expr fields of an `SmtBinding`. The sort comes from parsing the generated smt2's `(declare-const X SORT)` lines — no schema change needed.

**Why there's a dormant LLM path.** `DerivationPhase.buildPrompt` + `compiledTemplate` load `invariant_derivation.md` but have zero call sites. POSTMORTEM.md's thesis committed to Layer 2 (mechanical templates) as the steady-state pipeline; the LLM-per-signal prompt was vestigial from an earlier era. Deleting it removes confusion and ~90 LOC.

**What Phase D-core already ships.** `detectGaps(args)` takes `{db, clauseId, sourcePath, functionName, signalLine, bindings, z3WitnessText, inputs}` and does the rest. Our integration job is feeding it the right args from the live pipeline.

**Constraints you'll hit.**
- The safe-name transform in `instantiateTemplate` (line 272: `value.replace(/[^a-zA-Z0-9_]/g, "_").slice(0, 30)`) means the SMT constant name may differ from the raw source text. Bindings must record the *transformed* name (as it appears in the SMT) not the raw source text. Mismatching this breaks the comparator join.
- Templates use `{{var}}_<line>` for unsubstituted placeholders (line 278). These synthetic constants have no source correspondent — bindings for them get `source_line: 0, source_expr: "<abstract>"`.
- CEGAR refinement (`DerivationPhase.ts` around line 857) rewrites `v.smt2`. The variable names may change. Bindings from the original match become stale. For Phase A-thin: mark stale and skip gap detection on refined violations.

---

## File structure

**New:**
- `src/pipeline/GapDetectionPhase.ts` — new phase; wires violations to `detectGaps`
- `src/inputs/synthesizer.ts` — parse Z3 model + bindings → JS inputs for harness
- `src/pipeline/GapDetectionPhase.test.ts`, `src/inputs/synthesizer.test.ts`

**Modified:**
- `src/contracts.ts` — `SmtBinding` type + optional field on `Violation` (already done in prior commit; verify)
- `src/templates/Template.ts` — extend `TemplateResult` with `bindings: SmtBinding[]`
- `src/templates/TemplateEngine.ts` — extract bindings from match data
- `src/templates/TemplateEngine.test.ts` — binding extraction tests
- `src/pipeline/DerivationPhase.ts` — (a) remove dormant `buildPrompt`/`compiledTemplate`/`loadTemplate`; (b) carry `bindings` through `buildContracts` onto `Violation.smt_bindings`; (c) in CEGAR refinement, mark refined violations with a `bindings_stale` flag (or clear their `smt_bindings`)
- `src/pipeline/Pipeline.ts` — construct `GapDetectionPhase`; call it between `derivationPhase.execute` and `axiomPhase.execute`

**Deleted:** nothing from disk — `prompts/invariant_derivation.md` stays as reference documentation (it's cited from the design doc); only the unused loader code in `DerivationPhase.ts` goes.

---

## Task 1: Audit and remove the dormant LLM derivation path

**Why first:** removing dead code prevents confusion later when someone reads `DerivationPhase` looking for where the LLM runs per signal. It also tightens the actual call graph before we add the new phase.

**Files:**
- Modify: `src/pipeline/DerivationPhase.ts`
- Test: no dedicated test — the full suite passing after removal is the signal

**What to do:**

- [ ] **Step 1.1: Confirm zero call sites for `buildPrompt` and `compiledTemplate`**

Run from the worktree root:
```bash
grep -rn "buildPrompt\|compiledTemplate\|loadTemplate" src --include="*.ts"
```

Expected: only definitions in `DerivationPhase.ts`. If any consumer references them, STOP and escalate — the plan's premise is wrong.

- [ ] **Step 1.2: Remove the dormant code from `DerivationPhase.ts`**

Delete:
- The `compiledTemplate: HandlebarsTemplateDelegate` field.
- The `this.compiledTemplate = this.loadTemplate();` initialization.
- The entire `loadTemplate()` method.
- The entire `buildPrompt()` method.
- The `import Handlebars from "handlebars"` / `import { readFileSync } from "fs"` (only if they're no longer used elsewhere in the file — check first).

Leave `prompts/invariant_derivation.md` on disk. It's referenced from the v2 design doc as example material for LLM-extracted bindings and worked examples. The binding-emission section you wrote in Task 16 of the prior plan is still valuable reference.

- [ ] **Step 1.3: Run the full test suite**

```bash
npx vitest run
```

Expected: 136/136 pass, no new failures.

- [ ] **Step 1.4: Commit**

```bash
git add src/pipeline/DerivationPhase.ts
git commit -m "refactor(derivation): remove dormant LLM-per-signal prompt loader

buildPrompt, loadTemplate, and compiledTemplate had zero call sites. The
pipeline runs entirely on mechanical templates via templateEngine;
invariant_derivation.md is reference documentation, not runtime input."
```

---

## Task 2: Emit bindings for `binary_expression` templates (divide-by-zero as the prototype)

**Why this first:** binary_expression is the highest-leverage pattern — divide, modulo, add/sub/mul overflow, falsy-default, all share the `left`/`right` / `numerator`/`denominator` variable shape. Getting this pattern right generalizes to 6+ principles. It's also where the canonical gap-detection case (0/0 → NaN) lives.

**Files:**
- Modify: `src/templates/Template.ts` (extend `TemplateResult`)
- Modify: `src/templates/TemplateEngine.ts` (extract bindings)
- Test: `src/templates/TemplateEngine.bindings.test.ts` (new)

**What to do:**

- [ ] **Step 2.1: Read the current `TemplateResult` shape**

```bash
cat src/templates/Template.ts
```

It's probably `{ signalLine, signalType, smt2, claim, principle, confidence }`. You'll add `bindings: SmtBinding[]`.

- [ ] **Step 2.2: Write the failing test**

Create `src/templates/TemplateEngine.bindings.test.ts`. The test should:

1. Parse a fixture TS function like:
```ts
export function divide(a: number, b: number): number {
  const q = a / b;
  return q;
}
```
into a tree-sitter AST using the existing `parseFile` from `src/parser.ts`.

2. Call `templateEngine.generateProofs(fnNode, "divide", "src/divide.ts")`.

3. Find the result whose `principle` is `division-by-zero` (or whichever ID the seed principle uses — check `.neurallog/principles/division-by-zero.json`).

4. Assert:
   - `result.bindings` is non-empty
   - There's a binding for the denominator with `source_expr: "b"` (or the safe-name of it — inspect `result.smt2` to see what constant name was actually used), `source_line: 2`, `sort: "Real"` (parsed from the `(declare-const ... Real)` in the smt2)
   - Any unsubstituted placeholder in the smt2 (e.g., `sum_2`) gets a binding with `source_line: 0, source_expr: "<abstract>"`

Keep the test focused — one function, one assertion block. You'll generalize in later tasks.

- [ ] **Step 2.3: Run it — verify it fails**

```bash
npx vitest run src/templates/TemplateEngine.bindings.test.ts
```

Expected: `result.bindings` is undefined (property doesn't exist yet).

- [ ] **Step 2.4: Extend `TemplateResult`**

In `src/templates/Template.ts`, add:
```ts
import type { SmtBinding } from "../contracts.js";
```
(Path style — check whether existing imports in this file use `.js` or not; match that.)

Add to `TemplateResult`:
```ts
bindings: SmtBinding[];
```

- [ ] **Step 2.5: Add the binding extractor**

In `TemplateEngine.ts`, add a private method `extractBindings(node, vars, generatedSmt2, line)`:

- Parse `(declare-const NAME SORT)` lines from `generatedSmt2` via a simple regex. Build `Map<smtName, sort>`.
- For each `(templateVarName, sourceName)` in `vars` where the key doesn't start with `_` (those are internal guards):
  - Compute the safeName the same way `instantiateTemplate` does: `sourceName.replace(/[^a-zA-Z0-9_]/g, "_").slice(0, 30) || templateVarName`. This is the name that actually appears in the emitted SMT.
  - Look up the sort from the declare-const map. If not present (the template variable wasn't declared), skip — it's not an SMT constant in this block.
  - Find the AST source position for this template variable: re-walk `node` (the matched AST node) and use the same accessors the matchPattern code used to extract the variable. For binary_expression with variables `left`, `right`, `numerator`, `denominator`: those all come from `node.childForFieldName("left")` / `"right"`. Record `source_line: child.startPosition.row + 1`, `source_expr: child.text.slice(0, 80)`.
  - Emit the binding.
- For each SMT constant in the declare-const map that wasn't matched to a template variable (synthetic `_<line>` names from the fallback substitution), emit an abstract binding: `{smt_constant: name, source_line: 0, source_expr: "<abstract>", sort}`.

Return `SmtBinding[]`.

- [ ] **Step 2.6: Wire into `generateProofs`**

Where `results.push({signalLine, signalType, smt2, claim, principle, confidence})` currently happens (around line 54), also include `bindings: this.extractBindings(node, match, smt2, line)`.

- [ ] **Step 2.7: Run the test — verify it passes**

```bash
npx vitest run src/templates/TemplateEngine.bindings.test.ts
npx vitest run
```

Expected: binding test passes; full suite still 136+.

- [ ] **Step 2.8: Commit**

```bash
git add src/templates/ 
git commit -m "templates: extract per-constant bindings from binary_expression match data

Each TemplateResult now carries bindings mapping the emitted SMT
constants to their source positions. Derived from AST match metadata
the engine already computes; no per-principle authoring. Binary-expr
principles covered: division-by-zero, modulo-by-zero, addition-overflow,
subtraction-underflow, multiplication-overflow, falsy-default."
```

---

## Task 3: Generalize binding extraction to non-binary patterns

**Why:** binary_expression isn't the only pattern. `non_null_expression` (for null-assertion principles), `call_expression` (for method calls like `.reduce`, `.match`, `.find`, shell-injection), `try_statement`, `await_expression`, `throw_statement`, `assignment_expression`, and `if_statement` all need the same treatment. Each has its own extraction shape because `matchPattern` puts different variables in `vars` depending on the pattern type.

**Files:**
- Modify: `src/templates/TemplateEngine.ts` (expand `extractBindings` per pattern type)
- Test: extend `src/templates/TemplateEngine.bindings.test.ts`

**What to do:**

- [ ] **Step 3.1: Enumerate the variable-to-AST-node mapping per pattern type**

Re-read `TemplateEngine.matchPattern` and for each `if (pattern.nodeType === ...)` branch, write down which `vars.X` gets set and which AST node it comes from:

- `binary_expression`: `left`/`right`/`numerator`/`denominator`/`param` → children by field name `"left"`, `"right"`, and `findParamRef` result.
- `non_null_expression`: `value` → `node.firstNamedChild`.
- `try_statement`: no variables extracted into `vars` — handler emptiness is a guard, not a binding.
- `await_expression`, `throw_statement`: same — guards only, no user-visible SMT constants beyond what the smt2Template declares internally. Those will fall through to the abstract sentinel.
- `assignment_expression`: `prop` → `left.childForFieldName("property")`.
- `if_statement`: `condition` (text), `var` (modified variable name from `findModifiedVars`).
- `for_*`/`while`: `accumulator` from `findAccumulator`.
- `call_expression`: no explicit vars except guard flags.

- [ ] **Step 3.2: Extend `extractBindings` with a per-pattern dispatch**

Rather than pattern-match on `pattern.nodeType` inside extractBindings (which would duplicate matchPattern's logic), pass the AST node + vars + the pattern, and re-walk the node using the SAME accessors matchPattern used. The AST node doesn't change between matchPattern and extractBindings, so re-deriving positions is cheap.

Extract a small helper `astNodeForVar(matchNode, pattern, varName): SyntaxNode | null` that returns the AST node a given vars key came from:
- `binary_expression` + `left` → `matchNode.childForFieldName("left")`
- `binary_expression` + `right` → `matchNode.childForFieldName("right")`
- `binary_expression` + `numerator`/`param` → same resolution as matchPattern did
- `non_null_expression` + `value` → `matchNode.firstNamedChild`
- etc.

For vars keys where no clean AST node exists (e.g., `condition` from an if_statement — it's the raw text of the whole condition, not a single node), emit the binding with `source_line: <matchNode.startPosition.row + 1>, source_expr: <vars[key].slice(0, 80)>, sort: <from declare-const>`.

- [ ] **Step 3.3: Extend the test to cover each pattern type**

Add test cases for at least: non_null_expression (a `!` assertion on a nullable variable), call_expression with method (e.g., `.reduce()` without initial value), try_statement with empty catch, assignment_expression.

For each, assert bindings are non-empty and that at least one binding has a plausible `source_line` matching where the variable appears in the fixture source.

- [ ] **Step 3.4: Run — verify pass**

```bash
npx vitest run
```

- [ ] **Step 3.5: Commit**

```bash
git add src/templates/
git commit -m "templates: binding extraction for non_null, call, try, assignment patterns"
```

---

## Task 4: Carry bindings through `buildContracts` onto `Violation.smt_bindings`

**Why:** templates now produce bindings, but `DerivationPhase.buildContracts` builds `Violation` objects from `VerificationResult[]`. Those VerificationResults currently don't carry bindings — they're constructed from TemplateResult but only `smt2`, `principle`, etc. are forwarded. Need to add bindings to VerificationResult and plumb it through.

**Files:**
- Modify: `src/pipeline/DerivationPhase.ts` (around line 197-209 where templateVerifications gets built; and `buildContracts` where Violations are constructed)
- Test: `src/pipeline/DerivationPhase.bindings.test.ts` (new)

**What to do:**

- [ ] **Step 4.1: Read the `VerificationResult` type**

```bash
grep -n "VerificationResult\|interface Verif" src/verifier.ts src/pipeline/DerivationPhase.ts | head
```

Likely in verifier.ts or a types file. Extend it with an optional `bindings?: SmtBinding[]`.

- [ ] **Step 4.2: Write the failing test**

Create `src/pipeline/DerivationPhase.bindings.test.ts`. Use the existing `examples/division-by-zero.ts` (or a similar fixture). Construct a minimal pipeline — graph phase, context phase, derivation phase — and assert that the resulting `Contract.violations[N].smt_bindings` is populated for at least one violation.

(If setting up the pipeline is heavy, alternatively call `DerivationPhase.executeForFile` directly with a hand-built FunctionNode input.)

- [ ] **Step 4.3: Wire bindings through**

Where `templateVerifications.push({...})` happens (line ~200), add `bindings: tr.bindings`.

In `buildContracts`, where a Violation gets constructed from a sat VerificationResult, add `smt_bindings: v.bindings`.

- [ ] **Step 4.4: Run the test — verify it passes**

```bash
npx vitest run
```

- [ ] **Step 4.5: Commit**

```bash
git add src/pipeline/ src/verifier.ts
git commit -m "derivation: carry template bindings onto Violation.smt_bindings"
```

---

## Task 5: Input synthesizer — Z3 model + bindings + function params → JS inputs

**Why:** `detectGaps` needs an `inputs: Record<string, unknown>` to drive the harness. The Z3 witness contains values for SMT constants. Bindings tell us which SMT constants correspond to function parameters. Parameter names come from the function's AST. Put the three together and we have real inputs.

**Files:**
- Create: `src/inputs/synthesizer.ts`
- Create: `src/inputs/synthesizer.test.ts`

**What to do:**

- [ ] **Step 5.1: Design the API**

```ts
import type { SmtBinding } from "../contracts.js";
import type { Z3Value } from "../z3/modelParser.js";

export interface SynthesizeArgs {
  functionSource: string;     // raw TS source of the target function
  functionName: string;       // to locate the function in the source
  bindings: SmtBinding[];
  z3Model: Map<string, Z3Value>;
}

export function synthesizeInputs(args: SynthesizeArgs): Record<string, unknown>;
```

Returns an inputs object mapping parameter names to JS values. Parameters the Z3 model doesn't cover get sensible defaults (`0` for numeric params, `""` for string, etc.) or are omitted — the harness handles missing inputs gracefully.

- [ ] **Step 5.2: Write the failing test**

Cases to cover:
1. Simple number param: binding for `b` with sort Real, Z3 model `b=0`, function `divide(a, b)` → `{a: 0, b: 0}` (a defaulted since unconstrained).
2. Z3 model has an Int-sort constant that maps to a param → BigInt or Number (decide: Number if safe, BigInt otherwise).
3. Binding exists but function has no parameter by that name → binding skipped, param defaulted.
4. Parameter has no matching binding → param defaulted.
5. Z3 Real value is `"div_by_zero"` / `"nan"` → materialize as `NaN`.

- [ ] **Step 5.3: Implement**

Parse the function's parameter names from source. Simple approach: regex on the function declaration (`function NAME\(([^)]*)\)` then split on commas, strip type annotations). Robust approach: use ts-morph (already a dep). ts-morph is robust and matches what snapshot instrumentation uses — prefer it.

For each parameter:
- Find a binding whose `source_expr` matches the parameter name (normalize whitespace).
- Look up the Z3 model value for that binding's `smt_constant`.
- Materialize: Real number → JS number; Real "nan"/"div_by_zero" → `NaN`; Real "+infinity" → `Infinity`; Real "-infinity" → `-Infinity`; Int → Number if safe, else BigInt; Bool → boolean; String → string.
- If no binding/model, default (`0` for number-ish, `null` for unknown).

Return the map.

- [ ] **Step 5.4: Run the test**

```bash
npx vitest run src/inputs/synthesizer.test.ts
```

- [ ] **Step 5.5: Commit**

```bash
git add src/inputs/
git commit -m "inputs: synthesize harness inputs from Z3 model + bindings + function params"
```

---

## Task 6: CEGAR refinement — mark refined violations as bindings-stale

**Why:** CEGAR refinement rewrites `v.smt2`. The new SMT may reference different constant names. The template's original bindings are no longer trustworthy. Safest move: clear `smt_bindings` (or set a `bindings_stale: true` flag) so `GapDetectionPhase` skips these violations rather than running the detector on mismatched bindings.

**Files:**
- Modify: `src/pipeline/DerivationPhase.ts` (around the CEGAR section, ~line 880 where `v.smt2 = revisedSmt` happens and similar)

**What to do:**

- [ ] **Step 6.1: Locate the CEGAR mutation sites**

```bash
grep -n "v\.smt2 = revisedSmt\|v\.witness = newWitness" src/pipeline/DerivationPhase.ts
```

- [ ] **Step 6.2: Clear bindings on refinement**

At each site where CEGAR rewrites the violation's smt2, add:
```ts
v.smt_bindings = undefined; // CEGAR refinement invalidates the original template's bindings
```

- [ ] **Step 6.3: Smoke-check**

Run the full suite: `npx vitest run`. CEGAR test paths (if any) should still pass because clearing an optional field doesn't change Z3 behavior.

- [ ] **Step 6.4: Commit**

```bash
git add src/pipeline/DerivationPhase.ts
git commit -m "derivation: clear smt_bindings on CEGAR refinement (bindings no longer trustworthy)"
```

---

## Task 7: `GapDetectionPhase` + wire into `Pipeline.runFull`

**Why:** this is the integration point. For each Violation that has bindings + a witness + points at an executable target function, call `detectGaps`. Gap reports land in SQLite. Non-executable functions, bindings-stale violations, and violations without witnesses are skipped gracefully.

**Files:**
- Create: `src/pipeline/GapDetectionPhase.ts`
- Create: `src/pipeline/GapDetectionPhase.test.ts`
- Modify: `src/pipeline/Pipeline.ts` (construct the phase; call it after `derivationPhase.execute`, before `axiomPhase.execute`)

**What to do:**

- [ ] **Step 7.1: Read existing phase shapes**

```bash
ls src/pipeline/
grep -n "Phase\|execute\|PhaseResult" src/pipeline/*.ts | head -20
```

Match the existing phase interface — `execute(input, options): Promise<PhaseResult<Output>>`.

- [ ] **Step 7.2: Write the failing integration test**

Create `src/pipeline/GapDetectionPhase.test.ts`. Build a minimal derivation output with a Violation carrying bindings + witness + pointing at the `examples/division-by-zero.ts` fixture. Call the phase. Assert:
- A trace row exists in SQLite.
- A gap_reports row of kind `ieee_specials` exists.
- Violations without bindings are skipped (no trace written).
- Violations with `smt_bindings === undefined` (CEGAR-refined) are skipped.

- [ ] **Step 7.3: Implement the phase**

Sketch:
```ts
export interface GapDetectionInput {
  derivation: DerivationOutput;
  projectRoot: string;
}

export interface GapDetectionOutput {
  reportsWritten: number;
  skipped: { missingBindings: number; missingWitness: number; untestable: number };
}

export class GapDetectionPhase {
  async execute(input: GapDetectionInput, options: PhaseOptions): Promise<PhaseResult<GapDetectionOutput>> {
    const db = openDb(join(input.projectRoot, ".neurallog", "neurallog.db"));
    // run schema migrations via drizzle-kit at init (or do it inline via migrate())
    migrate(db, { migrationsFolder: "./drizzle" });
    
    let reportsWritten = 0;
    const skipped = { missingBindings: 0, missingWitness: 0, untestable: 0 };
    
    for (const { violation, context } of input.derivation.newViolations) {
      // context is the contract key, in signalKey format file/fn[line]
      // extract file, function, line via contracts.signalKey parsing
      // fetch contract from store to get absolute path
      // check bindings, witness
      // synthesize inputs
      // call detectGaps — catch any exceptions, increment skipped.untestable
    }
    
    db.$client.close();
    return { data: { reportsWritten, skipped } };
  }
}
```

Handle:
- Missing/undefined `smt_bindings` → increment skipped.missingBindings, continue.
- Missing `witness` → increment skipped.missingWitness, continue.
- `detectGaps` throws or the harness returns `untestable` outcome → increment skipped.untestable, continue. Don't fail the pipeline on per-violation errors.
- Signal key parsing: split on `/` and `[` to get file path + function + line. Re-use `signalKey`'s inverse if it exists; if not, parse here.

Needs a clause row to exist in SQLite before `detectGaps` runs (detectGaps inserts clause_bindings referencing a clauseId). Before the detectGaps call, insert a clauses row with the violation's smt2 + a hash, keep its id, pass to detectGaps.

- [ ] **Step 7.4: Wire into `Pipeline.runFull`**

In `src/pipeline/Pipeline.ts`, between:
```ts
const { data: derivation } = await this.derivationPhase.execute(...);
```
and:
```ts
const { data: report } = await this.axiomPhase.execute(undefined, options);
```

Add:
```ts
const { data: gapReport } = await this.gapDetectionPhase.execute(
  { derivation, projectRoot: config.projectRoot },
  options,
);
```

Include `this.gapDetectionPhase = new GapDetectionPhase()` in the constructor, alongside the other phases.

Also include `gapReport` in `PipelineResult` if tests or the CLI consume it (probably not for v1 — just log counts).

- [ ] **Step 7.5: Run tests**

```bash
npx vitest run src/pipeline/GapDetectionPhase.test.ts
npx vitest run
```

- [ ] **Step 7.6: Commit**

```bash
git add src/pipeline/
git commit -m "pipeline: GapDetectionPhase runs detectGaps on violations between derivation and axiom phases"
```

---

## Task 8: End-to-end — `analyze` produces a gap, `explain --gaps` renders it

**Why:** final acceptance. If this works on a real fixture end-to-end, Phase A-thin + D-core + the integration is a shipped product feature.

**Files:**
- Create: `src/e2eAnalyze.test.ts`
- Possibly: `examples/division-by-zero.ts` already exists from Phase D-core; verify it's still there

**What to do:**

- [ ] **Step 8.1: Write the integration test**

Create `src/e2eAnalyze.test.ts`:

1. Set up a scratch project dir with `.neurallog/` created, the division-by-zero fixture copied to `src/divide.ts`, and `.neurallog/principles/*.json` copied over (or reference them from the main tree via symlink — whichever works).

2. Construct a `Pipeline`. Call `runFull` with `entryFilePath: <fixture path>`, `projectRoot: <scratch dir>`, `model: "sonnet"` (won't actually be used if LLM paths are disabled — the mechanical template engine takes over), `maxConcurrency: 1`.

3. After `runFull` completes, assert:
   - At least one Contract has a violation with populated `smt_bindings`.
   - The SQLite db at `.neurallog/neurallog.db` exists.
   - `gap_reports` table has at least one row of kind `ieee_specials`.
   - Calling `explainGaps(db, <contractKey>)` returns a string containing `"encoding-gap"`, `"NaN"`, `"ieee_specials"`.

- [ ] **Step 8.2: Run and debug until green**

```bash
npx vitest run src/e2eAnalyze.test.ts
```

If anything fails:
- Bindings missing → check Task 2/3 extractor
- Violation has no witness → check whether Z3 is installed and `verifyBlock` is returning `sat` with a witness (run `echo ... | z3 -in` manually if suspect)
- detectGaps throws → check `clause_bindings` FK constraint (bindings need to precede witness inserts)
- Input synthesis produces wrong values → check the Z3 model parser output against the model text

- [ ] **Step 8.3: Run the full suite**

```bash
npx vitest run
```

Expected: 136 + new tests, all pass.

- [ ] **Step 8.4: Smoke-test via the actual CLI**

From the worktree root:
```bash
mkdir -p /tmp/gap-demo/src /tmp/gap-demo/.neurallog
cp examples/division-by-zero.ts /tmp/gap-demo/src/divide.ts
cp -r .neurallog/principles /tmp/gap-demo/.neurallog/
cd /tmp/gap-demo
# Build the CLI in the worktree first
cd - && npm run build
node /Users/tsavo/provekit/.worktrees/v2-phase-ad-core/dist/index.js analyze /tmp/gap-demo/src/divide.ts
# Expect output showing violations + a note about gap_reports
node /Users/tsavo/provekit/.worktrees/v2-phase-ad-core/dist/index.js explain /tmp/gap-demo/src/divide.ts:7 --gaps
# Expect THESIS-style encoding-gap output
```

- [ ] **Step 8.5: Commit**

```bash
git add src/e2eAnalyze.test.ts
git commit -m "e2e: neurallog analyze on division-by-zero.ts produces an ieee_specials gap, explain --gaps renders it"
```

---

## Self-Review

Spec coverage:

| Piece of the right-move recommendation | Task |
|---|---|
| Derive bindings mechanically from template match | Task 2 + 3 |
| Extend `TemplateResult.bindings` | Task 2 |
| Thread bindings onto `Violation.smt_bindings` | Task 4 |
| Input synthesis (Z3 model + bindings + params → JS inputs) | Task 5 |
| CEGAR marks bindings stale, skips | Task 6 |
| `GapDetectionPhase` wired into `runFull` | Task 7 |
| Delete dormant LLM-per-signal path | Task 1 |
| End-to-end fixture integration | Task 8 |

Placeholder scan: none. Each task has concrete verification steps and commit messages.

Type consistency:
- `SmtBinding` defined on `src/contracts.ts` (already done). Used by TemplateResult (Task 2), VerificationResult (Task 4), Violation (already), synthesizer (Task 5), GapDetectionPhase (Task 7).
- `synthesizeInputs` returns `Record<string, unknown>` — matches what `detectGaps.inputs` consumes.

Scope check: one plan, one integration goal (`analyze` produces gaps). Not multi-subsystem.

---

## Execution Handoff

Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task, two-stage review, fast iteration.
2. **Inline Execution** — executing-plans skill, batch with checkpoints.

Which?
