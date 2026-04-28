# ProvekIt: The Kit to Prove It's Fixed.

**ProvekIt is the fourth horseman of the git commit: tsc, lint, test, prove.** Every developer adds it to their CI. The job runs `provekit invariants verify --ci`, the standing-invariant gate fails on violations, and branch protection blocks the merge — the same shape as the three horsemen the industry already trusts. No bespoke integration, no separate dashboard, no event-bus subscriptions.

ProvekIt turns a bug report (or a landed commit) into a mechanically-verified bundle: patch, regression test, formal principle, optional substrate extension. Every stage is gated by mechanical oracles. No "LLM said it works."

> **Landing in this repo for the first time?** Read [docs/QUICKSTART.md](docs/QUICKSTART.md) for a greenfield-project walkthrough, including which modes are production-ready vs. research-only. The architectural pitch is in [`docs/specs/2026-04-27-constraint-driven-development.md`](docs/specs/2026-04-27-constraint-driven-development.md).

## What it does

A bug report goes in. A verified bundle comes out.

The bundle contains the code patch, a regression test that passes on the fix and fails on the original, a DSL principle that names the bug class, and (when the bug shape requires it) a new SAST capability that extends the substrate so the same class of bug can be detected everywhere.

18 oracles gate every stage of production. Oracles check invariant satisfiability under Z3, regression test pass/fail, SAST structural coherence, full-suite vitest pass, gap closure, migration safety, extractor coverage, and more. A stage that cannot clear its oracles stops the pipeline. The bundle is not assembled until all applicable oracles pass.

The result is not a suggested edit. It is a commit-ready artifact with a machine-readable audit trail showing exactly which oracles fired and what they verified.

## Why it's different

- Not a linter. Linters flag patterns. ProvekIt closes bugs: it formulates a formal invariant, generates a fix that satisfies it under Z3, and refuses to ship a patch that does not.
- Not a codegen tool. Codegen proposes. ProvekIt verifies. Every artifact in the bundle cleared a mechanical gate before it landed there.
- Not a coding agent with guardrails. The LLM tier is calibrated per stage (haiku tolerable for intake parsing, sonnet for classification, opus for invariant formulation), and the architecture's contribution is bounding what "competent" has to mean: small structured output per stage, not "understand the whole codebase." The oracles are the load-bearing claim. The pipeline is the product.
- Captures institutional knowledge. Every applied bundle updates the principles library and the SAST substrate. The system is strictly smarter after every fix.
- Substrate self-extends. When a bug shape cannot be expressed in the current DSL, ProvekIt proposes a new capability, gates it through oracles 14-18, and lands it atomically with the fix. The floor rises with each gap closed.
- Compounds. Remaining capability gaps are dogfood fuel. Each one that closes adds a new detection column to the substrate that runs on every future analysis.

## From bug report to verified bundle

On 2026-04-24, the full pipeline ran end-to-end on a real bug using Claude Agent SDK with Opus 4.7. The input was a TypeScript file containing `function divide(a, b) { return a / b; }` and a prose bug report. ProvekIt produced a verified patch, a mutation-verified vitest regression test, and a PR body. All stages (Intake through D2) executed. Z3 and the runtime harness both cleared their oracles.

The patch:

```diff
 export function divide(a: number, b: number): number {
+  if (b === 0) {
+    throw new Error("Division by zero: denominator must be non-zero");
+  }
   return a / b;
 }
```

The regression test encoded the Z3 witness directly (`const b = 0; const a = 1`), tolerated either fix shape (throw or return finite), and exhaustively rejected `Infinity`, `NaN`, and `-Infinity`. The PR body was auto-generated and written to the working directory in `prDraft` mode.

## Quick start

ProvekIt ships through three install paths. Pick the one that matches your situation.

### 1. New project — install the CLI and bootstrap

```bash
npm install -D provekit
npx provekit init
```

`provekit init` is idempotent. It creates `.provekit/`, applies migrations, seeds the principle library into `.provekit/principles/`, installs a pre-commit hook that runs `provekit invariants verify --ci`, and scaffolds `.github/workflows/provekit.yml`. Re-running it skips work that has already been done.

After init, the typical loop is:

```bash
npx provekit invariants verify   # standing-runtime gate — Z3 only, no LLM, no network
npx provekit lint                # principle library across the codebase
npx provekit fix bug-report.md   # run the full pipeline on a bug
```

### 2. Existing project with CI — drop in the GitHub Action

Copy [`.github/workflows/provekit-example.yml`](.github/workflows/provekit-example.yml) into your repo, or reference the bundled action directly:

```yaml
name: ProvekIt
on: [push, pull_request]
jobs:
  prove:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - uses: provekit/provekit/.github/actions/provekit-verify@main
```

See [`.github/actions/provekit-verify/README.md`](.github/actions/provekit-verify/README.md) for the action's inputs and outputs (including `summary-md` for PR-comment integration).

### 3. As a library — IDE / agent runtime / platform integration

```ts
import { verifyAll, readInvariants, writeInvariant } from "provekit";

const report = await verifyAll(projectRoot);
if (report.summary.violated > 0) {
  // block the agent's tool call, refuse the apply, surface in the IDE...
}
```

The library surface is Channel 2 of the distribution surface in [`docs/specs/2026-04-27-standing-invariant-runtime.md`](docs/specs/2026-04-27-standing-invariant-runtime.md). Holyship integrates ProvekIt at the agent's `report` boundary; the same surface is what an IDE or other agent runtime targets.

## Architecture at a glance

The pipeline runs in nine stages: Intake parses the bug signal, Locate finds the SAST locus, Classify picks the remediation layer, then C1-C6 produce the invariant, overlay, fix candidate, complementary changes, regression test, and principle candidate (with optional substrate extension). D1 assembles and verifies the bundle via the full 18-oracle suite. D2 applies transactionally. D3 updates the principles library and capability registry. See [ARCHITECTURE.md](./ARCHITECTURE.md) for a full walkthrough.

## Pitch leaks closed

The pitch had six honest cracks (`docs/plans/2026-04-25-pitch-leaks.md`). Three are closed:

- **Invariant fidelity (Leak 1).** Oracle 1.5 runs cross-LLM derivation agreement, prose-to-clause traceability, and adversarial-fixture pre-validation before C1 returns, with adaptive routing for taint-style versus arithmetic invariants. Underwrites: Z3 is checking an invariant grounded in the bug report, not one the LLM wrote in a vacuum.
- **Loop seams (Leak 4).** 211-scenario corpus across fast-check, SemGrep, Stryker, and a BugsJS skeleton. Integration-gap rate at 0%. Underwrites: the pipeline holds across realistic bug surface, not one happy path.
- **Hard-bug existence proof (Leak 2).** Real-LLM run on shell-injection produced a `taintSource` capability plus a `no_unsanitized_shell_exec` principle, and a separate run auto-applied an `execFileSync` argv-form fix with regression test. Required chained data-flow as substrate prereq. Underwrites: the substrate-extension path handles non-trivial bug classes, not just syntactic ones.

## Status

See [RETROSPECTIVE.md](./RETROSPECTIVE.md) for what is built, what the dogfood proved, what the remaining capability gaps are, and what is deferred.

The historical implementation plan lives at [docs/plans/2026-04-23-fix-loop.md](./docs/plans/2026-04-23-fix-loop.md).

## Documentation

- [ARCHITECTURE.md](./ARCHITECTURE.md): pipeline walkthrough, all five registries, 18 oracles, overlay isolation, logging architecture.
- [RETROSPECTIVE.md](./RETROSPECTIVE.md): what shipped, what the dogfood proved, remaining gaps, deferred work.
- [docs/LOGGING.md](./docs/LOGGING.md): logging conventions, the no-truncation rule, and the rationale behind it.
