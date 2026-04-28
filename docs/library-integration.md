# Library integration (Channel 2)

Audience: IDE authors, agent-runtime authors, and CI integrators who want to
embed ProvekIt as a programmable correctness gate. If you only want the
out-of-the-box CI check, use the `provekit` CLI plus the GitHub Action
(Channel 1) instead. This document is for the integrators who need more.

## 1. What this is

`provekit` exposes a stable TypeScript library surface alongside its CLI.
The library surface is a single module:

```ts
import {
  runFixLoop,
  verifyAll,
  verifyAllCached,
  extractIntent,
  readInvariants,
  writeInvariant,
  retireInvariant,
  buildStoredInvariant,
  hashInvariant,
  StubLLMProvider,
} from "provekit";

import type {
  BugSignal,
  BugLocus,
  RemediationPlan,
  FixLoopResult,
  VerifyReport,
  InvariantVerdict,
  CachedVerifyReport,
  StoredInvariant,
  IntentReport,
  LLMProvider,
  Path,
  PathVerdict,
} from "provekit";
```

Every name in `src/index.ts` is semver-committed: backward-compatible
within the 0.x major. Names that do NOT appear in `src/index.ts` are
internal and may break between minor versions without notice.

The four canonical entry points named in the design spec
(`docs/specs/2026-04-27-standing-invariant-runtime.md`, "Distribution
surface") are:

| Entry point        | What it does                                                     | Touches LLM? |
|--------------------|------------------------------------------------------------------|:------------:|
| `runFixLoop`       | Full pipeline: intake → invariant → patch → tests → bundle.      | Yes          |
| `verifyAll`        | Standing-runtime gate: re-checks stored invariants against code. | No           |
| `extractIntent`    | B0 retrospective intake: extracts intent from a code change.     | Yes          |
| `readInvariants` / `writeInvariant` | Constraint-store I/O.                           | No           |

`verifyAllCached` is a thin caching wrapper over `verifyAll` that you
should prefer in latency-sensitive contexts (IDE on-edit hooks, fast PR
checks).

### How the package ships (v1, pre-ESM-conversion)

`provekit` v0.x ships its library entry as a tsx-driven runtime shim,
not a precompiled `dist/`. Concretely:

- `package.json` `"main"` → `lib/provekit.cjs`, which calls
  `require("tsx/cjs/api").register()` then `require("../src/index.ts")`.
- `package.json` `"exports".import` → `lib/provekit.mjs`, which
  destructure-re-exports the value names so static ESM imports
  resolve.
- `package.json` `"types"` → `src/index.ts` directly. Consumers
  resolve types from TypeScript source.

This is the same strategy the CLI binary uses (channel 1) and unblocks
distribution without the full ESM conversion (a separate, larger task).
Two consumer-facing consequences worth knowing:

1. **`tsx` is a runtime dependency**, not a devDependency. It ships
   with `npm install provekit`. esbuild and get-tsconfig come along
   as tsx's own deps. (next, vite, vitest ship the same shape.)
2. **`"types"` points at `src/index.ts`.** Consumers using
   `moduleResolution: "node16" | "nodenext" | "bundler"` get full
   types out of the box. Older `node10` resolution may be flaky on
   the `.js`-style relative specifiers in the source. The full
   `.d.ts` emit will land alongside the ESM conversion.

## 2. Common integration shapes

### 2a. IDE on-edit hook

The lightest-weight integration. After a file save, run
`verifyAllCached` against the project root and surface failing
invariants as editor diagnostics. No LLM, no overlay, no patches.
Cached runs typically complete in well under the file-watch debounce
window once the cache is warm.

```ts
import { verifyAllCached } from "provekit";

async function onFileSave(projectRoot: string, savedFile: string) {
  const report = await verifyAllCached(projectRoot);

  for (const verdict of report.verdicts) {
    if (verdict.outcome === "violated") {
      // Map to your editor's diagnostic format.
      editor.publishDiagnostic({
        severity: "error",
        message: verdict.invariantId + ": " + verdict.summary,
        file: verdict.location?.file ?? savedFile,
        line: verdict.location?.line ?? 1,
      });
    } else if (verdict.outcome.startsWith("decayed_")) {
      // The binding decay alarm is a yellow, not a red.
      editor.publishDiagnostic({
        severity: "warning",
        message: "binding decay: " + verdict.outcome + " (" + verdict.invariantId + ")",
        file: verdict.location?.file ?? savedFile,
        line: verdict.location?.line ?? 1,
      });
    }
  }
}
```

The actual `InvariantVerdict` shape is exported as a type; consult its
fields for outcome enums (`resolved`, `violated`, `decayed_deleted`,
`decayed_changed`, `decayed_substrate`).

### 2b. Agent-runtime gate (Holyship-style)

The integration this surface was designed around. When an agent emits a
`report` (a typed bug-finding event), Holyship's gate library
constructs a `BugSignal`, locates the bug, builds a `RemediationPlan`,
and hands the whole thing to `runFixLoop`. The resulting `FixLoopResult`
flows back into the orchestrator, which decides whether to autoApply,
review, or escalate.

```ts
import {
  runFixLoop,
  type BugSignal,
  type BugLocus,
  type RemediationPlan,
  type LLMProvider,
  type FixLoopResult,
} from "provekit";
import { openDb } from "./db.js"; // your sqlite handle, see CLI for example

async function gateOnReport(
  report: AgentReport,
  llm: LLMProvider,
  projectRoot: string,
): Promise<FixLoopResult> {
  const db = openDb(projectRoot);

  const signal: BugSignal = {
    source: "holyship",
    rawText: report.rawText,
    summary: report.summary,
    failureDescription: report.failureDescription,
    codeReferences: report.references,
  };

  const locus: BugLocus = await holyshipLocate(db, signal);
  const plan: RemediationPlan = await holyshipPlan(signal, locus);

  return runFixLoop({
    signal,
    locus,
    plan,
    db,
    llm,
    options: {
      autoApply: false,        // Holyship gates manually.
      maxComplementarySites: 10,
      confidenceThreshold: 0.8,
    },
  });
}
```

The bundle returned in `FixLoopResult.bundle` is the unit of work
Holyship reviews. The `auditTrail` is the append-only log Holyship
ingests for forensics. ProvekIt does not write per-runtime adapters;
Holyship and any future agent runtime target the same surface above.

### 2c. CI custom integration

When the GitHub Action wrapper isn't sufficient (Buildkite plugin,
Jenkins shared library, GitLab CI custom job, internal CI runner), call
`verifyAllCached` directly and translate the report to your CI
system's check format.

```ts
import { verifyAllCached, exitCodeFor } from "provekit";

async function buildkitePluginEntry(projectRoot: string) {
  const report = await verifyAllCached(projectRoot);

  // Emit Buildkite annotations.
  for (const verdict of report.verdicts) {
    if (verdict.outcome === "violated") {
      console.log("--- buildkite annotation");
      console.log("error: " + verdict.invariantId + ". " + verdict.summary);
    }
  }

  // Honor ProvekIt's exit-code convention so CI fails the right way.
  process.exit(exitCodeFor(report));
}
```

`exitCodeFor` follows the same convention the CLI uses: 0 for clean,
1 for violated invariants, 2 for binding decay (architectural drift
the human needs to look at).

## 3. Type surface walkthrough

The type surface is partitioned by which entry point produces or
consumes each type. This section names every exported type once and
explains its role.

**Pipeline data flow.**
- `BugSignal`: normalized bug report. Produced by intake adapters
  (B1), consumed by `runFixLoop`.
- `BugLocus`: precise SAST-resolved location of the bug, including
  data-flow neighbors and dominance regions. Produced by B2, consumed
  by `runFixLoop` and downstream pipeline stages.
- `RemediationPlan`: planning container wrapping `BugSignal` +
  `BugLocus` plus layer assignments and proposed `PlannedArtifact`s.
  Produced by B3, required input to `runFixLoop`.
- `PlannedArtifact`, `ComplementaryChange`, `ComplementarySiteKind`:
  the artifact-level breakdown inside a `RemediationPlan`.
- `FixCandidate`, `TestArtifact`, `CodePatch`, `CodePatchFileEdit`:
  the products of pipeline stages C3 / C5. Carried inside `FixBundle`.
- `PrincipleCandidate`: what C6 emits, the candidate library
  principle the loop wants to learn from this fix.
- `FixBundle`: assembled bundle of patch + test + invariant +
  principle candidate. Output of D1.
- `FixLoopResult`: top-level return from `runFixLoop`. Includes a
  `FixBundle | null`, an `applied` flag, the `auditTrail`, and an
  optional `applyResult` when D2 ran.
- `AuditEntry`: append-only step records. Every stage writes one.
- `ApplyResult`, `PrDraftArtifacts`, `RollbackAudit`: D2's output
  shape, populated when `runFixLoop` actually applied a bundle.
- `OverlayHandle`: the C2 worktree handle. Pipeline-internal but
  exposed because injectable test runners take it as input.
- `LibraryPrinciple`, `FixTemplate`, `TestTemplate`, `CapabilitySpec`:
  the principle library shape (how mechanical-mode templates are
  declared).
- `CodeReference`, `BugProvenance`, `InvariantCitation`: fine-grained
  reference shapes used inside the structures above.

**LLM plumbing.**
- `LLMProvider`: the interface every integrator implements (or wires
  through to a provider in `src/llm/`).
- `StubLLMProvider`: concrete in-memory implementation suitable for
  tests. Construct with `new StubLLMProvider(new Map([[match, response]]))`.
- `StubAgentResponse`: canned-response shape for the agent-mode arm
  of `StubLLMProvider`.
- `NotImplementedError`, `OverlayBypassError`, `InvariantFormulationFailed`:
  named error classes the loop throws. Integrators catch these
  specifically rather than parsing strings.

**Standing-runtime gate.**
- `VerifyReport`: what `verifyAll` returns, a list of
  `InvariantVerdict`s plus an aggregate outcome.
- `VerifyOptions`: config for `verifyAll` (which invariants, which
  paths, severity thresholds).
- `InvariantVerdict`: per-invariant outcome. One of `resolved`,
  `violated`, or one of the `decayed_*` kinds.
- `BindingResolution`: how an invariant's named bindings resolved
  against current code (or didn't, hence the `DecayKind`).
- `DecayKind`: `"deleted" | "changed" | "substrate"`.
- `CachedVerifyReport`, `CachedInvariantVerdict`, `CacheEntry`,
  `VerifyEntry`: cache-aware variants. The fingerprint logic for
  cache invalidation lives behind `verifyAllCached`.

**Constraint store.**
- `StoredInvariant`: on-disk JSON shape of an invariant claim.
  `readInvariants` returns these; `writeInvariant` accepts one.
- `InvariantClaim`: in-memory shape `buildStoredInvariant`
  consumes to produce a `StoredInvariant`.

**Path enumeration.**
- `Path`, `PathStep`: the call paths the standing-runtime gate
  enumerates between an invariant binding's "guard" and "use" sites.
- `PathVerdict`: the per-path Z3 result.

**Retrospective intake.**
- `IntentReport`: top-level output of `extractIntent`.
- `IntentReportCitation`, `IntentReportIntent`,
  `IntentReportConstraintCandidate`, `IntentReportOutputBundle`,
  `IntentReportTrigger`: sub-shapes of `IntentReport`.
- `RetrospectiveIntakeInput`: input shape for `extractIntent`.
- `IntentReportSchemaError`, `RetrospectiveIntakeInputError`:
  named error classes thrown on bad input or schema-failure response.

## 4. Semver commitment

ProvekIt is pre-1.0 (currently 0.x). The semver promise on the library
surface for the duration of the 0.x major is:

**What is stable.** Every name re-exported from `src/index.ts` and the
shape of every type re-exported from `src/index.ts`. Adding optional
fields to existing types is a minor bump. Adding a new export is a
minor bump.

**What is not stable.**
- Anything not exported from `src/index.ts`. The internal package
  layout WILL move; integrators reaching into `provekit/src/fix/...`
  do so at their own risk.
- Performance characteristics. A minor bump may make a call faster or
  slower as long as the type contract holds.
- The CLI's flag set within a major. The CLI's commands (`provekit verify`,
  `provekit fix`, etc.) are stable; their flags may grow.

**Breaking changes.** Removing a name from `src/index.ts`, narrowing
an existing type, or changing a function signature is a major bump
(0.x → 0.(x+1) during pre-1.0; 1.x → 2.x post-1.0). 0.x → 0.(x+1)
breaks are flagged in the changelog with a `BREAKING:` prefix and a
migration note.

**Pre-1.0 caveat.** Until 1.0, minor bumps may include a small number
of breaking renames if the underlying contract was wrong. The
changelog and the deprecation period (one minor cycle) are how we
communicate this. Post-1.0, the surface freezes harder.

## 5. What's NOT in the surface

If you need a name and it isn't exported, the answer is almost
certainly: it's internal, and we'll likely refuse to add it without
a strong use-case. Open an issue describing what you're trying to
accomplish; in most cases the right answer is either "compose the
existing surface differently" or "we'll add a higher-level entry
point that does this without exposing the internals."

Specifically not in the library surface:
- Per-IDE plugins. We do not ship a VS Code extension, a JetBrains
  plugin, or a Vim integration. Channel 2 is the surface; integrators
  build the plugin.
- Slack bots, Linear webhooks, GitHub Issues subscribers, email
  connectors. These are downstream of CLI + library; we do not ship
  or maintain them.
- The internal SAST schema. The substrate database is an
  implementation detail of `verifyAll`; we reserve the right to
  rebuild it.
- The LLM prompts. They live in `src/fix/prompts/` and
  `src/fix/stages/*`. They are not stable inputs.

If a future use-case warrants a new public name (for example, a
streaming variant of `runFixLoop`, or a memory-backed `Db` factory
for testing), file an issue. We promise to take it seriously; we do
not promise to take it.
