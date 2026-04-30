# provekit lift v0: synthesize `.proof` from existing TS code

> Status: design draft, scaffold-only landed. Detect stage real, the
> rest stubbed. Author: T + Claude Opus 4.7. Date: 2026-04-30.

> Companion to `protocol/specs/2026-04-29-formulate-via-lifter-prompt.md`
> and `protocol/specs/2026-04-29-ts-ir-language.md`. Read those after this.

## Stakes

ProvekIt's v1 contract requires the developer to hand-author `must(...)`
calls inside `.invariant.ts` files. Adoption is gated on that authoring
step. The corpus of existing TypeScript that has *no* invariants has
zero coverage today. Lift is the retrospective on-ramp: point it at an
unannotated `parseInt.ts`, get a signed `.proof` out, ship it next to
the package. No source rewrite. No `must()`.

The proof must be byte-equivalent (CID-identical) to the hand-authored
version of the same invariant. That is the dogfood test: lift wins iff
its output is indistinguishable from what an expert would have typed.

## What lift IS NOT

Lift is **not** the existing `formulate` stage. `formulate` is the
prospective bug-fix-workflow producer: it consumes
`(intent, investigateReport, tests)` from a live debugging session and
emits an `.invariant.ts` surface text that flows downstream through
canonicalization. Lift is the retrospective sibling: no intent, no diff,
no investigate report. Just "here is some TS code, find its
preconditions, mint a `.proof`."

Lift is also **not** the existing `src/ir/lift/` module. That module
lifts `.invariant.ts` SURFACE text into an `IrFormula` (one layer of the
canonicalizer pipeline). The new feature operates one layer up: it
*synthesizes* the surface text in the first place, then hands it to the
existing `liftProject` to do the AST → IR work.

To avoid the verb collision, internal code lives under
`src/proveLift/`. The CLI surface is `provekit lift <file>.ts`. The
existing module stays `src/ir/lift/` (the IR-lifter); new module is
"the prove-lifter."

## Inputs and outputs

**Input:** a single `.ts` file with exactly one exported function.
Argument and return types must be primitive (`number`, `string`,
`boolean`). Anything else is a v0-OUT diagnostic.

**Output:** a signed `.proof` file containing
- one property memento (the lifted forall-precondition),
- one bridge memento (linking the function symbol to the property).

## Acceptance criteria

1. End-to-end on parseInt: starting from a raw `parseInt.ts` (no
   `must()`, no `forall()`), `provekit lift parseInt.ts` produces a
   `.proof` file.
2. The minted property memento has propertyHash `8c38f05152707736`,
   matching the hand-authored fixture at
   `scripts/output/parseInt-mementos/parseIntPreservesNonNegativeIntegers.json`.
   Same propertyHash → CID-equivalence at the IR-formula level.
3. The minted `.proof` round-trips through TS, Go, and C++ verifiers
   (the existing cross-lang harness).
4. Failure modes are loud and named:
   - `non-primitive-surface`: the function takes/returns something
     other than number/string/boolean.
   - `no-tests-cover-this-surface`: Filter cannot discriminate
     candidates because the file has no test coverage.
   - `all-candidates-dropped`: every LLM proposal was rejected by
     Filter (every candidate over-constrains a passing test).
   - `multiple-exports`: the file has more than one exported function.

## Architecture: lift is one more adapter, not a new system

`provekit lift` is a new top-level subcommand in `cli.ts`, peer to
`verify` / `mint` / `dump`. It dispatches to a single adapter today
(the TS-primitive adapter), but the dispatch shape leaves room for
Rust / Go / Python adapters later. The adapter contract is:

```typescript
interface LiftAdapter {
  /** Score 0..1 of how well this adapter handles a given file. */
  detectScore(filePath: string): number;
  /** Run the full five-stage pipeline and return a path to the .proof. */
  liftToProof(input: LiftInput): Promise<LiftResult>;
}
```

The dispatcher picks the highest-scoring adapter, refusing if the top
score is below a threshold. v0 ships only `tsPrimitiveAdapter` whose
score is `1.0` for files matching `*.ts` containing exactly one
exported function with primitive arg/return types, `0.0` otherwise.
The shape mirrors the kit-discovery pattern at
`src/ir/extensions/kitDiscovery.ts`: score, then load, then run.

## Five-stage pipeline

```
       parseInt.ts
            |
       [ Detect ]   <-- TS AST -> IR sort. Refuses non-primitive types.
            |
       FunctionShape { name, paramSorts, returnSort, source }
            |
       [ Propose ]  <-- LLM call w/ intake prompt. Body candidates only.
            |
       Candidate[] { quantifierShape (FIXED), predicateBody (LLM) }
            |
       [ Filter ]   <-- run package's tests. Drop over-strict candidates.
            |
       Candidate[] (survivors)
            |
       [ Review ]   <-- CLI y/n/edit/none for each survivor.
            |
       Accepted candidates
            |
       [ Mint ]     <-- existing claim-envelope + proof-envelope path.
            |
       parseInt.proof
```

### Stage 1: Detect

- Build a `ts.Program` from the input file.
- Find exactly one exported function declaration.
- Read each parameter's declared type via `ts.TypeChecker`. Map:
  - `number` → `{ kind: "primitive", name: "Int" }`
  - `string` → `{ kind: "primitive", name: "String" }`
  - `boolean` → `{ kind: "primitive", name: "Bool" }`
- Anything else (`Date`, `Array<T>`, generics, unions, intersections,
  interfaces) → emit `non-primitive-surface` and refuse.
- Return `FunctionShape { name, paramNames[], paramSorts[], returnSort, sourceText }`.

Note: this is **not** the same as `src/ir/lift/sorts.ts::resolveSort`,
which requires `__sort` brands. v0 lift accepts raw primitive TS types
because the input is real source code, not `.invariant.ts`. Sort
resolution lives at `src/proveLift/detectSort.ts`.

### Stage 2: Propose

The LLM receives the intake prompt at
`src/proveLift/prompts/intake.md` (editable prose). The prompt is
load-bearing: it must enforce that the LLM ONLY fills the predicate
body, never the quantifier shape. The shape is forced by the function
shape detected in stage 1.

**Quantifier-shape rule (v0):** the binder sort is the function's
RETURN sort, not a parameter sort. Lift expresses "what must hold of
the function's output." Quantifying over the parameter sort (the
obvious-but-wrong rule) makes the parseInt CID-equivalence acceptance
gate impossible: the hand-authored fixture quantifies over Int and
reaches String via the `String(n)` coercion in the body. So for
parseInt with shape `parseInt(s: string) -> number`, the scaffold is:

```
forall n: Int.
  <PREDICATE_BODY_OVER_n_AND_parseInt>
```

The legal-sort universe surfaced to the LLM is the union of return
sort and parameter sorts. The body may invoke kit-registered
coercions (`String(n)`, `Number(s)`, `Boolean(x)`) to bridge between
sorts. v0 scaffold picks the return sort as the binder; run-2 may
generalize to "LLM proposes binder sort, Detect constrains to the
legal universe" (Open Question 4).

The LLM is asked: *what must be true of `n` for `parseInt(String(n))`
to round-trip?* It returns candidate body strings, e.g.:

- `n >= 0 -> parseInt(String(n)) === n`   (the hand-authored answer)
- `n > 0 -> parseInt(String(n)) === n`    (over-strict)
- `parseInt(String(n)) === n`             (under-strict; false at n = -1.5)

The intake prompt's job is to surface 3-5 candidates so Filter has
something to work with. Output is JSON: an array of `{body, rationale}`
strings. The framework substitutes them into the fixed shape.

### Stage 3: Filter (the test-suite oracle)

For each candidate, the framework asks: *if this precondition were
real, would the package's existing tests violate it?* If yes, the
candidate is over-strict and dropped.

Mechanism, v0:
1. Discover tests via the project's `vitest`/`jest` runner.
2. For each test that calls the target function, extract the actual
   argument values from the test source (regex on
   `targetFunction("...")` or `targetFunction(42)`).
3. Substitute the test's argument into the candidate's antecedent.
   If the antecedent evaluates to `false` (in plain JS) but the test
   passes, the candidate is over-strict: drop it.
4. If no tests exercise the function, fail loudly with
   `no-tests-cover-this-surface`.

This is **not** symbolic execution. It is a tiny consistency check: the
test corpus is the ground truth for "valid input the function accepts;"
any candidate whose antecedent says those inputs are illegal is wrong.
Symbolic execution is on the v1 roadmap, not v0.

### Stage 4: Review

CLI presents each surviving candidate:

```
[lift] Proposed precondition for parseInt(s: string):

  forall n: Int.
    n >= 0 -> parseInt(String(n)) === n

  Rationale: parseInt only round-trips on non-negative integers; the
  test parseInt("0") === 0 passes, parseInt("-1") === -1 also passes
  in plain JS but the property's intent is round-trip on natural nums.

  Survives 8 of 8 tests.

  [a]ccept  [r]eject  [e]dit  [n]one of these  [q]uit
```

`edit` opens `$EDITOR` on the candidate; the edited body is re-Filtered.
`none` skips this function with no proof minted.

### Stage 5: Mint

Accepted candidates flow through:
1. `src/ir/lift/index.ts::liftFormulaExpression` to project the surface
   `forall(...)` body into a real `IrFormula`.
2. `src/canonicalizer/canonicalize.ts::propertyHashFromFormula` for the
   propertyHash.
3. `src/claimEnvelope/mint.ts::mintMemento` for the property memento.
4. `mintBridge` for the bridge memento (linking the function symbol
   to the property's CID).
5. `src/proofEnvelope/index.ts::buildProofEnvelope` to compose into a
   single `.proof` file at `<input>.proof`.

The minter and envelope builder are pre-existing; lift adds nothing
new at this layer. CID-equivalence falls out of the canonicalizer
producing the same bytes for the same logical formula.

## The intake prompt as editable prose

Per the abilities-first-runtime pattern: the prompt is a `.md` file
the user can edit without recompiling the framework. It is
load-bearing because it teaches the LLM to refuse non-primitive
surfaces, to produce JUST predicate bodies, and to surface multiple
candidates rather than one confident answer.

The prompt lives at `src/proveLift/prompts/intake.md`. The adapter
loads it, performs `{{}}`-style variable substitution, and submits.
There is no compiled template: the file IS the contract.

## Error modes (loud, never silent)

| Mode | Condition | Exit code |
|---|---|---|
| `non-primitive-surface` | param/return type not in {number, string, boolean} | 2 |
| `multiple-exports` | more than one exported function in the file | 2 |
| `no-exports` | zero exported functions in the file | 2 |
| `no-tests-cover-this-surface` | Filter found no test exercising the function | 3 |
| `all-candidates-dropped` | every LLM candidate was over-strict | 4 |
| `llm-unavailable` | no `LiftLLM` provider configured | 5 |
| `signing-key-missing` | no `PROVEKIT_KEY` env var or `--key` flag | 6 |

Every diagnostic includes the function name, file path, line number,
and the specific reason. No silent skip. No best-effort defaults.

## Non-goals (v0)

- **Symbolic execution.** Filter is the test corpus, not a solver.
- **Multi-export files.** One function per file. Splitter is v1.
- **Non-primitive types.** No arrays, tuples, objects, generics,
  unions. v1.
- **Existential properties** (`exists`). Forall only. v1.
- **Cross-function invariants.** A precondition involving two
  functions is v1 work.
- **Invariants beyond preconditions.** Postconditions, frame
  conditions, termination: v2+.
- **Auto-installing the proof in `node_modules`.** The user invokes
  `provekit lift`, gets a file, decides what to do with it.
- **Mutating the source.** Lift does not write `must()` calls back
  into `parseInt.ts`. The proof is a sidecar.

## Open questions

1. **CID equivalence is a hard claim.** The hand-authored memento at
   `scripts/output/parseInt-mementos/parseIntPreservesNonNegativeIntegers.json`
   has propertyHash `8c38f05152707736`. The minted memento's hash must
   match exactly. The risk: the lifter's projection of
   `n >= 0 -> parseInt(String(n)) === n` may not produce the same
   `IrFormula` bytes the hand-authored fixture encodes, because the
   fixture uses `≥` (the unicode atom) and a specific `Ctor` shape
   for `String(n)`. Verification gate: write a unit test in v0 that
   asserts CID match before claiming the criterion met.

2. **Detect-scored adapter pattern.** The prompt says lift is "one
   more adapter" alongside an existing detect-scored dispatcher. The
   closest existing pattern is `kitDiscovery` (per-package, not
   per-file) and the `cli.ts` switch (string-match, not score-based).
   v0 ships its own `LiftAdapterRegistry` rather than retrofitting
   either. When a second adapter (Rust, Go) lands, the registry can be
   promoted to a shared dispatcher.

3. **`prompts/intake.md` packaging.** `tsc` only emits `.js`/`.d.ts`,
   so the editable prompt file is missing from `dist/`. Vitest runs
   from source and hides this. Run-2 must either (a) copy non-TS
   assets to dist via a build step, (b) read from a packaged location
   resolvable at runtime (`require.resolve` then walk up to source),
   or (c) inline the prompt as a TS string export. Option (a) keeps
   the "editable prose" promise intact for installed consumers.

4. **LLM provider injection.** ProvekIt has no LLM client today (the
   `formulate` stage's LLM is in `src/fix/stages/formulateInvariant.ts`
   and is owned by the bug-fix workflow). v0 lift takes a `LiftLLM`
   interface as a constructor arg; the CLI wiring uses an
   environment-driven provider selection. Default v0 binding: no LLM
   wired, Propose returns a stub list, full pipeline runs end-to-end on
   the stub. Real LLM wiring is the next-run task.

## Scope of THIS run (the design + scaffold landing)

- Design doc: this file.
- Scaffolded adapter under `implementations/typescript/src/proveLift/`:
  - `index.ts`: public API + `LiftAdapter` registry.
  - `tsPrimitiveAdapter.ts`: the v0 adapter.
  - `detect.ts` + `detectSort.ts`: Detect stage (real, working).
  - `propose.ts`: Propose stage (stub: returns hardcoded candidates).
  - `filter.ts`: Filter stage (stub: passes all candidates).
  - `review.ts`: Review stage (stub: accepts the first).
  - `mint.ts`: Mint stage (stub: throws).
  - `prompts/intake.md`: editable LLM intake prompt.
  - `errors.ts`: typed diagnostic shapes.
  - `__fixtures__/parseInt.ts`: raw input fixture.
  - `tsPrimitiveAdapter.test.ts`: failing tests, TDD shape.
- CLI wiring: `cli.ts` gets a `case "lift"` arm calling the new
  adapter registry. (NOT in this run; next-run.)

## Cut list (out of scope this run)

- No real LLM call. Stubbed.
- No real test discovery. Stubbed.
- No real mint. Stubbed (the integration lives one run later).
- No CLI wiring. Pipeline lives as library API, callable from tests.
- No CID-equivalence assertion test. That gates run 2's mint work.
- No round-trip-through-Go-and-C++ verification. That gates run 3.

## What's next (run 2)

1. Wire the LLM client (decide: reuse `src/fix/stages` plumbing or
   inject fresh).
2. Implement Filter against the actual project's vitest config.
3. Implement Mint by composing existing primitives.
4. CID-equivalence test against the parseInt fixture. This is the
   acceptance gate.
5. CLI wiring.
6. Cross-language round-trip test (TS authors, Go + C++ verify).

## Project conventions

- pnpm, single conventional commit.
- No emojis, no em-dashes in code or comments.
- Tests live next to source as `*.test.ts`.
- Per Sir's preference: jj over git when both work. Worktree-isolated.
- Comments only when WHY is non-obvious.
