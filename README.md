# ProveKit: The Kit to Prove It's Fixed.

ProveKit turns a bug report into a mechanically-verified fix bundle: patch, regression test, formal principle, and optional substrate extension. Every stage is gated by mechanical oracles. No "LLM said it works."

## What it does

A bug report goes in. A verified bundle comes out.

The bundle contains the code patch, a regression test that passes on the fix and fails on the original, a DSL principle that names the bug class, and (when the bug shape requires it) a new SAST capability that extends the substrate so the same class of bug can be detected everywhere.

18 oracles gate every stage of production. Oracles check invariant satisfiability under Z3, regression test pass/fail, SAST structural coherence, full-suite vitest pass, gap closure, migration safety, extractor coverage, and more. A stage that cannot clear its oracles stops the pipeline. The bundle is not assembled until all applicable oracles pass.

The result is not a suggested edit. It is a commit-ready artifact with a machine-readable audit trail showing exactly which oracles fired and what they verified.

## Why it's different

- Not a linter. Linters flag patterns. ProveKit closes bugs: it formulates a formal invariant, generates a fix that satisfies it under Z3, and refuses to ship a patch that does not.
- Not a codegen tool. Codegen proposes. ProveKit verifies. Every artifact in the bundle cleared a mechanical gate before it landed there.
- Not a coding agent with guardrails. The LLM is fungible at every stage boundary. The oracles are not. The pipeline is the product.
- Captures institutional knowledge. Every applied bundle updates the principles library and the SAST substrate. The system is strictly smarter after every fix.
- Substrate self-extends. When a bug shape cannot be expressed in the current DSL, ProveKit proposes a new capability, gates it through oracles 14-18, and lands it atomically with the fix. The floor rises with each gap closed.
- Compounds. Eight remaining capability gaps are dogfood fuel. Each one that closes adds a new detection column to the substrate that runs on every future analysis.

## Quick start

```bash
npm install
provekit init             # scan codebase, build SAST index, wire commit hook
provekit analyze          # find proven clauses and gap violations across the tree
provekit fix gap_report:42             # close a specific gap report
provekit fix bug-report.md --apply     # run fix loop and apply autonomously
```

The `--apply` flag cherry-picks the resulting commit onto the target branch. Without it, ProveKit writes a patch file and PR draft to the working directory for human review.

## Architecture at a glance

The pipeline runs in nine stages: Intake parses the bug signal, Locate finds the SAST locus, Classify picks the remediation layer, then C1-C6 produce the invariant, overlay, fix candidate, complementary changes, regression test, and principle candidate (with optional substrate extension). D1 assembles and verifies the bundle via the full 18-oracle suite. D2 applies transactionally. D3 updates the principles library and capability registry. See [ARCHITECTURE.md](./ARCHITECTURE.md) for a full walkthrough.

## Status

See [RETROSPECTIVE.md](./RETROSPECTIVE.md) for what is built, what the dogfood proved, what the eight remaining capability gaps are, and what is deferred.

The historical implementation plan lives at [docs/plans/2026-04-23-fix-loop.md](./docs/plans/2026-04-23-fix-loop.md).
