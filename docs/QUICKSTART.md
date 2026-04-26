# Quickstart: ProveKit on a greenfield project

A practical walkthrough for landing ProveKit in a new TypeScript/JavaScript repo and running it against actual bugs. Honest about what's production-ready, what's mostly-there, and what's research.

**Reading order:** prerequisites → install → init → the three modes you'll actually use → CI integration → known limits.

---

## Prerequisites

- Node ≥ 18
- git (the target project must be a git repo)
- Optional: `ANTHROPIC_API_KEY` exported, only required for the fix loop (modes 2-3 below)

## Install

ProveKit isn't on npm yet, and the production build (`npm run build && npm link`) is currently broken pending a CJS/ESM module-config refactor. Until that lands, run the CLI directly via `tsx`:

```bash
git clone <provekit-repo>
cd provekit
npm install
# DON'T run npm run build — it currently fails on import.meta + module=commonjs mismatch (tracked separately)
```

Then either alias the CLI:

```bash
# Add to ~/.bashrc or ~/.zshrc
alias provekit="npx tsx /path/to/provekit/src/cli.ts"
```

Or invoke explicitly per command:

```bash
npx tsx /path/to/provekit/src/cli.ts --version       # → provekit v0.3.0
```

The walkthrough below assumes you've aliased it as `provekit`.

For commands that take a project path (`init`, `lint`), pass the target project as the positional arg. For `analyze`, pass the file path.

---

## Step 1: init (only needed for `analyze` / `fix` — `lint` works without it)

```bash
provekit init /path/to/your/greenfield/repo --no-hook
```

What this does:
- Verifies the target is a git repo (errors if not)
- Scans for signals in your code (TODO/FIXME comments, log levels, function-name patterns) — these become the input set for the `analyze` phase
- Without `--no-hook`, installs a pre-push git hook that runs `provekit verify` before every push

Output is written to `.provekit/` at the target repo root:
- `.provekit/provekit.db` — SQLite store for SAST + signals + gap reports + fix bundle history
- `.provekit/contexts/`, `.provekit/contracts/`, `.provekit/observations/` — analyze-pipeline artifacts
- `.provekit/graph.json`, `.provekit/derivation.json`, `.provekit/report.json` — analyze output

**Note:** `init` does NOT seed `.provekit/principles/` into your project. The principle library lives with the ProveKit package; `provekit lint` falls back to that location when your project has no local override. To customize: copy `.provekit/principles/` from the ProveKit checkout into your target project.

Add this to your target project's `.gitignore`:

```gitignore
.provekit/provekit.db
.provekit/provekit.db-shm
.provekit/provekit.db-wal
.provekit/harvest/staging/
.provekit/harvest/harvest.db
.provekit/harvest/harvest.db-shm
.provekit/harvest/harvest.db-wal
.provekit/fix-loop-*.log
.provekit/fuzz-runs/
```

---

## Step 2: lint (Mode 1 — static analysis with the principle library)

```bash
provekit lint /path/to/your/project          # full project walk
provekit lint /path/to/your/project --ci     # exit 1 on any violation (for CI)
provekit lint /path/to/your/project -v       # verbose; surfaces parser/principle errors
```

What this does (no LLM, no cost, seconds for small projects):
1. Walks every `.ts/.tsx/.mts/.cts` file under the project root (skipping `node_modules`, `dist`, `.git`, `test/`, `__tests__/`)
2. Builds the SAST index in a scratch DB
3. Runs every DSL principle in `.provekit/principles/` (or falls back to the principles bundled with the ProveKit package if the project hasn't been seeded yet)
4. Prints one line per match: `<file>:<line>  [<severity>]  <principle>: <message>`
5. Summarizes: files indexed, principles evaluated, total matches by severity

Example output on a 1-file scratch project:

```
src/divide.ts:2  [violation]  division-by-zero: division denominator may be zero

Files indexed: 1/1
Principles evaluated: 15/15
Matches: 1 (1 violations, 0 warnings, 0 info)
```

The principle library (≈ 15 DSL files in `.provekit/principles/`) covers a small, opinionated set of bug classes:

| Principle | Catches |
|---|---|
| `division-by-zero` | `a / b` where `b` lacks a prior `=== 0` check |
| `falsy-default` | `param \|\| default` where `param` flows from a function parameter (silently discards `0`, `""`, `false`) |
| `addition-overflow` | `+` arithmetic, suppressed if a `< literal` upper-bound check exists for the LHS |
| `subtraction-underflow` | `-` arithmetic, mirror of addition |
| `loop-accumulator-overflow` | `+=` inside a `for` loop body |
| `throw-uncaught` | `throw` not enclosed by try/catch |
| `multiplication-overflow` | `*` arithmetic |
| `empty-collection-loop` | `for-of` over a possibly-empty collection |
| `unguarded-await` | `await` in a path with no try/catch |
| `find-undefined-result` | `.find()` result used without an existence check |
| `match-null-result` | `.match()` result used without null-check |
| `null-assertion` | `!.` non-null assertion that the SAST can't prove |
| `reduce-no-initial` | `.reduce(...)` without an initial value (throws on empty arrays) |
| `split-empty-string` | `.split("")` (probably wrong, usually meant chars) |
| `modulo-by-zero` | `a % b` where `b` lacks a prior `=== 0` check |

The library has known noise floors (some principles are over-broad — see `docs/plans/2026-04-26-principle-tightening.md`). Tightening is ongoing.

To customize: copy `.provekit/principles/*.dsl` from the ProveKit package into your project's `.provekit/principles/` and edit. The `lint` command prefers the project's local copies when present.

### A second pipeline: `provekit analyze`

Note that `provekit analyze <file.ts>` is a **different** machinery — it runs Z3-based contract verification per file (Phases 1-5), not the principle library. Both are useful:
- `lint` for fast principle-library scanning across many files
- `analyze` for deep per-file Z3 contract proof

CI typically wires `lint --ci`. `analyze` is for development.

---

## Step 3: fix loop in prDraft mode (Mode 2 — mostly tested)

```bash
provekit fix gap_report:42                  # close a finding from the analyze stage
provekit fix bug-report.md --no-confirm     # close a human-written bug report
provekit fix - < some-failure.txt           # read from stdin
```

What happens (this calls Claude Opus, costs roughly $0.50-3 per run, takes 2-10 minutes):
1. **Intake**: parses the input via the matching adapter (`gap_report:N`, file path, plain text, GitHub issue URL, etc.)
2. **Locate**: finds the SAST node where the bug lives
3. **Classify**: picks the remediation layer (code patch vs. substrate extension)
4. **C1-C5**: LLM formulates a formal invariant, opens an overlay on a scratch worktree, proposes a fix, computes complementary sites, generates a regression test
5. **C6**: tries to express the bug class as a DSL principle for the library. Optionally proposes a new SAST capability if existing ones can't capture the shape (substrate-extension path)
6. **D1**: assembles the fix bundle and runs all 18 oracles (Z3 satisfiability, regression test pass/fail, full vitest suite, SAST coherence, etc.). The pipeline halts on any oracle failure.
7. **D2** (default — prDraft mode): writes `patch.diff` and `pr-body.md` to the working directory. **Does not modify your branch.**

Review the patch by hand. If it looks right:

```bash
git apply patch.diff
git add -p             # review hunk-by-hunk
git commit -m "fix: <summary>"
```

This mode is the right default for production use today. The apply mechanics have 23 unit + 4 integration tests. The end-to-end with real Claude has high test coverage on every component but has not been smoked end-to-end on a fresh bug from intake through cherry-pick. **Treat the LLM as a peer who proposes patches; review them.**

### Bug-report format (for `provekit fix bug-report.md`)

There is no rigid schema. The `report` intake adapter parses any prose via an LLM into structured fields (summary, failure description, code references). A useful template:

```markdown
# Bug: division by zero in calc.ts

When `divide(1, 0)` is called, the function returns `Infinity` instead of throwing.

The expected behavior is that division by zero should throw with a clear error message.

Found at `src/calc.ts:5` in the `divide()` function. Reproduces with the test
`divide-by-zero.test.ts:14`.

## Expected vs actual
- Expected: `divide(1, 0)` throws `Error("Division by zero")`
- Actual: returns `Infinity` (JavaScript's IEEE 754 behavior)

## Notes
The fix should add a denominator-zero check before the division.
```

The narrower the file/line/function pointer, the better the locate stage performs.

---

## Step 4: fix loop with `--apply` (Mode 3 — research, NOT recommended for production yet)

```bash
provekit fix bug-report.md --apply --verbose
```

Same pipeline as Mode 2, but D2 cherry-picks the commit onto your target branch (the branch you have checked out, or one passed via `--target-branch <name>`).

**Why we say "not recommended":** the apply machinery (`src/fix/apply.ts`, `src/fix/stages/applyBundle.ts`) is well-tested with synthetic bundles. But a real-LLM end-to-end dogfood from intake → C1-C6 → bundle → cherry-pick has never run as a full session. The shape of a real Claude-produced bundle could violate assumptions the synthetic-bundle tests don't exercise (absolute vs. relative paths in `patch.fileEdits[].file`, dialect mismatches in `cap.migrationSql`, import-path resolution in `cap.extractorTs`). See `docs/plans/2026-04-25-production-readiness.md` task P2.

**What to do instead:** stay in Mode 2 (prDraft). Open a PR from the patch. CI runs the regression test. You merge by hand.

---

## Step 5: substrate self-extension (research, NOT production)

When the principle library can't express the bug class, the C6 stage can propose a new SAST capability (a new column or table that lets a principle catch the bug). Oracles 14-18 verify the capability proposal before the fix bundle is assembled.

This works in StubLLM tests (`src/fix/dogfood.empty-catch.test.ts`, `src/fix/dogfood.shell-injection.test.ts`) but has never closed end-to-end with real Claude. **Treat any "I added a new capability" output as a research artifact.** If you're feeling adventurous: capture the proposed migration SQL + extractor TS, review them by hand, ship them as a hand-written commit.

---

## CI integration

Minimal GitHub Actions stub for Mode 1 (`provekit lint` on PRs):

```yaml
# .github/workflows/provekit.yml
name: ProveKit lint

on:
  pull_request:
    branches: [main]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '20' }

      # Install ProveKit (until npm publication, this is a tarball / git URL)
      - run: |
          git clone https://github.com/<your-org>/provekit /tmp/provekit
          cd /tmp/provekit && npm ci
          # Until the build path is fixed, run via tsx
          echo 'alias provekit="npx tsx /tmp/provekit/src/cli.ts"' >> ~/.bashrc

      - run: npx tsx /tmp/provekit/src/cli.ts lint . --ci
```

The `--ci` flag exits non-zero on any violation. Lint runs against the bundled principle library by default (no `provekit init` required for lint-only flows).

Mode 2 (fix loop) is too slow + costs API credits per PR; don't run it from CI by default. If you want fix-loop integration, gate it on a `provekit fix` label or a manual workflow_dispatch.

---

## Configuration

`.provekit/principles/<name>.dsl` — DSL files defining a principle. Each is a self-contained text file. To add a principle:

1. Write the DSL (read existing principles for the pattern)
2. Drop it in `.provekit/principles/`
3. Re-run `provekit analyze`

To disable a principle: delete the `.dsl` file (the matching `.json` metadata is metadata-only and won't fire on its own).

`.provekit/principles/<name>.json` — metadata for the principle (description, smt2 templates, provenance). Source-controlled alongside the DSL.

The DSL grammar lives in `src/dsl/parser.ts`; the relations in `src/dsl/relations.ts`; the capabilities in `src/sast/schema/capabilities/`. See `docs/specs/2026-04-23-provekit-v2-design.md` for full design.

---

## Known limits

- **TypeScript only.** ProveKit uses ts-morph; non-TS files are ignored. JSX/TSX work; pure JS works if it parses with the TypeScript compiler in loose mode.
- **No incremental analyze.** Every run rebuilds the full SAST. Fast for small projects, painful for large monorepos. Use `provekit derive` (diff-only mode) for faster local feedback.
- **Principle library has noise.** The 15 principles cover a slice of bug-shapes; some over-fire. The harvest pipeline (`docs/plans/2026-04-25-bugsjs-harvest.md`) is mining real-bug corpora to bootstrap more principles, but mining hit an expressiveness ceiling on diff-relative bug signals (see `docs/plans/2026-04-26-principle-tightening.md` for current state).
- **`--apply` not validated end-to-end with real LLM.** Use prDraft + manual review.
- **Substrate self-extension is research.** Don't expect it to close arbitrary bug classes on demand.
- **Bug-1 v22 hung in stub-mode.** The substrate's range is bounded; the fix loop can stall on hard cases. Have a manual-merge fallback.

---

## Troubleshooting

**`Error: No database found at .provekit/provekit.db`**
You haven't run `provekit init` or `provekit analyze` in this project. Run `provekit init` then `provekit analyze`.

**`Error: Not a git repository`**
ProveKit needs a git repo. `git init && git add . && git commit -m "init"` then re-run.

**`Cherry-pick conflict` during `provekit fix --apply`**
The bundle's commit didn't apply cleanly to your target branch (branch advanced since the worktree was created, or the patch overlaps with concurrent edits). The apply path aborts the cherry-pick and the helper worktree is cleaned up. Re-run `provekit fix` or apply by hand from the prDraft.

**`Parse error in DSL` after editing a principle**
Run `npx vitest run src/dsl/` to catch parser errors. The DSL grammar is documented at the top of `src/dsl/ast.ts`.

**LLM calls timing out or returning shape errors**
Check `ANTHROPIC_API_KEY` is set. The `--verbose` flag streams LLM reasoning; useful for diagnosing prompt/response shape issues. Logs land in `.provekit/fix-loop-*.log`.

---

## Where to go next

- **You want to add a bug class to the principle library:** read an existing `.dsl` file (start with `division-by-zero.dsl` — the cleanest example with the `same_value` relation), copy the pattern, write your own.
- **You want to use the fix loop in CI:** hold off until P2 (real-LLM end-to-end dogfood) closes. Stay in Mode 1 (analyze) for now.
- **You want to understand the architecture:** `ARCHITECTURE.md` walks the 9-stage pipeline; `THESIS.md` walks the philosophical claim.
- **You want to know what's deferred:** `RETROSPECTIVE.md`.
